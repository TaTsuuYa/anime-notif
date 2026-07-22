//! Filesystem-safe filename construction.

/// Replaces characters that are unsafe or awkward in filenames (path
/// separators, control characters, characters reserved on Windows) with
/// `_`, and trims surrounding whitespace/dots.
pub fn sanitize(stem: &str) -> String {
    let cleaned: String = stem
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches(|c: char| c.is_whitespace() || c == '.');
    if trimmed.is_empty() {
        "download".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_unsafe_characters() {
        assert_eq!(
            sanitize("One Piece: Episode/1121 [1080p]"),
            "One Piece_ Episode_1121 [1080p]"
        );
    }

    #[test]
    fn empty_input_gets_a_fallback_name() {
        assert_eq!(sanitize("   ..  "), "download");
    }

    #[test]
    fn ordinary_names_are_untouched() {
        assert_eq!(
            sanitize("One Piece - 1121 - 1080p"),
            "One Piece - 1121 - 1080p"
        );
    }
}
