use super::PackageManager;
use std::path::{Path, PathBuf};

impl PackageManager {
    pub fn lockfile_name(&self) -> &'static str {
        match self {
            PackageManager::Bun => "bun.lockb",
            PackageManager::Pnpm => "pnpm-lock.yaml",
            PackageManager::Yarn => "yarn.lock",
            PackageManager::Npm => "package-lock.json",
        }
    }
}

/// Priority order when multiple lockfiles coexist at the same level.
const PRIORITY: [PackageManager; 4] = [
    PackageManager::Bun,
    PackageManager::Pnpm,
    PackageManager::Yarn,
    PackageManager::Npm,
];

/// Walk up from `start` (directory) looking for a lockfile. At each level,
/// lockfiles are checked in `PRIORITY` order. The first hit at the nearest
/// level wins.
pub fn detect_package_manager(start: &Path) -> Option<(PackageManager, PathBuf)> {
    let mut current = start;
    loop {
        for pm in &PRIORITY {
            let probe = current.join(pm.lockfile_name());
            if probe.exists() {
                return Some((pm.clone(), probe));
            }
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn setup() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn detects_npm() {
        let t = setup();
        fs::write(t.path().join("package-lock.json"), "").unwrap();
        let (pm, _) = detect_package_manager(t.path()).unwrap();
        assert_eq!(pm, PackageManager::Npm);
    }

    #[test]
    fn detects_pnpm() {
        let t = setup();
        fs::write(t.path().join("pnpm-lock.yaml"), "").unwrap();
        let (pm, _) = detect_package_manager(t.path()).unwrap();
        assert_eq!(pm, PackageManager::Pnpm);
    }

    #[test]
    fn detects_yarn() {
        let t = setup();
        fs::write(t.path().join("yarn.lock"), "").unwrap();
        let (pm, _) = detect_package_manager(t.path()).unwrap();
        assert_eq!(pm, PackageManager::Yarn);
    }

    #[test]
    fn detects_bun() {
        let t = setup();
        fs::write(t.path().join("bun.lockb"), "").unwrap();
        let (pm, _) = detect_package_manager(t.path()).unwrap();
        assert_eq!(pm, PackageManager::Bun);
    }

    #[test]
    fn bun_wins_over_pnpm_at_same_level() {
        let t = setup();
        fs::write(t.path().join("bun.lockb"), "").unwrap();
        fs::write(t.path().join("pnpm-lock.yaml"), "").unwrap();
        let (pm, _) = detect_package_manager(t.path()).unwrap();
        assert_eq!(pm, PackageManager::Bun);
    }

    #[test]
    fn pnpm_wins_over_yarn_at_same_level() {
        let t = setup();
        fs::write(t.path().join("pnpm-lock.yaml"), "").unwrap();
        fs::write(t.path().join("yarn.lock"), "").unwrap();
        let (pm, _) = detect_package_manager(t.path()).unwrap();
        assert_eq!(pm, PackageManager::Pnpm);
    }

    #[test]
    fn yarn_wins_over_npm_at_same_level() {
        let t = setup();
        fs::write(t.path().join("yarn.lock"), "").unwrap();
        fs::write(t.path().join("package-lock.json"), "").unwrap();
        let (pm, _) = detect_package_manager(t.path()).unwrap();
        assert_eq!(pm, PackageManager::Yarn);
    }

    #[test]
    fn nearest_ancestor_wins() {
        let t = setup();
        fs::write(t.path().join("pnpm-lock.yaml"), "").unwrap();
        let child = t.path().join("packages/foo");
        fs::create_dir_all(&child).unwrap();
        fs::write(child.join("package-lock.json"), "").unwrap();

        let (pm, path) = detect_package_manager(&child).unwrap();
        assert_eq!(pm, PackageManager::Npm);
        assert!(path.starts_with(&child));
    }
}
