//! Golden-vector dump: the shared oracle for out-of-process ports of this crate.
//!
//! The quickshell tray implements the matcher in JavaScript (a per-keystroke
//! subprocess round-trip is not worth it for a menu). To keep that port honest
//! it is not compared "by eye": the corpus below is ranked here, the result is
//! written to a fixture, and the JS side replays the same fixture — any drift in
//! scores, order or spans fails the check.
//!
//! Everything is deterministic on purpose: fixed score precision, labels and
//! queries carried in the JSON, no timestamps.

use crate::rank::{rank, Anchor, Layout, RankOpts};

/// Corpus subset for the dump: labels and queries in file order.
pub fn parse_corpus(text: &str) -> (Vec<String>, Vec<String>) {
    let (mut labels, mut queries) = (Vec::new(), Vec::new());
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(v) = line.strip_prefix("label:") {
            labels.push(v.trim().to_string());
        } else if let Some(v) = line.strip_prefix("query:") {
            queries.push(v.trim().to_string());
        }
    }
    (labels, queries)
}

fn anchor_name(anchor: Anchor) -> &'static str {
    match anchor {
        Anchor::None => "none",
        Anchor::PrefixOnFirst => "prefix-on-first",
    }
}

fn layout_name(layout: Layout) -> &'static str {
    match layout {
        Layout::Off => "off",
        Layout::RuToEnFallback => "ru-to-en-fallback",
    }
}

/// Render the ranked corpus as JSON. Field order is fixed and scores are
/// printed with nine decimals so two runs are byte-identical.
pub fn vectors_json(labels: &[String], queries: &[String], opts: RankOpts) -> String {
    let refs: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
    let mut out = String::new();
    out.push_str("{\n");
    out.push_str(&format!(
        "  \"weights_version\": {},\n",
        crate::WEIGHTS_VERSION
    ));
    out.push_str(&format!(
        "  \"anchor\": \"{}\",\n  \"layout\": \"{}\",\n  \"fold_case\": {},\n",
        anchor_name(opts.anchor),
        layout_name(opts.layout),
        opts.fold_case
    ));
    out.push_str("  \"labels\": [\n");
    for (i, label) in labels.iter().enumerate() {
        let sep = if i + 1 == labels.len() { "" } else { "," };
        out.push_str(&format!("    \"{}\"{}\n", json_escape(label), sep));
    }
    out.push_str("  ],\n  \"cases\": [\n");
    for (ci, query) in queries.iter().enumerate() {
        out.push_str(&format!(
            "    {{\n      \"query\": \"{}\",\n",
            json_escape(query)
        ));
        out.push_str("      \"results\": [\n");
        let hits = rank(&refs, query, opts);
        for (ri, m) in hits.iter().enumerate() {
            let sep = if ri + 1 == hits.len() { "" } else { "," };
            let spans: Vec<String> = m.spans.iter().map(|(a, b)| format!("[{a}, {b}]")).collect();
            out.push_str(&format!(
                "        {{\"index\": {}, \"label\": \"{}\", \"score\": {:.9}, \"spans\": [{}]}}{}\n",
                m.index,
                json_escape(&labels[m.index]),
                m.score,
                spans.join(", "),
                sep
            ));
        }
        out.push_str("      ]\n    }");
        if ci + 1 != queries.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("  ]\n}\n");
    out
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const CORPUS: &str =
        "# c\nlabel: Open in browser\nquery: op\n\nlabel: Lock screen\nquery: lock\n# tail\n";

    #[test]
    fn corpus_parses_labels_and_queries() {
        let (labels, queries) = parse_corpus(CORPUS);
        assert_eq!(labels, vec!["Open in browser", "Lock screen"]);
        assert_eq!(queries, vec!["op", "lock"]);
    }

    #[test]
    fn dump_is_deterministic() {
        let (labels, queries) = parse_corpus(CORPUS);
        let a = vectors_json(&labels, &queries, RankOpts::for_menus());
        let b = vectors_json(&labels, &queries, RankOpts::for_menus());
        assert_eq!(a, b);
        assert!(a.contains("\"weights_version\": 1"));
        assert!(a.contains("\"anchor\": \"none\""));
        assert!(a.contains("\"fold_case\": true"));
        assert!(a.contains("\"score\": 0."));
    }

    #[test]
    fn dump_carries_the_labels_it_ranked() {
        let (labels, queries) = parse_corpus(CORPUS);
        let json = vectors_json(&labels, &queries, RankOpts::for_menus());
        for label in &labels {
            assert!(json.contains(label), "{label} missing");
        }
    }

    #[test]
    fn escaping_survives_quotes_and_backslashes() {
        assert_eq!(json_escape("a\"b\\c"), "a\\\"b\\\\c");
    }
}
