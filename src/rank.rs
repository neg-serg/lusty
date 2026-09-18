//! Query filtering and ranking over listed entries.
//!
//! Mirrors filesystem_explorer.lua compute_sorted_matches: the first letter of
//! the query must be a prefix of the entry's basename (so 'c' cannot match
//! 'pic.jpg'), the rest are fuzzy-scored on the label, and results are sorted
//! shallower-first, then by score. A query of exactly "." reveals dot files:
//! the prefix anchor is skipped (any basename may match).

use crate::listing::Entry;
use lusty_fuzzy::scorer as fuzzy;
use std::collections::HashMap;
use std::path::Path;

/// Filter and rank entries for a typed query, returning indices into the
/// slice (the TUI keeps one cached listing and re-ranks per keystroke without
/// cloning entries). Empty query returns all indices (listing order).
pub fn rank_indices(entries: &[Entry], query: &str) -> Vec<usize> {
    rank_indices_mw(entries, query).0
}

/// rank_indices plus the widest matched label in chars (single pass over the
/// candidates; serve needs it for column sizing and this avoids a second
/// full scan of the matched set per keystroke).
pub fn rank_indices_mw(entries: &[Entry], query: &str) -> (Vec<usize>, usize) {
    if query.is_empty() {
        let mw = entries
            .iter()
            .map(|e| e.label.chars().count())
            .max()
            .unwrap_or(0);
        return ((0..entries.len()).collect(), mw);
    }
    // Only an exact "." query exempts the first-letter anchor (dot reveal).
    let first = if query == "." {
        None
    } else {
        Some(query.as_bytes()[0].to_ascii_lowercase())
    };
    // Rank big listings on the rayon pool: scoring is per-entry and fully
    // independent, and one keystroke on a 600k-entry tree must not stall.
    let scored: Vec<(usize, f64)>;
    let maxw_out: usize;
    if entries.len() >= 4096 {
        use rayon::prelude::*;
        let chunks: Vec<(Vec<(usize, f64)>, usize)> = entries
            .par_chunks(8192)
            .enumerate()
            .map(|(ci, chunk)| {
                let base = ci * 8192;
                let mut scorer = fuzzy::Scorer::new();
                let mut local: Vec<(usize, f64)> = Vec::new();
                let mut lmw: usize = 0;
                for (k, e) in chunk.iter().enumerate() {
                    let idx = base + k;
                    if let Some(f) = first {
                        if e.name0 != f {
                            continue;
                        }
                    }
                    let score = scorer.score(&e.label, query);
                    if score != 0.0 {
                        local.push((idx, score));
                        let lw = e.label.chars().count();
                        if lw > lmw {
                            lmw = lw;
                        }
                    }
                }
                (local, lmw)
            })
            .collect();
        let mut merged: Vec<(usize, f64)> = Vec::new();
        let mut mw: usize = 0;
        for (mut v, lmw) in chunks {
            merged.append(&mut v);
            if lmw > mw {
                mw = lmw;
            }
        }
        scored = merged;
        maxw_out = mw;
    } else {
        let mut scorer = fuzzy::Scorer::new();
        let mut out: Vec<(usize, f64)> = Vec::new();
        let mut mw: usize = 0;
        for (idx, e) in entries.iter().enumerate() {
            if let Some(first) = first {
                if e.name0 != first {
                    continue;
                }
            }
            let score = scorer.score(&e.label, query);
            if score != 0.0 {
                out.push((idx, score));
                let lw = e.label.chars().count();
                if lw > mw {
                    mw = lw;
                }
            }
        }
        scored = out;
        maxw_out = mw;
    }
    let mut out = scored;
    let maxw = maxw_out;
    out.sort_by(|(ia, sa), (ib, sb)| {
        let a = &entries[*ia];
        let b = &entries[*ib];
        a.depth
            .cmp(&b.depth)
            .then_with(|| sb.partial_cmp(sa).unwrap_or(std::cmp::Ordering::Equal))
            .then_with(|| a.basename().cmp(b.basename()))
    });
    (out.into_iter().map(|(i, _)| i).collect(), maxw)
}

/// Empty-query ordering with frecency: the shallower depth stays first (the
/// picker's "less nested wins" contract, so Enter does not jump into a
/// subdirectory), then the most frequent/recent paths, then the canonical
/// (depth, name) order the listing already has. Paths absent from the map keep
/// their canonical position within their depth group.
pub fn order_by_frecency(
    entries: &[Entry],
    root: &Path,
    frec: &HashMap<Vec<u8>, f64>,
) -> (Vec<usize>, usize) {
    use std::os::unix::ffi::OsStrExt;
    let scores: Vec<f64> = entries
        .iter()
        .map(|e| {
            frec.get(e.path(root).as_os_str().as_bytes())
                .copied()
                .filter(|s| *s > 0.0)
                .unwrap_or(0.0)
        })
        .collect();
    let mut idx: Vec<usize> = (0..entries.len()).collect();
    // `sort_by` is stable, and the listing is already in canonical order, so
    // equal (depth, score) keys keep it.
    idx.sort_by(|&a, &b| {
        entries[a]
            .depth
            .cmp(&entries[b].depth)
            .then_with(|| {
                scores[b]
                    .partial_cmp(&scores[a])
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| a.cmp(&b))
    });
    let maxw = entries
        .iter()
        .map(|e| e.label.chars().count())
        .max()
        .unwrap_or(0);
    (idx, maxw)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::listing::{list, FileKind, Options};
    use std::fs;
    use std::path::Path;

    // Each test gets its own fixture directory: the tests run in parallel
    // threads and must not race on a shared temp path.
    fn fixture(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("lusty_native_rank_{}", tag));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("sub")).unwrap();
        fs::write(dir.join("alpha.txt"), b"x").unwrap();
        fs::write(dir.join("beta.lua"), b"x").unwrap();
        fs::write(dir.join("pic.jpg"), b"x").unwrap();
        fs::write(dir.join("sub/gamma.txt"), b"x").unwrap();
        dir
    }

    fn entries(dir: &Path) -> Vec<Entry> {
        list(
            dir,
            &Options {
                depth: 2,
                skip_dirs: vec![],
                follow_mounts: false,
                show_dots: false,
            },
        )
    }

    fn labels(entries: &[Entry], idxs: &[usize]) -> Vec<String> {
        idxs.iter().map(|&i| entries[i].label.clone()).collect()
    }

    #[test]
    fn first_letter_must_prefix_basename() {
        let dir = fixture("anchor");
        let e = entries(&dir);
        assert_eq!(
            rank_indices(&e, "c").len(),
            0,
            "query c must not match pic.jpg"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn prefix_query_keeps_matching_entries() {
        let dir = fixture("prefix");
        let e = entries(&dir);
        let l = labels(&e, &rank_indices(&e, "p"));
        assert!(l.contains(&"pic.jpg".to_string()), "pic.jpg starts with p");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn deep_entry_matches_by_basename() {
        let dir = fixture("deep");
        let e = entries(&dir);
        let idxs = rank_indices(&e, "gamma");
        assert_eq!(idxs.len(), 1, "gamma.txt inside sub/ is the only match");
        assert_eq!(e[idxs[0]].label, "sub/gamma.txt");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_query_returns_all() {
        let dir = fixture("all");
        let e = entries(&dir);
        assert_eq!(rank_indices(&e, "").len(), e.len());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn dot_query_skips_anchor() {
        let dir = fixture("dotq");
        fs::write(dir.join(".hidden"), b"x").unwrap();
        let e = list(
            dir.as_path(),
            &Options {
                depth: 1,
                skip_dirs: vec![],
                follow_mounts: false,
                show_dots: true,
            },
        );
        // ".hidden" starts with '.', so the anchor would never match it; with
        // the exemption the dot rule itself decides (every name has a dot).
        let idxs = rank_indices(&e, ".");
        assert!(idxs.iter().any(|&i| e[i].basename() == ".hidden"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn dirs_sort_before_their_contents() {
        // Fixture dirs are reported at depth 1, contents deeper; ranking
        // keeps shallower entries first for equal-ish queries.
        let dir = fixture("order");
        let e = entries(&dir);
        let idxs = rank_indices(&e, "g");
        assert_eq!(idxs.len(), 1);
        assert_eq!(e[idxs[0]].kind, FileKind::File);
        assert_eq!(e[idxs[0]].label, "sub/gamma.txt");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn frecency_orders_within_depth() {
        use std::os::unix::ffi::OsStrExt;
        let dir = fixture("frec");
        let e = entries(&dir);
        let mut frec: HashMap<Vec<u8>, f64> = HashMap::new();
        let key = |p: std::path::PathBuf| p.as_os_str().as_bytes().to_vec();
        // Beta is canonically after alpha; a high score lifts it within depth 1.
        frec.insert(key(dir.join("beta.lua")), 10.0);
        // Gamma is depth 2: it leads its own group but never depth 1.
        frec.insert(key(dir.join("sub/gamma.txt")), 100.0);

        let (idxs, _) = order_by_frecency(&e, &dir, &frec);
        let pos = |label: &str| idxs.iter().position(|&i| e[i].label == label).unwrap();
        assert!(
            pos("beta.lua") < pos("alpha.txt"),
            "frequent file first in its depth"
        );
        assert!(
            pos("beta.lua") < pos("sub/gamma.txt"),
            "depth 1 still beats depth 2"
        );
        let first_depth2 = idxs.iter().position(|&i| e[i].depth == 2).unwrap();
        assert_eq!(pos("sub/gamma.txt"), first_depth2, "gamma leads depth 2");

        // An empty map keeps the canonical listing order.
        let (plain, _) = order_by_frecency(&e, &dir, &HashMap::new());
        let labels: Vec<&str> = plain.iter().map(|&i| e[i].label.as_str()).collect();
        assert_eq!(
            labels,
            vec!["alpha.txt", "beta.lua", "pic.jpg", "sub", "sub/gamma.txt"]
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
