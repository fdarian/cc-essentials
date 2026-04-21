use crate::fs_util::walk_up_for;
use std::path::{Path, PathBuf};

/// Find the nearest `biome.json` or `biome.jsonc` walking up from `start`.
/// At the same directory level, `biome.json` is preferred over `biome.jsonc`.
pub fn find_biome_config(start: &Path) -> Option<PathBuf> {
    walk_up_for(start, &["biome.json", "biome.jsonc"])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn finds_biome_json_in_ancestor() {
        let t = tempfile::tempdir().unwrap();
        fs::write(t.path().join("biome.json"), "{}").unwrap();
        let child = t.path().join("src");
        fs::create_dir(&child).unwrap();
        let found = find_biome_config(&child).unwrap();
        assert!(found.ends_with("biome.json"));
    }

    #[test]
    fn finds_biome_jsonc_when_only_jsonc_exists() {
        let t = tempfile::tempdir().unwrap();
        fs::write(t.path().join("biome.jsonc"), "{}").unwrap();
        let found = find_biome_config(t.path()).unwrap();
        assert!(found.ends_with("biome.jsonc"));
    }

    #[test]
    fn prefers_biome_json_over_jsonc_at_same_level() {
        let t = tempfile::tempdir().unwrap();
        fs::write(t.path().join("biome.json"), "{}").unwrap();
        fs::write(t.path().join("biome.jsonc"), "{}").unwrap();
        let found = find_biome_config(t.path()).unwrap();
        assert!(found.ends_with("biome.json"));
    }

    #[test]
    fn nested_config_wins_over_ancestor() {
        let t = tempfile::tempdir().unwrap();
        fs::write(t.path().join("biome.json"), r#"{"root":true}"#).unwrap();
        let child = t.path().join("packages/foo");
        fs::create_dir_all(&child).unwrap();
        fs::write(child.join("biome.json"), r#"{"extends":"//"}"#).unwrap();

        let found = find_biome_config(&child).unwrap();
        assert_eq!(fs::read_to_string(&found).unwrap(), r#"{"extends":"//"}"#);
    }

    #[test]
    fn returns_none_when_no_config_exists() {
        let t = tempfile::tempdir().unwrap();
        assert!(find_biome_config(t.path()).is_none());
    }
}
