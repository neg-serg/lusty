//! Corpus invariants: the two scoring paths must agree, spans must describe the
//! matched characters, and the dump must be deterministic.

use lusty_fuzzy::{rank, score_with_spans, Anchor, RankOpts, Scorer, WEIGHTS_VERSION};

const CORPUS: &str = include_str!("corpus/menu.txt");

fn corpus() -> (Vec<String>, Vec<String>) {
    lusty_fuzzy::vectors::parse_corpus(CORPUS)
}

#[test]
fn corpus_is_not_empty() {
    let (labels, queries) = corpus();
    assert!(labels.len() >= 20, "labels: {}", labels.len());
    assert!(queries.len() >= 10, "queries: {}", queries.len());
    assert_eq!(WEIGHTS_VERSION, 1);
}

/// The hot path (`Scorer::score`) and the span path must produce the same score
/// for every label/query pair — otherwise a UI would highlight one alignment
/// while ranking by another.
#[test]
fn span_path_scores_match_the_hot_path() {
    let (labels, queries) = corpus();
    let mut hot = Scorer::new();
    for label in &labels {
        for query in &queries {
            let expected = hot.score(label, query);
            let (got, spans) = score_with_spans(label, query);
            assert_eq!(
                expected, got,
                "score mismatch for {label:?} / {query:?} (spans {spans:?})"
            );
        }
    }
}

#[test]
fn spans_cover_exactly_the_query_bytes() {
    let (labels, queries) = corpus();
    for label in &labels {
        for query in &queries {
            let (score, spans) = score_with_spans(label, query);
            if score == 0.0 {
                assert!(spans.is_empty(), "{label:?} / {query:?}: {spans:?}");
                continue;
            }
            let covered: usize = spans.iter().map(|(a, b)| b - a).sum();
            assert_eq!(covered, query.len(), "{label:?} / {query:?}: {spans:?}");
            assert!(spans.windows(2).all(|w| w[0].1 <= w[1].0), "{spans:?}");
            assert!(spans.last().unwrap().1 <= label.len());
        }
    }
}

#[test]
fn ranking_is_deterministic_and_ordered() {
    let (labels, queries) = corpus();
    let refs: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
    for query in &queries {
        let a = rank(&refs, query, RankOpts::for_menus());
        let b = rank(&refs, query, RankOpts::for_menus());
        assert_eq!(a, b, "not deterministic for {query:?}");
        assert!(
            a.windows(2).all(|w| w[0].score >= w[1].score),
            "unordered for {query:?}"
        );
        // Scores may be negative — a match with many skipped characters scores
        // below zero (inherited from the Lua port: "pic.jpg" / "c" is -0.024).
        // Only an exact 0.0 means "no match" and is filtered.
        assert!(a.iter().all(|m| m.score != 0.0));
        if query == "zzzz" {
            assert!(a.is_empty(), "{a:?}");
        }
    }
}

#[test]
fn anchor_policy_changes_the_candidate_set() {
    let (labels, _) = corpus();
    let refs: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
    let loose = rank(
        &refs,
        "b",
        RankOpts {
            anchor: Anchor::None,
            ..RankOpts::for_menus()
        },
    );
    let strict = rank(
        &refs,
        "b",
        RankOpts {
            anchor: Anchor::PrefixOnFirst,
            ..RankOpts::for_menus()
        },
    );
    assert!(
        loose.len() > strict.len(),
        "{} vs {}",
        loose.len(),
        strict.len()
    );
    assert!(loose.iter().any(|m| labels[m.index] == "Open in browser"));
}

#[test]
fn dump_is_byte_identical_between_runs() {
    let (labels, queries) = corpus();
    let a = lusty_fuzzy::vectors::vectors_json(&labels, &queries, RankOpts::for_menus());
    let b = lusty_fuzzy::vectors::vectors_json(&labels, &queries, RankOpts::for_menus());
    assert_eq!(a, b);
    assert!(a.contains("\"query\": \"щз\""), "RU-layout case missing");
}
