use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[allow(dead_code)] // used in next commit
pub struct FileStamp {
    pub mtime_unix_nanos: i128,
    pub size: u64,
}

/// Walk up from `start_dir` (must be a directory) checking each `candidate` at
/// each level, returning the first match's absolute path. `candidates` are
/// checked in order at each level (first match wins per level).
#[allow(dead_code)] // used in next commit
pub fn walk_up_for(start_dir: &Path, candidates: &[&str]) -> Option<PathBuf> {
    let mut current = start_dir;
    loop {
        for name in candidates {
            let probe = current.join(name);
            if probe.exists() {
                return Some(probe);
            }
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => return None,
        }
    }
}

#[allow(dead_code)] // used in next commit
pub fn file_stamp(path: &Path) -> io::Result<FileStamp> {
    let meta = std::fs::metadata(path)?;
    let modified = meta.modified()?;
    let duration = modified
        .duration_since(UNIX_EPOCH)
        .map_err(|e| io::Error::other(format!("mtime before unix epoch: {e}")))?;
    Ok(FileStamp {
        mtime_unix_nanos: duration.as_nanos() as i128,
        size: meta.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn walk_up_for_finds_nearest_in_nested_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let nested = root.join("a/b/c");
        fs::create_dir_all(&nested).unwrap();
        fs::write(root.join("marker.txt"), "x").unwrap();

        let found = walk_up_for(&nested, &["marker.txt"]).unwrap();
        assert_eq!(found, root.join("marker.txt"));
    }

    #[test]
    fn walk_up_for_returns_none_when_absent() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(walk_up_for(tmp.path(), &["nope.txt"]).is_none());
    }

    #[test]
    fn walk_up_for_prefers_first_candidate_at_same_level() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("a"), "x").unwrap();
        fs::write(tmp.path().join("b"), "x").unwrap();
        let found = walk_up_for(tmp.path(), &["a", "b"]).unwrap();
        assert!(found.ends_with("a"));

        let found = walk_up_for(tmp.path(), &["b", "a"]).unwrap();
        assert!(found.ends_with("b"));
    }

    #[test]
    fn walk_up_for_nearer_level_wins_over_ancestor() {
        let tmp = tempfile::tempdir().unwrap();
        let nested = tmp.path().join("a/b");
        fs::create_dir_all(&nested).unwrap();
        fs::write(tmp.path().join("marker"), "ancestor").unwrap();
        fs::write(nested.join("marker"), "nearer").unwrap();

        let found = walk_up_for(&nested, &["marker"]).unwrap();
        assert_eq!(fs::read_to_string(found).unwrap(), "nearer");
    }

    #[test]
    fn file_stamp_round_trip_is_stable_for_unchanged_file() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("f");
        fs::write(&p, "x").unwrap();
        let a = file_stamp(&p).unwrap();
        let b = file_stamp(&p).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn file_stamp_reflects_mtime_change_after_rewrite() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("f");
        fs::write(&p, "x").unwrap();
        let a = file_stamp(&p).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(&p, "yy").unwrap();
        let b = file_stamp(&p).unwrap();
        assert_ne!(a, b);
    }
}
