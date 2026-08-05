//! Incremental reading of a growing file. Line contents are never interpreted.

use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// A hash of this many leading bytes distinguishes a new `Client.txt` from the
/// previous one even when the new file is longer. Truncation is not caught this
/// way — size handles that.
pub const FINGERPRINT_BYTES: usize = 512;

/// Hands out new complete lines and reports that the file was rotated.
///
/// Detecting rotation is this type's job; deciding what to do about it — start a
/// new generation in the store — belongs to the caller.
pub struct LogTail {
    path: PathBuf,
    pub offset: u64,
    pub fingerprint: Option<String>,
    pub rotated: bool,
}

impl LogTail {
    pub fn new(path: &Path, offset: u64, fingerprint: Option<String>) -> LogTail {
        LogTail {
            path: path.to_path_buf(),
            offset,
            fingerprint,
            rotated: false,
        }
    }

    fn head_fingerprint(head: &[u8]) -> Option<String> {
        // While the file is shorter than FINGERPRINT_BYTES its hash changes with
        // every append, and a changing hash would read as endless rotation.
        if head.len() < FINGERPRINT_BYTES {
            return None;
        }
        Some(format!("{:x}", Sha256::digest(head)))
    }

    fn is_rotated(&self, size: u64, fingerprint: &Option<String>) -> bool {
        if size < self.offset {
            return true;
        }
        // A replacement longer than the old offset is invisible by size alone.
        match (fingerprint, &self.fingerprint) {
            (Some(new), Some(old)) => new != old,
            _ => false,
        }
    }

    /// New complete lines as `(offset just past the line, line)`.
    ///
    /// Any I/O error yields an empty result rather than propagating: the game can
    /// delete or recreate the file between the metadata call and the read, and
    /// skipping one poll is safer than killing the update thread.
    pub fn read_new(&mut self) -> Vec<(u64, String)> {
        self.rotated = false;

        let Ok(metadata) = std::fs::metadata(&self.path) else {
            return Vec::new();
        };
        let size = metadata.len();

        let Ok(mut file) = File::open(&self.path) else {
            return Vec::new();
        };

        let mut head = vec![0u8; FINGERPRINT_BYTES];
        let read = match file.read(&mut head) {
            Ok(n) => n,
            Err(_) => return Vec::new(),
        };
        head.truncate(read);
        let fingerprint = Self::head_fingerprint(&head);

        if self.is_rotated(size, &fingerprint) {
            self.offset = 0;
            self.rotated = true;
        }
        // Assigned unconditionally, None included: holding on to the previous
        // file's fingerprint would make a later, longer file look like yet
        // another rotation.
        self.fingerprint = fingerprint;

        if size <= self.offset {
            return Vec::new();
        }
        if file.seek(SeekFrom::Start(self.offset)).is_err() {
            return Vec::new();
        }
        let mut data = Vec::with_capacity((size - self.offset) as usize);
        if file
            .take(size - self.offset)
            .read_to_end(&mut data)
            .is_err()
        {
            return Vec::new();
        }

        let Some(last_newline) = data.iter().rposition(|b| *b == b'\n') else {
            // Not one complete line yet — wait for the game to finish writing.
            return Vec::new();
        };
        let complete = &data[..=last_newline];

        let mut result = Vec::new();
        let mut position = self.offset;
        for raw in complete.split(|b| *b == b'\n') {
            if position + raw.len() as u64 >= self.offset + complete.len() as u64 {
                break;
            }
            position += raw.len() as u64 + 1;
            let line = String::from_utf8_lossy(raw)
                .trim_end_matches('\r')
                .to_string();
            result.push((position, line));
        }
        self.offset += complete.len() as u64;
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    /// A unique temp file per test: these run in parallel and a shared name
    /// would have them clobbering each other's log.
    fn temp_path(name: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("poe2-tail-{name}-{}.txt", std::process::id()));
        let _ = fs::remove_file(&p);
        p
    }

    fn append(path: &std::path::Path, text: &str) {
        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();
        f.write_all(text.as_bytes()).unwrap();
    }

    #[test]
    fn reads_only_complete_lines() {
        let path = temp_path("complete");
        append(&path, "first\nsecond\npartial");
        let mut tail = LogTail::new(&path, 0, None);
        let lines: Vec<String> = tail.read_new().into_iter().map(|(_, l)| l).collect();
        assert_eq!(lines, vec!["first", "second"]);

        // The partial line arrives only once its newline does.
        append(&path, " finished\n");
        let lines: Vec<String> = tail.read_new().into_iter().map(|(_, l)| l).collect();
        assert_eq!(lines, vec!["partial finished"]);
        fs::remove_file(&path).ok();
    }

    #[test]
    fn offsets_point_past_each_line() {
        let path = temp_path("offsets");
        append(&path, "ab\ncd\n");
        let mut tail = LogTail::new(&path, 0, None);
        let offsets: Vec<u64> = tail.read_new().into_iter().map(|(o, _)| o).collect();
        assert_eq!(offsets, vec![3, 6]);
        assert_eq!(tail.offset, 6);
        fs::remove_file(&path).ok();
    }

    #[test]
    fn nothing_new_yields_nothing() {
        let path = temp_path("nothing");
        append(&path, "one\n");
        let mut tail = LogTail::new(&path, 0, None);
        assert_eq!(tail.read_new().len(), 1);
        assert!(tail.read_new().is_empty());
        assert!(!tail.rotated);
        fs::remove_file(&path).ok();
    }

    #[test]
    fn missing_file_is_not_an_error() {
        let path = temp_path("missing");
        let mut tail = LogTail::new(&path, 0, None);
        assert!(tail.read_new().is_empty());
        assert!(!tail.rotated);
    }

    #[test]
    fn truncation_is_detected_and_restarts_from_zero() {
        let path = temp_path("truncate");
        append(&path, "aaaa\nbbbb\n");
        let mut tail = LogTail::new(&path, 0, None);
        tail.read_new();
        assert_eq!(tail.offset, 10);

        fs::write(&path, "cc\n").unwrap();
        let lines: Vec<String> = tail.read_new().into_iter().map(|(_, l)| l).collect();
        assert!(tail.rotated, "a shorter file must read as rotation");
        assert_eq!(lines, vec!["cc"]);
        fs::remove_file(&path).ok();
    }

    #[test]
    fn replacement_of_the_same_length_is_caught_by_the_fingerprint() {
        // Size alone cannot see this: the new file is exactly as long as what we
        // had already read. Only the fingerprint of its head differs.
        let path = temp_path("replace");
        let old = "A".repeat(FINGERPRINT_BYTES) + "\n";
        let new = "B".repeat(FINGERPRINT_BYTES) + "\n";
        fs::write(&path, &old).unwrap();
        let mut tail = LogTail::new(&path, 0, None);
        tail.read_new();
        assert!(!tail.rotated);

        fs::write(&path, &new).unwrap();
        tail.read_new();
        assert!(
            tail.rotated,
            "same size, different content, must be rotation"
        );
        fs::remove_file(&path).ok();
    }

    #[test]
    fn a_file_shorter_than_the_fingerprint_gets_none() {
        let path = temp_path("short");
        append(&path, "tiny\n");
        let mut tail = LogTail::new(&path, 0, None);
        tail.read_new();
        // While the file is this short, appending would change any hash of its
        // head, and a changing hash would read as endless rotation.
        assert_eq!(tail.fingerprint, None);
        append(&path, "more\n");
        tail.read_new();
        assert!(!tail.rotated, "growth of a short file is not rotation");
        fs::remove_file(&path).ok();
    }

    #[test]
    fn invalid_utf8_is_replaced_not_fatal() {
        let path = temp_path("utf8");
        fs::write(&path, b"good\n\xff\xfe bad\n").unwrap();
        let mut tail = LogTail::new(&path, 0, None);
        let lines: Vec<String> = tail.read_new().into_iter().map(|(_, l)| l).collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "good");
        assert!(lines[1].contains("bad"));
        fs::remove_file(&path).ok();
    }

    #[test]
    fn carriage_returns_are_stripped() {
        let path = temp_path("crlf");
        append(&path, "windows\r\nline\r\n");
        let mut tail = LogTail::new(&path, 0, None);
        let lines: Vec<String> = tail.read_new().into_iter().map(|(_, l)| l).collect();
        assert_eq!(lines, vec!["windows", "line"]);
        fs::remove_file(&path).ok();
    }
}
