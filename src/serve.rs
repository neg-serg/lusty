//! Headless backend for the nvim shim.
//!
//! Runs one listing in memory and answers plain-text requests on stdin, one
//! per line, so the nvim side can render results in a normal floating window
//! (no terminal buffer involved):
//!
//!   E                     -> "C <total> <depth> <root>"  (ready)
//!   Q <from> <to> <query> [sort] [dirs] [rev]
//!                            -> "N <matched>", "W <maxw>", then
//!                               "R <i> <kind> <label>\t<path>" rows for
//!                               ranked indices in [from,to), then "E".
//!                               sort is optional (default 0 = the canonical
//!                               depth+name order): 1 ext, 2 size desc,
//!                               3 time desc. dirs/rev are 0/1 (default 0)
//!                               and mirror the standalone TUI: they only
//!                               reshape the canonical order (sort 0) —
//!                               dirs groups directories first per depth,
//!                               rev reverses each depth group.
//!   M <mask> <index>...    -> "K <index> <meta>" per entry index, then "E".
//!                            <meta> is the eza -l field block selected by
//!                            mask bits (1 perm, 2 user, 4 size, 8 time),
//!                            empty when the stat fails. The client asks only
//!                            for rows currently visible in the float.
//!   P <ranked-index>       -> "P <absolute path>"
//!
//! kind is one of d/f/l (dir/file/link). Lines are '\n'-terminated; labels
//! and metadata are raw (no ANSI). Backslash, TAB and LF inside a label or
//! path are escaped as `\\`, `\t`, `\n` so a file name containing them cannot
//! break the framing; the nvim client reverses this. The process exits on
//! stdin EOF.

use std::io::{self, BufRead, Write};
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;

use crate::cache;
use crate::listing::{Entry, FileKind, Options};
use crate::rank;

/// Re-sort the listing by an eza-style key. The sort helpers expect the
/// canonical (depth, name) order and sort per depth group, so the caller
/// restores that order from the base snapshot before every change.
fn apply_sort(entries: &mut Vec<Entry>, root: &std::path::Path, sort: u8) {
    match sort {
        1 => crate::listing::sort_by_ext(entries),
        2 => crate::listing::sort_by_meta(root, entries, false),
        3 => crate::listing::sort_by_meta(root, entries, true),
        _ => {}
    }
}

/// Escape a raw byte string for the line protocol: backslash, TAB and LF
/// become the two-byte sequences `\\`, `\t`, `\n`, so a file name containing
/// them cannot break the request/response framing. The nvim client reverses
/// this (`unescape` in native.lua). Bytes, not `str`: names may be non-UTF8.
fn write_escaped(out: &mut impl Write, bytes: &[u8]) -> io::Result<()> {
    for &b in bytes {
        match b {
            b'\\' => out.write_all(b"\\\\")?,
            b'\t' => out.write_all(b"\\t")?,
            b'\n' => out.write_all(b"\\n")?,
            _ => out.write_all(&[b])?,
        }
    }
    Ok(())
}

pub fn serve(
    root: PathBuf,
    depth: usize,
    skip_dirs: Vec<String>,
    show_dots: bool,
) -> io::Result<()> {
    let opts = Options {
        depth,
        skip_dirs,
        follow_mounts: false,
        show_dots,
    };
    // Canonical listing; sorting (Q sort token) reorders it in place and
    // a pristine snapshot lets later changes start from the (depth, name)
    // order again instead of stacking.
    let mut entries = cache::cached_list(&root, &opts);
    let mut base: Option<Vec<Entry>> = None;

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    if let Ok(db) = std::env::var("LUSTY_SERVE_DEBUG") {
        let _ = std::fs::write(&db, format!("entries={} C-about-to-print", entries.len()));
    }
    writeln!(out, "C {} {} {}", entries.len(), depth, root.display())?;
    out.flush()?;
    if let Ok(db) = std::env::var("LUSTY_SERVE_DEBUG") {
        let mut f = std::fs::OpenOptions::new().append(true).open(&db).unwrap();
        use std::io::Write as _;
        let _ = f.write_all(b" C-printed\n");
    }

    // Memoize the most recent query: navigation and redraws resend the
    // same query, and re-ranking per arrow key on huge listings is waste.
    // memo_hit tracks "a ranking exists": the first request is always the
    // empty query, which would otherwise equal the initial memo_q and never
    // fill the memo (a fresh picker would list nothing until the first key).
    let mut memo_q = String::new();
    let mut memo_sort = 0u8;
    let mut memo_dirs = false;
    let mut memo_rev = false;
    let mut memo_hit = false;
    let mut memo_ranked: Vec<usize> = Vec::new();
    let mut memo_maxw: usize = 0;
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        // Queries never contain tabs; full split so Q can carry an optional
        // sort token after the query.
        let parts: Vec<&str> = line.split('\t').collect();
        match parts[0] {
            "Q" => {
                let from: usize = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
                let to: usize = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
                let query = parts.get(3).unwrap_or(&"").to_string();
                let sort: u8 = parts
                    .get(4)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0)
                    .min(3);
                let dirs_first: bool = parts.get(5).map(|s| *s == "1").unwrap_or(false);
                let reverse: bool = parts.get(6).map(|s| *s == "1").unwrap_or(false);
                if !memo_hit
                    || query != memo_q
                    || sort != memo_sort
                    || dirs_first != memo_dirs
                    || reverse != memo_rev
                {
                    memo_hit = true;
                    if sort != memo_sort || dirs_first != memo_dirs || reverse != memo_rev {
                        if let Some(b) = &base {
                            entries = b.clone();
                        }
                        if sort != 0 {
                            if base.is_none() {
                                base = Some(entries.clone());
                            }
                            apply_sort(&mut entries, &root, sort);
                        } else if dirs_first || reverse {
                            if base.is_none() {
                                base = Some(entries.clone());
                            }
                            crate::listing::reorder(&mut entries, dirs_first, reverse);
                        }
                        memo_sort = sort;
                        memo_dirs = dirs_first;
                        memo_rev = reverse;
                    }
                    let (ranked, maxw) = if query.is_empty() {
                        let mw = entries
                            .iter()
                            .map(|e| e.label.chars().count())
                            .max()
                            .unwrap_or(0);
                        ((0..entries.len()).collect(), mw)
                    } else {
                        rank::rank_indices_mw(&entries, &query)
                    };
                    memo_q = query.clone();
                    memo_ranked = ranked;
                    memo_maxw = maxw;
                }
                let ranked = &memo_ranked;
                writeln!(out, "N {}", ranked.len())?;
                writeln!(out, "W {}", memo_maxw)?;
                let end = to.min(ranked.len());
                for &i in &ranked[from.min(ranked.len())..end] {
                    let e = &entries[i];
                    let kind = match e.kind {
                        FileKind::Dir => 'd',
                        FileKind::Link => 'l',
                        _ => 'f',
                    };
                    write!(out, "R {} {} ", i, kind)?;
                    write_escaped(&mut out, e.label.as_bytes())?;
                    out.write_all(b"\t")?;
                    write_escaped(&mut out, e.path(&root).as_os_str().as_bytes())?;
                    out.write_all(b"\n")?;
                }
                writeln!(out, "E")?;
                out.flush()?;
            }
            "D" => {
                // top-level directories (depth 1) for '/' completion
                for e in &entries {
                    if e.depth == 1 && e.kind == FileKind::Dir {
                        out.write_all(b"D ")?;
                        write_escaped(&mut out, e.raw_basename())?;
                        out.write_all(b"\n")?;
                    }
                }
                writeln!(out, "E")?;
                out.flush()?;
            }
            "M" => {
                // Metadata for the visible rows only: mask first, then entry
                // indices (the R rows' <i> field). One stat per index, no
                // ranking involved. Formatting is shared with the standalone
                // TUI (listing::meta_line) so both views agree bit-for-bit.
                let toks: Vec<&str> = line.split('\t').collect();
                let mask: u8 = toks.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
                for tok in toks.iter().skip(2) {
                    let i: usize = match tok.parse() {
                        Ok(i) => i,
                        Err(_) => continue,
                    };
                    if i < entries.len() {
                        let e = &entries[i];
                        let meta =
                            crate::listing::meta_line(&e.path(&root), mask).unwrap_or_default();
                        writeln!(out, "K {} {}", i, meta)?;
                    }
                }
                writeln!(out, "E")?;
                out.flush()?;
            }
            _ => {}
        }
    }
    Ok(())
}
