use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)] // variants wired up in next commit
pub enum PackageManager {
    Npm,
    Pnpm,
    Yarn,
    Bun,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct BiomeSetup {
    pub config_path: PathBuf,
    pub binary_path: PathBuf,
    pub version: String,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DetectedProject {
    pub start: PathBuf,
    pub repo_root: Option<PathBuf>,
    pub package_manager: Option<(PackageManager, PathBuf)>,
    pub biome: Option<BiomeSetup>,
}
