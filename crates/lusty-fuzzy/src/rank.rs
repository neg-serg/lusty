//! Query ranking over plain labels: ordering, anchor policy and match spans.
//!
//! This is the consumer-facing half of the crate: the pickers rank files with
//! their own depth/frecency rules (`lusty::rank`), so this module only owns the
//! parts that are independent of what is being ranked — scoring, the anchor
//! policy, the RU→EN fallback and the ordering of equally scored candidates.

use crate::layout::normalize_query_char;
use crate::scorer;

/// Whether the query's first character must equal the candidate's first
/// character. The file pickers rely on it (`c` must not match `pic.jpg`); menu
/// lists turn it off, because there the user types a word from the middle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchor {
    None,
    PrefixOnFirst,
}

/// How to treat a query typed in the wrong keyboard layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    Off,
    /// Retry with the RU→EN mapping when the literal query finds nothing.
    RuToEnFallback,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RankOpts {
    pub anchor: Anchor,
    pub layout: Layout,
    /// Case-fold with full Unicode (labels from DBus apps are often localized:
    /// a lowercase `настройки` must find `Настройки`). The file pickers keep the
    /// historical ASCII-only folding, which is what makes the byte spans cheap
    /// and what their golden dumps already pin.
    pub fold_case: bool,
}

impl RankOpts {
    /// Tray/context menus: no anchor, RU fallback on, Unicode case folding on.
    pub const fn for_menus() -> Self {
        Self {
            anchor: Anchor::None,
            layout: Layout::RuToEnFallback,
            fold_case: true,
        }
    }

    /// File/buffer pickers: the historical Lusty behaviour.
    pub const fn for_files() -> Self {
        Self {
            anchor: Anchor::PrefixOnFirst,
            layout: Layout::RuToEnFallback,
            fold_case: false,
        }
    }
}

impl Default for RankOpts {
    fn default() -> Self {
        Self::for_files()
    }
}

/// One ranked candidate. `spans` are byte ranges into the label (see
/// [`scorer::score_with_spans`]) and are empty for an empty query.
#[derive(Clone, Debug, PartialEq)]
pub struct Match {
    pub index: usize,
    pub score: f64,
    pub spans: Vec<(usize, usize)>,
}

/// Map a query typed in the RU layout onto the EN characters of the table.
pub fn normalize_query(query: &str) -> String {
    query.chars().filter_map(normalize_query_char).collect()
}

/// Rank `labels` against `query`, best first, ties in input order.
///
/// An empty query returns every label in input order with the neutral score and
/// no spans (callers use that as "no filtering"). Non-matching labels are
/// dropped.
pub fn rank(labels: &[&str], query: &str, opts: RankOpts) -> Vec<Match> {
    if query.is_empty() {
        return labels
            .iter()
            .enumerate()
            .map(|(index, label)| Match {
                index,
                score: scorer::score(label, ""),
                spans: Vec::new(),
            })
            .collect();
    }
    let mut out = rank_effective(labels, query, opts);
    if opts.layout == Layout::RuToEnFallback && !query.is_ascii() {
        let mapped = normalize_query(query);
        if mapped != query && mapped.is_ascii() && !mapped.is_empty() {
            // Merge both readings instead of "retry only on empty": a Cyrillic
            // query can hit a Cyrillic label literally (the scorer compares
            // bytes, ASCII-case-insensitively) while the same keystrokes in the
            // EN layout hit Latin labels. Keeping the better score per label
            // means neither reading hides the other.
            let alt = rank_effective(labels, &mapped, opts);
            let mut merged: std::collections::BTreeMap<usize, Match> =
                out.into_iter().map(|m| (m.index, m)).collect();
            for m in alt {
                match merged.get(&m.index) {
                    Some(prev) if prev.score >= m.score => {}
                    _ => {
                        merged.insert(m.index, m);
                    }
                }
            }
            out = merged.into_values().collect();
            out.sort_by(|a, b| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
    }
    out
}

/// Case-fold `s` and remember where every folded byte came from, so spans can
/// be reported in the *original* label's byte offsets (a UI highlights the label
/// it drew, not a folded copy of it).
fn fold_with_map(s: &str) -> (String, Vec<usize>) {
    let mut folded = String::with_capacity(s.len());
    let mut map: Vec<usize> = Vec::with_capacity(s.len() + 1);
    for (offset, ch) in s.char_indices() {
        for lower in ch.to_lowercase() {
            let before = folded.len();
            folded.push(lower);
            for _ in before..folded.len() {
                map.push(offset);
            }
        }
    }
    map.push(s.len());
    (folded, map)
}

/// Map folded byte ranges back onto the original string.
fn unmap_spans(spans: Vec<(usize, usize)>, map: &[usize]) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> = Vec::with_capacity(spans.len());
    for (a, b) in spans {
        let start = map.get(a).copied().unwrap_or(0);
        let end = map.get(b).copied().unwrap_or(start);
        match out.last_mut() {
            Some((_, prev_end)) if *prev_end == start => *prev_end = end,
            _ => out.push((start, end)),
        }
    }
    out
}

fn rank_effective(labels: &[&str], query: &str, opts: RankOpts) -> Vec<Match> {
    let (needle, _) = if opts.fold_case {
        fold_with_map(query)
    } else {
        (query.to_string(), Vec::new())
    };
    let first = match opts.anchor {
        Anchor::None => None,
        Anchor::PrefixOnFirst => needle.as_bytes().first().map(|b| b.to_ascii_lowercase()),
    };
    let mut out: Vec<Match> = Vec::new();
    for (index, label) in labels.iter().enumerate() {
        let (hay, map) = if opts.fold_case {
            fold_with_map(label)
        } else {
            (label.to_string(), Vec::new())
        };
        if let Some(f) = first {
            let head = hay.as_bytes().first().map(|b| b.to_ascii_lowercase());
            if head != Some(f) {
                continue;
            }
        }
        let (score, spans) = scorer::score_with_spans(&hay, &needle);
        if score != 0.0 {
            let spans = if opts.fold_case {
                unmap_spans(spans, &map)
            } else {
                spans
            };
            out.push(Match {
                index,
                score,
                spans,
            });
        }
    }
    // Stable: equal scores keep the input order (`sort_by` is stable).
    out.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const MENU: &[&str] = &[
        "Open in browser",
        "Open in terminal",
        "Lock screen",
        "Take screenshot",
        "Preferences",
        "Power off",
    ];

    #[test]
    fn anchor_off_finds_mid_word_hits() {
        let hits = rank(MENU, "b", RankOpts::for_menus());
        let labels: Vec<&str> = hits.iter().map(|m| MENU[m.index]).collect();
        assert!(labels.contains(&"Open in browser"), "{labels:?}");
    }

    #[test]
    fn anchor_on_drops_them() {
        let hits = rank(MENU, "b", RankOpts::for_files());
        assert!(hits.is_empty(), "{:?}", hits.len());
    }

    #[test]
    fn start_of_label_outranks_middle() {
        let hits = rank(MENU, "o", RankOpts::for_menus());
        assert_eq!(MENU[hits[0].index], "Open in browser");
        assert!(hits.len() >= 3, "{hits:?}");
    }

    #[test]
    fn empty_query_keeps_input_order() {
        let hits = rank(MENU, "", RankOpts::for_menus());
        assert_eq!(hits.len(), MENU.len());
        assert!(hits.iter().enumerate().all(|(i, m)| m.index == i));
        assert!(hits.iter().all(|m| m.spans.is_empty() && m.score == 0.75));
    }

    #[test]
    fn ru_layout_query_also_matches_en_labels() {
        // "щз" is what "op" looks like when typed in the RU layout.
        let hits = rank(MENU, "щз", RankOpts::for_menus());
        let top = MENU[hits[0].index];
        assert_eq!(top, "Open in browser", "{hits:?}");
    }

    #[test]
    fn lowercase_cyrillic_query_finds_an_uppercase_label() {
        // Tray labels arrive localized; folding must be Unicode-wide for menus.
        let labels = &["Настройки", "Показать скрытые файлы"];
        let hits = rank(labels, "настр", RankOpts::for_menus());
        assert_eq!(hits.len(), 1, "{hits:?}");
        assert_eq!(labels[hits[0].index], "Настройки");
        // Spans are byte offsets into the ORIGINAL label: "Настройки" starts with
        // the two-byte 'Н', so the match covers bytes 0..10 (настр).
        assert_eq!(hits[0].spans, vec![(0, 10)], "{:?}", hits[0].spans);
    }

    #[test]
    fn file_pickers_keep_the_byte_semantics() {
        // fold_case = false is the pre-existing behaviour: ASCII-only folding,
        // so a lowercase Cyrillic query does not match an uppercase label.
        let labels = &["Настройки", "Settings"];
        let hits = rank(labels, "настр", RankOpts::for_files());
        assert!(hits.is_empty(), "{hits:?}");
    }

    #[test]
    fn literal_cyrillic_query_finds_the_cyrillic_label() {
        let labels = &["Показать скрытые файлы", "Show hidden files"];
        let hits = rank(labels, "скр", RankOpts::for_menus());
        assert!(
            hits.iter()
                .any(|m| labels[m.index] == "Показать скрытые файлы"),
            "{hits:?}"
        );
    }

    #[test]
    fn spans_are_reported_for_the_top_hit() {
        // Adjacent matches coalesce into one run: "pref" starts "Preferences".
        let hits = rank(MENU, "pref", RankOpts::for_menus());
        assert_eq!(hits[0].spans, vec![(0, 4)]);
        // Disjoint matches stay separate runs.
        let hits = rank(MENU, "t s", RankOpts::for_menus());
        assert_eq!(hits[0].index, 3, "expected Take screenshot");
        assert!(hits[0].spans.len() >= 2, "{:?}", hits[0].spans);
    }
}
