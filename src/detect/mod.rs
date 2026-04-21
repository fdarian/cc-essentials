mod biome_bin;
mod biome_config;
mod package_manager;
mod repo;

#[allow(unused_imports)]
pub use biome_bin::{find_biome_binary, probe_version};
#[allow(unused_imports)]
pub use biome_config::find_biome_config;
#[allow(unused_imports)]
pub use package_manager::detect_package_manager;
#[allow(unused_imports)]
pub use repo::find_git_root;

use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)] // variants wired up in future commit
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
