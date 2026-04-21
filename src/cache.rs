use crate::detect::BiomeSetup;
use crate::fs_util::FileStamp;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheKey {
    pub biome_config_path: PathBuf,
    pub biome_config_stamp: FileStamp,
    pub lockfile_path: Option<PathBuf>,
    pub lockfile_stamp: Option<FileStamp>,
    pub start_dir_canonical: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedDetection {
    pub schema_version: u32,
    pub key: CacheKey,
    pub biome: BiomeSetup,
}

#[derive(Debug, Clone)]
pub struct Cache {
    dir: PathBuf,
}

impl Cache {
    /// Open the cache at the platform cache dir. Creates the directory if missing.
    pub fn open() -> Result<Self> {
        let base = dirs::cache_dir().context("platform cache directory unavailable")?;
        let dir = base.join("cc-essentials").join("detect");
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("create cache dir {}", dir.display()))?;
        Ok(Self { dir })
    }

    /// Open the cache rooted at an explicit directory.
    pub fn open_at(dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    /// Return the cache directory path.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn entry_path(&self, config_path: &Path) -> PathBuf {
        let hex = blake3::hash(config_path.to_string_lossy().as_bytes()).to_hex();
        self.dir.join(format!("{hex}.json"))
    }

    /// Lookup the cached detection. Returns:
    /// - `Ok(Some(entry))` only if schema_version matches, stored `CacheKey` equals the queried key,
    ///   and the binary + config paths still exist on disk.
    /// - `Ok(None)` if the file is missing, unparseable, stale, or paths no longer exist.
    /// - `Err(_)` on unexpected IO errors (e.g. permission denied).
    pub fn lookup(&self, key: &CacheKey) -> Result<Option<CachedDetection>> {
        let path = self.entry_path(&key.biome_config_path);
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => {
                return Err(anyhow::Error::new(e).context(format!("read cache {}", path.display())))
            }
        };
        let entry: CachedDetection = match serde_json::from_slice(&bytes) {
            Ok(e) => e,
            Err(_) => return Ok(None), // treat corrupt cache as miss
        };
        if entry.schema_version != SCHEMA_VERSION {
            return Ok(None);
        }
        if &entry.key != key {
            return Ok(None);
        }
        if !entry.biome.binary_path.exists() {
            return Ok(None);
        }
        if !entry.biome.config_path.exists() {
            return Ok(None);
        }
        Ok(Some(entry))
    }

    /// Store a detection entry. Writes atomically via `tempfile::NamedTempFile::persist`,
    /// so concurrent hooks on the same project race harmlessly (last write wins, no partial files).
    pub fn store(&self, entry: &CachedDetection) -> Result<()> {
        let target = self.entry_path(&entry.key.biome_config_path);
        let bytes = serde_json::to_vec_pretty(entry).context("serialize cache entry")?;
        let mut tmp =
            tempfile::NamedTempFile::new_in(&self.dir).context("create temp cache file")?;
        std::io::Write::write_all(&mut tmp, &bytes).context("write cache tempfile")?;
        tmp.persist(&target)
            .map_err(|e| anyhow::anyhow!("persist cache tempfile: {}", e))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs_util::file_stamp;
    use std::fs;

    fn make_biome_setup(t: &tempfile::TempDir) -> (BiomeSetup, CacheKey) {
        let config = t.path().join("biome.json");
        fs::write(&config, "{}").unwrap();
        let binary = t.path().join("biome");
        fs::write(&binary, "").unwrap();
        let biome = BiomeSetup {
            config_path: config.clone(),
            binary_path: binary,
            version: "1.9.4".to_string(),
        };
        let key = CacheKey {
            biome_config_path: config.clone(),
            biome_config_stamp: file_stamp(&config).unwrap(),
            lockfile_path: None,
            lockfile_stamp: None,
            start_dir_canonical: t.path().to_path_buf(),
        };
        (biome, key)
    }

    fn open_cache(t: &tempfile::TempDir) -> Cache {
        Cache::open_at(t.path().join("cache")).unwrap()
    }

    #[test]
    fn round_trip_store_then_lookup() {
        let t = tempfile::tempdir().unwrap();
        let c = open_cache(&t);
        let (biome, key) = make_biome_setup(&t);
        c.store(&CachedDetection {
            schema_version: SCHEMA_VERSION,
            key: key.clone(),
            biome: biome.clone(),
        })
        .unwrap();
        let hit = c.lookup(&key).unwrap().unwrap();
        assert_eq!(hit.biome.version, "1.9.4");
        assert_eq!(hit.biome.config_path, biome.config_path);
    }

    #[test]
    fn missing_cache_returns_none() {
        let t = tempfile::tempdir().unwrap();
        let c = open_cache(&t);
        let (_biome, key) = make_biome_setup(&t);
        assert!(c.lookup(&key).unwrap().is_none());
    }

    #[test]
    fn stale_config_stamp_invalidates() {
        let t = tempfile::tempdir().unwrap();
        let c = open_cache(&t);
        let (biome, key) = make_biome_setup(&t);
        c.store(&CachedDetection {
            schema_version: SCHEMA_VERSION,
            key: key.clone(),
            biome,
        })
        .unwrap();

        // mutate config → new stamp, same path
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(&key.biome_config_path, r#"{"changed":true}"#).unwrap();
        let new_key = CacheKey {
            biome_config_stamp: file_stamp(&key.biome_config_path).unwrap(),
            ..key
        };
        assert!(c.lookup(&new_key).unwrap().is_none());
    }

    #[test]
    fn missing_binary_invalidates() {
        let t = tempfile::tempdir().unwrap();
        let c = open_cache(&t);
        let (biome, key) = make_biome_setup(&t);
        c.store(&CachedDetection {
            schema_version: SCHEMA_VERSION,
            key: key.clone(),
            biome: biome.clone(),
        })
        .unwrap();
        fs::remove_file(&biome.binary_path).unwrap();
        assert!(c.lookup(&key).unwrap().is_none());
    }

    #[test]
    fn missing_config_invalidates() {
        let t = tempfile::tempdir().unwrap();
        let c = open_cache(&t);
        let (biome, key) = make_biome_setup(&t);
        c.store(&CachedDetection {
            schema_version: SCHEMA_VERSION,
            key: key.clone(),
            biome: biome.clone(),
        })
        .unwrap();
        fs::remove_file(&biome.config_path).unwrap();
        assert!(c.lookup(&key).unwrap().is_none());
    }

    #[test]
    fn schema_version_bump_invalidates() {
        let t = tempfile::tempdir().unwrap();
        let c = open_cache(&t);
        let (biome, key) = make_biome_setup(&t);
        c.store(&CachedDetection {
            schema_version: SCHEMA_VERSION + 1000,
            key: key.clone(),
            biome,
        })
        .unwrap();
        assert!(c.lookup(&key).unwrap().is_none());
    }

    #[test]
    fn corrupt_cache_is_treated_as_miss() {
        let t = tempfile::tempdir().unwrap();
        let c = open_cache(&t);
        let (_biome, key) = make_biome_setup(&t);
        // write garbage to the target path
        let target = c.entry_path(&key.biome_config_path);
        fs::write(&target, "not valid json {{{").unwrap();
        assert!(c.lookup(&key).unwrap().is_none());
    }
}
