use crate::detect::BiomeSetup;
use crate::fs_util::FileStamp;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
    // Disk backing added in next commit. For now just a marker.
    _private: (),
}

impl Cache {
    pub fn open() -> Result<Self> {
        Ok(Self { _private: () })
    }

    pub fn lookup(&self, _key: &CacheKey) -> Result<Option<CachedDetection>> {
        Ok(None)
    }

    pub fn store(&self, _entry: &CachedDetection) -> Result<()> {
        Ok(())
    }
}
