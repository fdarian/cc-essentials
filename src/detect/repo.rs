use std::io;
use std::path::{Path, PathBuf};

/// Walk up from `start` (directory) looking for a `.git` entry (dir or file).
/// Returns the directory containing `.git`.
///
/// A `.git` file (not dir) is produced by git worktrees and submodules — we
/// accept it and still return the containing directory.
pub fn find_git_root(start: &Path) -> io::Result<Option<PathBuf>> {
    let mut current = start;
    loop {
        let dot_git = current.join(".git");
        match std::fs::symlink_metadata(&dot_git) {
            Ok(_) => return Ok(Some(current.to_path_buf())),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => return Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn find_git_root_finds_dir_ancestor() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir(root.join(".git")).unwrap();
        let nested = root.join("a/b");
        fs::create_dir_all(&nested).unwrap();

        let got = find_git_root(&nested).unwrap().unwrap();
        assert_eq!(got, root);
    }

    #[test]
    fn find_git_root_finds_file_ancestor() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::write(root.join(".git"), "gitdir: /elsewhere").unwrap();
        let nested = root.join("pkg");
        fs::create_dir(&nested).unwrap();

        let got = find_git_root(&nested).unwrap().unwrap();
        assert_eq!(got, root);
    }

    #[test]
    fn find_git_root_returns_none_when_absent() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(find_git_root(tmp.path()).unwrap().is_none());
    }
}
