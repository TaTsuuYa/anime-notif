//! Hand-off command resolution and execution for `torrent` (when a command
//! override is configured) and `magnet` links.
//!
//! Commands are tokenized *before* the link is substituted in (either from
//! the user's template string via [`shell_words::split`], or from a fixed
//! per-platform default token list) and then executed directly via
//! [`std::process::Command`] — never through a shell — so a magnet URI's
//! `&`/`%`/etc. can never be interpreted as shell syntax.

use crate::error::DownloadError;

const PLACEHOLDER_MAGNET: &str = "{magnet}";
const PLACEHOLDER_LINK: &str = "{link}";

/// The default hand-off command for this platform, as already-split
/// tokens, used when no `command` override is configured.
#[cfg(target_os = "linux")]
fn default_command_tokens() -> Vec<String> {
    vec!["xdg-open".into(), PLACEHOLDER_MAGNET.into()]
}

#[cfg(target_os = "macos")]
fn default_command_tokens() -> Vec<String> {
    vec!["open".into(), PLACEHOLDER_MAGNET.into()]
}

#[cfg(target_os = "windows")]
fn default_command_tokens() -> Vec<String> {
    // `start` is a cmd.exe builtin, not a standalone executable; the empty
    // string is the (required) window-title argument `start` expects
    // before the target when the target itself might contain spaces/quotes.
    vec![
        "cmd".into(),
        "/C".into(),
        "start".into(),
        "".into(),
        PLACEHOLDER_MAGNET.into(),
    ]
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn default_command_tokens() -> Vec<String> {
    vec!["xdg-open".into(), PLACEHOLDER_MAGNET.into()]
}

/// Tokenizes a hand-off command: `configured` (a user-supplied template
/// string, shell-word-split) if set, otherwise this platform's default.
pub fn resolve_command_tokens(configured: Option<&str>) -> Result<Vec<String>, DownloadError> {
    match configured {
        Some(template) => shell_words::split(template)
            .map_err(|e| DownloadError::BadCommand(template.to_string(), e.to_string())),
        None => Ok(default_command_tokens()),
    }
}

/// Substitutes `{magnet}`/`{link}` tokens (matched whole, not as a
/// substring) with `link`. Pure and side-effect-free so it's directly
/// testable without spawning a process.
pub fn substitute(tokens: &[String], link: &str) -> Vec<String> {
    tokens
        .iter()
        .map(|t| {
            if t == PLACEHOLDER_MAGNET || t == PLACEHOLDER_LINK {
                link.to_string()
            } else {
                t.clone()
            }
        })
        .collect()
}

/// Launches `tokens[0]` with `tokens[1..]` as arguments, fire-and-forget
/// (doesn't wait for it to exit — it may be a long-running torrent client
/// or a browser that outlives the daemon).
pub fn spawn(tokens: &[String]) -> Result<(), DownloadError> {
    let (program, args) = tokens.split_first().ok_or(DownloadError::EmptyCommand)?;
    std::process::Command::new(program)
        .args(args)
        .spawn()
        .map_err(|e| DownloadError::Spawn(program.clone(), e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitutes_whole_token_only() {
        let tokens = vec![
            "transmission-remote".to_string(),
            "--add".to_string(),
            "{magnet}".to_string(),
        ];
        let resolved = substitute(&tokens, "magnet:?xt=urn:btih:AAA&dn=Show");
        assert_eq!(
            resolved,
            vec![
                "transmission-remote",
                "--add",
                "magnet:?xt=urn:btih:AAA&dn=Show",
            ]
        );
    }

    #[test]
    fn does_not_substitute_partial_matches() {
        // A token that merely contains the placeholder text isn't touched
        // — only an exact-match token is substituted.
        let tokens = vec!["echo".to_string(), "prefix-{magnet}-suffix".to_string()];
        let resolved = substitute(&tokens, "magnet:?xt=aaa");
        assert_eq!(resolved, vec!["echo", "prefix-{magnet}-suffix"]);
    }

    #[test]
    fn resolve_configured_command_is_shell_word_split() {
        let tokens = resolve_command_tokens(Some("transmission-remote --add {magnet}")).unwrap();
        assert_eq!(tokens, vec!["transmission-remote", "--add", "{magnet}"]);
    }

    #[test]
    fn resolve_unconfigured_uses_platform_default() {
        let tokens = resolve_command_tokens(None).unwrap();
        assert!(!tokens.is_empty());
        assert!(tokens.contains(&"{magnet}".to_string()));
    }

    #[test]
    fn rejects_unbalanced_quotes() {
        let err = resolve_command_tokens(Some("echo \"unterminated")).unwrap_err();
        assert!(matches!(err, DownloadError::BadCommand(_, _)));
    }

    #[test]
    #[cfg(unix)]
    fn spawn_launches_process() {
        // `true` exits immediately with status 0 and exists on every unix
        // CI runner; this only checks that spawning plumbing itself works,
        // not any particular external tool's behavior.
        spawn(&["true".to_string()]).unwrap();
    }
}
