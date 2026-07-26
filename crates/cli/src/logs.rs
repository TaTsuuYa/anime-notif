//! Reading and following anime-notif's log file (`anime-notif logs`).
//!
//! The daemon writes logs to a daily-rotating file under
//! [`anime_notif_core::paths::default_log_dir`], in addition to stdout (so
//! systemd/journald keeps capturing it as before). This module lets the
//! CLI show that file directly, which works the same regardless of the
//! host's init system — unlike `journalctl`, which only exists on
//! systemd-based Linux, this works on Windows/macOS too.

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::error::CliError;

/// Prefix `anime_notif_daemon`'s file logging (set up in the `anime-notif`
/// binary) rotates under; the daily roller names files
/// `<FILE_PREFIX>.<YYYY-MM-DD>`.
pub const FILE_PREFIX: &str = "anime-notif.log";

/// Finds the most recently-dated log file in `dir`. File names sort
/// chronologically as plain strings (`YYYY-MM-DD` is zero-padded), so the
/// lexicographically-largest match is the current one.
pub fn find_current_log_file(dir: &Path) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(FILE_PREFIX))
        })
        .collect();
    candidates.sort();
    candidates.pop()
}

/// Returns the last `lines` lines of `path`, newline-terminated.
pub fn read_tail(path: &Path, lines: usize) -> Result<String, CliError> {
    let content = std::fs::read_to_string(path).map_err(|source| CliError::LogIo {
        path: path.to_path_buf(),
        source,
    })?;
    let all_lines: Vec<&str> = content.lines().collect();
    let start = all_lines.len().saturating_sub(lines);
    let mut out = all_lines[start..].join("\n");
    out.push('\n');
    Ok(out)
}

/// Polls `path` forever, calling `on_line` for each new line as it's
/// appended (like `tail -f`) — never returns except on an I/O error.
///
/// Doesn't follow log rotation: if the daemon rolls over to a new day's
/// file while this is running, new lines stop appearing here (restart
/// `logs --follow` to pick up the new file). Reads at the byte level and
/// only decodes complete lines, so a write straddling a read boundary
/// can't produce a spurious UTF-8 error.
pub fn follow(path: &Path, mut on_line: impl FnMut(&str)) -> Result<(), CliError> {
    let io_err = |source: std::io::Error| CliError::LogIo {
        path: path.to_path_buf(),
        source,
    };

    let mut file = std::fs::File::open(path).map_err(io_err)?;
    file.seek(SeekFrom::End(0)).map_err(io_err)?;

    let mut pending: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match file.read(&mut chunk) {
            Ok(0) => std::thread::sleep(Duration::from_millis(300)),
            Ok(n) => {
                pending.extend_from_slice(&chunk[..n]);
                while let Some(idx) = pending.iter().position(|&b| b == b'\n') {
                    let line_bytes: Vec<u8> = pending.drain(..=idx).collect();
                    let line = String::from_utf8_lossy(&line_bytes[..line_bytes.len() - 1]);
                    on_line(&line);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(source) => return Err(io_err(source)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_current_log_file_picks_the_latest_date() {
        let dir = tempfile::tempdir().unwrap();
        for name in [
            "anime-notif.log.2026-07-24",
            "anime-notif.log.2026-07-26",
            "anime-notif.log.2026-07-25",
        ] {
            std::fs::write(dir.path().join(name), "x").unwrap();
        }
        // An unrelated file must never be picked up.
        std::fs::write(dir.path().join("control_token"), "x").unwrap();

        let found = find_current_log_file(dir.path()).unwrap();
        assert_eq!(found.file_name().unwrap(), "anime-notif.log.2026-07-26");
    }

    #[test]
    fn find_current_log_file_none_when_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert!(find_current_log_file(dir.path()).is_none());
    }

    #[test]
    fn find_current_log_file_none_when_dir_missing() {
        assert!(find_current_log_file(Path::new("/does/not/exist")).is_none());
    }

    #[test]
    fn read_tail_returns_last_n_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("log");
        std::fs::write(&path, "one\ntwo\nthree\nfour\nfive\n").unwrap();

        assert_eq!(read_tail(&path, 2).unwrap(), "four\nfive\n");
        assert_eq!(
            read_tail(&path, 100).unwrap(),
            "one\ntwo\nthree\nfour\nfive\n"
        );
    }

    #[test]
    fn read_tail_missing_file_is_an_error() {
        let err = read_tail(Path::new("/does/not/exist"), 10).unwrap_err();
        assert!(matches!(err, CliError::LogIo { .. }));
    }

    #[test]
    fn follow_reports_only_lines_appended_after_it_starts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("log");
        std::fs::write(&path, "old line\n").unwrap();

        let path_for_writer = path.clone();
        let writer = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(&path_for_writer)
                .unwrap();
            writeln!(f, "new line one").unwrap();
            writeln!(f, "new line two").unwrap();
        });

        let (tx, rx) = std::sync::mpsc::channel();
        let path_for_follow = path.clone();
        std::thread::spawn(move || {
            let _ = follow(&path_for_follow, |line| {
                let _ = tx.send(line.to_string());
            });
        });

        let first = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let second = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(first, "new line one");
        assert_eq!(second, "new line two");
        writer.join().unwrap();
    }
}
