mod biome_bin;
mod biome_config;
mod package_manager;
mod repo;

pub use biome_bin::{find_biome_binary, probe_version};
pub use biome_config::find_biome_config;
pub use package_manager::detect_package_manager;
pub use repo::find_git_root;

use crate::cache::{Cache, CacheKey, CachedDetection, SCHEMA_VERSION};
use crate::fs_util::file_stamp;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PackageManager {
    Npm,
    Pnpm,
    Yarn,
    Bun,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiomeSetup {
    pub config_path: PathBuf,
    pub binary_path: PathBuf,
    pub version: String,
}

#[derive(Debug, Clone)]
pub struct DetectedProject {
    pub start: PathBuf,
    pub repo_root: Option<PathBuf>,
    pub package_manager: Option<(PackageManager, PathBuf)>,
    pub biome: Option<BiomeSetup>,
}

/// Entrypoint: detect repo + package manager + biome setup from a starting path.
/// `start` must be a directory. Callers operating on a file should pass the file's parent.
pub fn detect_from(start: &Path, cache: &Cache) -> Result<DetectedProject> {
    let start = std::fs::canonicalize(start)
        .with_context(|| format!("canonicalize start path {}", start.display()))?;

    let repo_root = find_git_root(&start).context("find git root")?;
    let package_manager = detect_package_manager(&start);

    let biome = resolve_biome(&start, &package_manager, cache)?;

    Ok(DetectedProject {
        start,
        repo_root,
        package_manager,
        biome,
    })
}

fn resolve_biome(
    start: &Path,
    package_manager: &Option<(PackageManager, PathBuf)>,
    cache: &Cache,
) -> Result<Option<BiomeSetup>> {
    let Some(config_path) = find_biome_config(start) else {
        return Ok(None);
    };

    let config_stamp = file_stamp(&config_path).context("stamp biome config")?;
    let lockfile_path = package_manager.as_ref().map(|pm| pm.1.clone());
    let lockfile_stamp = match &lockfile_path {
        Some(p) => Some(file_stamp(p).context("stamp lockfile")?),
        None => None,
    };
    let key = CacheKey {
        biome_config_path: config_path.clone(),
        biome_config_stamp: config_stamp,
        lockfile_path,
        lockfile_stamp,
        start_dir_canonical: start.to_path_buf(),
    };

    if let Some(hit) = cache.lookup(&key)? {
        if hit.schema_version == SCHEMA_VERSION
            && hit.biome.binary_path.exists()
            && hit.biome.config_path.exists()
        {
            return Ok(Some(hit.biome));
        }
    }

    let config_dir = config_path
        .parent()
        .context("biome config has no parent directory")?;
    let Some(binary_path) = find_biome_binary(config_dir).context("find biome binary")? else {
        return Ok(None);
    };
    let version = probe_version(&binary_path).context("probe biome version")?;
    let biome = BiomeSetup {
        config_path: config_path.clone(),
        binary_path,
        version,
    };

    cache.store(&CachedDetection {
        schema_version: SCHEMA_VERSION,
        key,
        biome: biome.clone(),
    })?;

    Ok(Some(biome))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    fn setup_project(biome_json_at_root: bool, nested_biome: bool) -> tempfile::TempDir {
        let t = tempfile::tempdir().unwrap();
        let root = t.path();
        fs::create_dir(root.join(".git")).unwrap();
        fs::write(root.join("pnpm-lock.yaml"), "").unwrap();
        if biome_json_at_root {
            fs::write(root.join("biome.json"), r#"{"root":true}"#).unwrap();
            // stub biome binary under node_modules/.bin
            let bin_dir = root.join("node_modules/.bin");
            fs::create_dir_all(&bin_dir).unwrap();
            make_exec(&bin_dir.join("biome"), "#!/bin/sh\necho 'Version: 1.9.4'\n");
        }
        if nested_biome {
            let pkg = root.join("packages/foo");
            fs::create_dir_all(&pkg).unwrap();
            fs::write(pkg.join("biome.json"), r#"{"extends":"//"}"#).unwrap();
        }
        t
    }

    #[cfg(unix)]
    fn make_exec(p: &Path, body: &str) {
        use std::os::unix::fs::PermissionsExt;
        fs::write(p, body).unwrap();
        let mut perms = fs::metadata(p).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(p, perms).unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn detect_from_monorepo_root_picks_root_biome() {
        let t = setup_project(true, true);
        let cache = Cache::open().unwrap();
        let d = detect_from(t.path(), &cache).unwrap();
        assert!(d.repo_root.is_some());
        assert_eq!(d.package_manager.as_ref().unwrap().0, PackageManager::Pnpm);
        let biome = d.biome.unwrap();
        assert!(biome.config_path.ends_with("biome.json"));
        assert!(biome
            .config_path
            .parent()
            .unwrap()
            .ends_with(t.path().file_name().unwrap()));
        assert_eq!(biome.version, "1.9.4");
    }

    #[test]
    #[cfg(unix)]
    fn detect_from_nested_package_picks_nearest_biome() {
        let t = setup_project(true, true);
        let nested_raw = t.path().join("packages/foo");
        // Canonicalize so macOS /var -> /private/var symlink doesn't break starts_with.
        let nested = std::fs::canonicalize(&nested_raw).unwrap();
        let cache = Cache::open().unwrap();
        let d = detect_from(&nested, &cache).unwrap();
        let biome = d.biome.unwrap();
        // nearest biome.json is in packages/foo
        assert!(biome.config_path.starts_with(&nested));
    }

    #[test]
    fn detect_from_no_biome_returns_none_biome_without_error() {
        let t = tempfile::tempdir().unwrap();
        let cache = Cache::open().unwrap();
        let d = detect_from(t.path(), &cache).unwrap();
        assert!(d.biome.is_none());
    }
}
