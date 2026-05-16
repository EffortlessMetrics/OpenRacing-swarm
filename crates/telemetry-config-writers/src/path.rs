use std::path::{Path, PathBuf};

/// Resolves a game-relative path, specially handling the "Documents/" prefix for Windows.
pub(crate) fn resolve_game_path(game_path: &Path, relative_path: &str) -> PathBuf {
    // If a non-empty game_path is provided, respect it.
    // This is critical for tests using TempDir to avoid overwriting real user files.
    if !game_path.as_os_str().is_empty() && game_path != Path::new(".") {
        return game_path.join(relative_path);
    }

    #[cfg(windows)]
    if let Some(stripped) = relative_path.strip_prefix("Documents/") {
        // Try to use USERPROFILE/Documents as the base on Windows
        if let Some(user_profile) = std::env::var_os("USERPROFILE") {
            let mut path = PathBuf::from(user_profile);
            path.push("Documents");
            return path.join(stripped.replace('/', "\\"));
        }
    }
    game_path.join(relative_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_game_path_with_nonempty_base() {
        let base = Path::new("test_base");

        // Non-empty base always joins directly, even for Documents/ paths
        let res = resolve_game_path(base, "Config/app.ini");
        assert_eq!(res, base.join("Config/app.ini"));

        let res = resolve_game_path(base, "Documents/MyGame/app.ini");
        assert_eq!(res, base.join("Documents/MyGame/app.ini"));
    }

    #[test]
    fn test_resolve_game_path_plain_relative() {
        // "." base with a non-Documents path just joins
        let base = Path::new(".");
        let res = resolve_game_path(base, "Config/app.ini");
        assert_eq!(res, base.join("Config/app.ini"));
    }

    /// When game_path is "." (current dir), Documents/ paths on Windows
    /// should resolve to USERPROFILE/Documents/… instead of ./Documents/…
    #[cfg(windows)]
    #[test]
    fn test_resolve_game_path_documents_resolves_via_userprofile() {
        let base = Path::new(".");
        let res = resolve_game_path(base, "Documents/MyGame/app.ini");

        if let Some(user_profile) = std::env::var_os("USERPROFILE") {
            let expected = PathBuf::from(user_profile).join("Documents\\MyGame\\app.ini");
            assert_eq!(res, expected);
        }
    }
}
