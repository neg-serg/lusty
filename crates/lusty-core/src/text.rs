//! Display hygiene for file names.
//!
//! Three surfaces print names — the standalone TUI, the `--list` plumbing
//! output and the `serve` protocol the nvim client consumes — and none of them
//! may pass a control byte through: a file called `esc_ESC[2J_name` would
//! otherwise clear the screen of whoever prints it.

use std::borrow::Cow;

/// Terminal-safe copy of a label or path: TAB becomes a space and every other
/// control character a '?'. Returns the input borrowed when there is nothing
/// to replace.
pub fn sanitize_display(s: &str) -> Cow<'_, str> {
    if !s.chars().any(char::is_control) {
        return Cow::Borrowed(s);
    }
    Cow::Owned(
        s.chars()
            .map(|c| {
                if c == '\t' {
                    ' '
                } else if c.is_control() {
                    '?'
                } else {
                    c
                }
            })
            .collect(),
    )
}

/// Sanitized label for the line protocol. Control characters are replaced the
/// same way, except TAB and LF: those are wire framing, escaped as `\t`/`\n`
/// by the writer (and the client expects them escaped), so they stay as bytes.
pub fn sanitize_wire_label(s: &str) -> Cow<'_, str> {
    if !s.chars().any(|c| c.is_control() && c != '\t' && c != '\n') {
        return Cow::Borrowed(s);
    }
    Cow::Owned(
        s.chars()
            .map(|c| {
                if c.is_control() && c != '\t' && c != '\n' {
                    '?'
                } else {
                    c
                }
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_sanitizer_strips_controls() {
        assert_eq!(sanitize_display("plain name.txt"), "plain name.txt");
        assert_eq!(sanitize_display("a\u{1b}[2Jb\tc"), "a?[2Jb c");
        assert_eq!(sanitize_display("суб/файл.txt"), "суб/файл.txt");
        assert!(matches!(sanitize_display("plain"), Cow::Borrowed(_)));
    }

    #[test]
    fn wire_label_keeps_the_framing_characters() {
        // TAB and LF are escaped by the protocol writer, not replaced here.
        assert_eq!(sanitize_wire_label("tab\tname"), "tab\tname");
        assert_eq!(sanitize_wire_label("nl\nname"), "nl\nname");
        // Anything else that is a control byte must not reach the client.
        assert_eq!(sanitize_wire_label("a\u{1b}[2Jb"), "a?[2Jb");
        assert_eq!(sanitize_wire_label("bell\u{7}"), "bell?");
        assert!(matches!(sanitize_wire_label("tab\tname"), Cow::Borrowed(_)));
    }
}
