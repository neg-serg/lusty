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
//!   F <score> <path>       -> no reply: frecency record, sent once by the
//!                            client after the banner. With a non-empty map
//!                            the empty query orders the higher-scored paths
//!                            first inside each depth level.
//!   V <index> <w> <h>      -> "V <lines> <dim>", then one "L <text>" per row,
//!                            then "E": preview pane text for the nvim float
//!                            (ANSI stripped, clipped to <w>). The "L " prefix
//!                            keeps a content line equal to "E" from ending
//!                            the response early.
//!
//! The server also prints `X preview` right after the ready banner. The nvim
//! client sends `V` only when it has seen that capability, so a client with an
//! older backend leaves the preview key as a no-op instead of stalling the
//! response FIFO.
//!
//! kind is one of d/f/l (dir/file/link). Lines are '\n'-terminated; labels
//! and metadata are raw (no ANSI). Backslash, TAB and LF inside a label or
//! path are escaped as `\\`, `\t`, `\n` so a file name containing them cannot
//! break the framing; the nvim client reverses this. The process exits on
//! stdin EOF.

use std::collections::HashMap;
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

/// Reverse `write_escaped` for the client→server direction (`F` records).
fn unescape_bytes(s: &str) -> Vec<u8> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' && i + 1 < b.len() {
            match b[i + 1] {
                b't' => out.push(b'\t'),
                b'n' => out.push(b'\n'),
                b'\\' => out.push(b'\\'),
                c => out.push(c),
            }
            i += 2;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    out
}

/// Preview text for an nvim buffer. SGR sequences are kept so the client can
/// turn chafa's colours into extmarks; other ANSI escapes are dropped, TAB
/// becomes a space and other control bytes are removed (`nvim_buf_set_lines`
/// rejects raw LF and renders the rest as garbage). Clipping counts visible
/// characters only, so a kept SGR sequence never eats the row.
fn preview_line(s: &str, width: usize) -> String {
    let mut out = String::with_capacity(s.len().min(width * 4));
    let mut chars = s.chars().peekable();
    let mut vis = 0usize;
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.peek() == Some(&'[') {
                let mut seq = String::from("\x1b[");
                chars.next();
                let mut last = None;
                for n in chars.by_ref() {
                    seq.push(n);
                    if ('\x40'..='\x7e').contains(&n) {
                        last = Some(n);
                        break;
                    }
                }
                if last == Some('m') {
                    out.push_str(&seq);
                }
            } else {
                let _ = chars.next();
            }
            continue;
        }
        if vis >= width {
            break;
        }
        if c == '\t' {
            out.push(' ');
            vis += 1;
        } else if c.is_control() {
            continue;
        } else {
            out.push(c);
            vis += 1;
        }
    }
    out
}

pub fn serve(
    root: PathBuf,
    depth: usize,
    skip_dirs: Vec<String>,
    show_dots: bool,
    follow_mounts: bool,
) -> io::Result<()> {
    let opts = Options {
        depth,
        skip_dirs,
        follow_mounts,
        show_dots,
    };
    // Canonical listing; sorting (Q sort token) reorders it in place and
    // a pristine snapshot lets later changes start from the (depth, name)
    // order again instead of stacking.
    let mut entries = cache::cached_list(&root, &opts);
    let mut base: Option<Vec<Entry>> = None;
    // Frecency journal from the nvim client (absolute path bytes -> score),
    // sent as fire-and-forget `F` records before the first `Q`.
    let mut frec: HashMap<Vec<u8>, f64> = HashMap::new();

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    if let Ok(db) = std::env::var("LUSTY_SERVE_DEBUG") {
        let _ = std::fs::write(&db, format!("entries={} C-about-to-print", entries.len()));
    }
    writeln!(out, "C {} {} {}", entries.len(), depth, root.display())?;
    // Capability line: the nvim client only sends `V` when it has seen it, so
    // an older server (no X line) makes the preview key a no-op.
    writeln!(out, "X preview")?;
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
    // Frecency records can arrive after a Q (the client sends them at startup,
    // but the order is not guaranteed); re-rank when the map grew.
    let mut memo_frec = 0usize;
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        // Queries never contain tabs; full split so Q can carry an optional
        // sort token after the query.
        let parts: Vec<&str> = line.split('\t').collect();
        match parts[0] {
            "F" => {
                // Fire-and-forget frecency record: "F\t<score>\t<escaped path>".
                // No reply, so it does not disturb the handler FIFO on the nvim
                // side; the client sends the whole journal before its first Q.
                let score: f64 = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0.0);
                if score > 0.0 {
                    if let Some(p) = parts.get(2) {
                        frec.insert(unescape_bytes(p), score);
                    }
                }
            }
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
                    || frec.len() != memo_frec
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
                        if frec.is_empty() {
                            let mw = entries
                                .iter()
                                .map(|e| e.label.chars().count())
                                .max()
                                .unwrap_or(0);
                            ((0..entries.len()).collect(), mw)
                        } else {
                            rank::order_by_frecency(&entries, &root, &frec)
                        }
                    } else {
                        rank::rank_indices_mw(&entries, &query)
                    };
                    memo_q = query.clone();
                    memo_ranked = ranked;
                    memo_maxw = maxw;
                    memo_frec = frec.len();
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
            "V" => {
                // Preview pane for the nvim float: "V\t<index>\t<w>\t<h>" ->
                // "V <lines> <dim>", one "L <text>" per row, then "E". The
                // "L " prefix keeps a content line equal to "E" from ending
                // the response early.
                let i: usize = parts
                    .get(1)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(usize::MAX);
                let w: usize = parts
                    .get(2)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(40)
                    .clamp(8, 400);
                let h: usize = parts
                    .get(3)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(12)
                    .clamp(1, 200);
                let pane = if i < entries.len() {
                    let e = &entries[i];
                    crate::preview::render(&e.path(&root), e.kind == FileKind::Dir, w, h)
                } else {
                    crate::preview::Pane {
                        dim: true,
                        lines: Vec::new(),
                    }
                };
                let lines: Vec<String> = pane
                    .lines
                    .iter()
                    .take(h)
                    .map(|l| preview_line(l, w))
                    .collect();
                writeln!(out, "V {} {}", lines.len(), u8::from(pane.dim))?;
                for l in &lines {
                    out.write_all(b"L ")?;
                    out.write_all(l.as_bytes())?;
                    out.write_all(b"\n")?;
                }
                writeln!(out, "E")?;
                out.flush()?;
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::preview_line;

    #[test]
    fn preview_line_keeps_sgr_and_clips_visible_chars() {
        // SGR survives so the client can build extmarks from it.
        assert_eq!(
            preview_line("\x1b[38;5;9mAB\x1b[0mCD", 40),
            "\x1b[38;5;9mAB\x1b[0mCD"
        );
        // Clipping counts visible characters, not escape bytes, and keeps the
        // escapes that precede the cut.
        assert_eq!(preview_line("\x1b[38;5;9mABCD\x1b[0m", 2), "\x1b[38;5;9mAB");
        // Non-SGR CSI (cursor movement) is dropped; TAB becomes a space.
        assert_eq!(preview_line("a\x1b[2K\tb", 40), "a b");
    }
}
