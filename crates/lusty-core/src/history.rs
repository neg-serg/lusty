//! Directory history of one picker session: the stack behind `go_back`,
//! `go_forward`, `go_to_session_start` and the Esc-cancel jolt back home.
//!
//! Ported from `fzf-navigator` (see CREDITS.md), where the same state lives in
//! a `history` file plus a `history_index` counter inside the session tmpdir,
//! with these invariants — all of them asserted below:
//!
//! - a fresh visit truncates the forward tail (browser semantics), so
//!   `back`/`forward` never replay a path the user abandoned;
//! - visiting the directory you are already in is a no-op, so `go_home` on
//!   `$HOME` cannot grow the stack;
//! - `back` at the oldest entry and `forward` at the newest are no-ops that
//!   report `None` instead of wrapping around;
//! - the session start directory is fixed at construction and stays reachable
//!   no matter how far the browsing went.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct DirHistory {
    dirs: Vec<PathBuf>,
    index: usize,
    session_start: PathBuf,
}

impl DirHistory {
    pub fn new(start: PathBuf) -> Self {
        DirHistory {
            dirs: vec![start.clone()],
            index: 0,
            session_start: start,
        }
    }

    /// Directory the picker would show right now.
    pub fn current(&self) -> &Path {
        &self.dirs[self.index]
    }

    /// Where the picker was launched; the anchor for inserted paths.
    pub fn session_start(&self) -> &Path {
        &self.session_start
    }

    /// Number of reachable entries (the forward tail included).
    pub fn len(&self) -> usize {
        self.dirs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.dirs.is_empty()
    }

    pub fn index(&self) -> usize {
        self.index
    }

    /// Reachable entries, oldest first — the `--session-map` dump and the
    /// tests read this.
    pub fn entries(&self) -> &[PathBuf] {
        &self.dirs
    }

    /// Move to `dir`, dropping everything that used to be "forward".
    /// Returns false when `dir` is already the current one (nothing changed).
    pub fn visit(&mut self, dir: PathBuf) -> bool {
        if dir == self.dirs[self.index] {
            return false;
        }
        self.dirs.truncate(self.index + 1);
        self.dirs.push(dir);
        self.index = self.dirs.len() - 1;
        true
    }

    /// One step back; `None` at the oldest entry.
    pub fn back(&mut self) -> Option<&Path> {
        if self.index == 0 {
            return None;
        }
        self.index -= 1;
        Some(&self.dirs[self.index])
    }

    /// One step forward; `None` at the newest entry.
    pub fn forward(&mut self) -> Option<&Path> {
        if self.index + 1 >= self.dirs.len() {
            return None;
        }
        self.index += 1;
        Some(&self.dirs[self.index])
    }

    /// Jump to the launch directory (the ported Esc rule: cancel returns to
    /// where the picker started instead of dropping the whole session).
    /// Returns false when that is where we already are.
    pub fn to_session_start(&mut self) -> bool {
        let start = self.session_start.clone();
        self.visit(start)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn a_fresh_visit_truncates_the_forward_tail() {
        let mut h = DirHistory::new(p("/a"));
        h.visit(p("/a/b"));
        h.visit(p("/a/c"));
        assert_eq!(h.back(), Some(p("/a/b").as_path()));
        assert!(h.visit(p("/x")));
        assert_eq!(h.entries(), [p("/a"), p("/a/b"), p("/x")]);
        assert_eq!(h.index(), 2);
        assert_eq!(h.forward(), None, "the abandoned branch is gone");
    }

    #[test]
    fn visiting_the_current_directory_is_a_no_op() {
        let mut h = DirHistory::new(p("/a"));
        assert!(!h.visit(p("/a")));
        assert_eq!(h.len(), 1);
        h.visit(p("/b"));
        assert!(!h.visit(p("/b")));
        assert_eq!(h.entries(), [p("/a"), p("/b")]);
    }

    #[test]
    fn back_and_forward_stop_at_the_ends() {
        let mut h = DirHistory::new(p("/a"));
        h.visit(p("/b"));
        assert_eq!(h.back(), Some(p("/a").as_path()));
        assert_eq!(h.back(), None);
        assert_eq!(h.index(), 0);
        assert_eq!(h.forward(), Some(p("/b").as_path()));
        assert_eq!(h.forward(), None);
        assert_eq!(h.index(), 1);
    }

    #[test]
    fn the_session_start_survives_a_truncation() {
        let mut h = DirHistory::new(p("/start"));
        h.visit(p("/start/x"));
        h.visit(p("/start/x/y"));
        h.back();
        h.visit(p("/other"));
        assert_eq!(h.entries(), [p("/start"), p("/start/x"), p("/other")]);
        assert!(h.to_session_start());
        assert_eq!(h.current(), p("/start"));
        assert!(!h.to_session_start(), "already there");
        assert_eq!(h.session_start(), p("/start"));
    }
}
