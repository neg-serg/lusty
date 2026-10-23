//! Truecolor icon colours, ported from `fzf-navigator`'s `theme.yml`
//! (see CREDITS.md): the same eza theme file that gives its `eza --icons`
//! listing per-extension foregrounds. lusty reads that file instead of copying
//! it, so one theme drives both pickers.
//!
//! Invariants, all asserted in the tests below:
//!
//! - **a theme never breaks the picker**: unknown sections, `null` colours and
//!   malformed lines are skipped; a hand-edited `theme.yml` costs colours, not
//!   the listing;
//! - **exact name beats extension, longest suffix beats shortest**: `Makefile`
//!   uses its `filenames:` colour, `a.tar.gz` uses `tar.gz` before `gz`;
//! - **a built-in starter table always exists**, so coloured icons work with no
//!   config at all; a loaded theme only overrides the keys it names;
//! - **no colour is invented**: anything the theme does not name returns
//!   `None`, and the caller keeps the LS_COLORS / theme foreground.

use std::path::{Path, PathBuf};

/// Colours taken verbatim from `fzf-navigator`'s `theme.yml` — the extensions
/// a file browser meets first. The full file (≈760 entries) is picked up from
/// the eza config directory when it is installed, see [`IconTheme::load`].
const STARTER_EXTS: &[(&str, u32)] = &[
    ("rs", 0xDEA584),
    ("toml", 0x9C4221),
    ("json", 0xCBCB41),
    ("jsonc", 0xCBCB41),
    ("yaml", 0xD70000),
    ("yml", 0xD70000),
    ("txt", 0x89E051),
    ("nix", 0x7EBAE4),
    ("lua", 0x51A0CF),
    ("py", 0xFFBC03),
    ("js", 0xCBCB41),
    ("ts", 0x519ABA),
    ("jsx", 0x20C2E3),
    ("tsx", 0x1354BF),
    ("css", 0x663399),
    ("scss", 0xF55385),
    ("html", 0xE44D26),
    ("htm", 0xE34C26),
    ("sh", 0x4D5A5E),
    ("bash", 0x89E051),
    ("zsh", 0x89E051),
    ("fish", 0x4D5A5E),
    ("c", 0x599EFF),
    ("h", 0xA074C4),
    ("cpp", 0x519ABA),
    ("hpp", 0xA074C4),
    ("cc", 0xF34B7D),
    ("go", 0x00ADD8),
    ("java", 0xCC3E44),
    ("kt", 0x7F52FF),
    ("rb", 0x701516),
    ("php", 0xA074C4),
    ("xml", 0xE37933),
    ("pdf", 0xB30B00),
    ("epub", 0xEAB16D),
    ("png", 0xA074C4),
    ("jpg", 0xA074C4),
    ("jpeg", 0xA074C4),
    ("gif", 0xA074C4),
    ("svg", 0xFFB13B),
    ("webp", 0xA074C4),
    ("ico", 0xCBCB41),
    ("mp3", 0x00AFFF),
    ("flac", 0x0075AA),
    ("ogg", 0x0075AA),
    ("opus", 0x0075AA),
    ("wav", 0x00AFFF),
    ("mp4", 0xFD971F),
    ("mkv", 0xFD971F),
    ("webm", 0xFD971F),
    ("mov", 0xFD971F),
    ("zip", 0xECA517),
    ("gz", 0xECA517),
    ("xz", 0xECA517),
    ("zst", 0xECA517),
    ("7z", 0xECA517),
    ("rar", 0xECA517),
    ("bin", 0x9F0500),
    ("exe", 0x9F0500),
    ("o", 0x9F0500),
    ("csv", 0x89E051),
];

/// Exact-name colours from the same file (`filenames:` section), the handful
/// that are worth shipping: the licence/readme/build files a repo is read by.
const STARTER_FILES: &[(&str, u32)] = &[
    ("Makefile", 0x6D8086),
    ("Dockerfile", 0x458EE6),
    ("LICENSE", 0xD0BF4F),
    ("Cargo.toml", 0xDEA584),
    ("Cargo.lock", 0xDEA584),
    ("flake.nix", 0x7EBAE4),
    (".gitignore", 0xF54D27),
    (".gitattributes", 0xF54D27),
    (".gitconfig", 0xF54D27),
    (".bashrc", 0x89E051),
    (".zshrc", 0x89E051),
    (".env", 0xFAF743),
];

fn rgb(color: u32) -> [u8; 3] {
    [
        ((color >> 16) & 0xff) as u8,
        ((color >> 8) & 0xff) as u8,
        (color & 0xff) as u8,
    ]
}

/// `#RRGGBB` -> RGB. Anything else (including `null`) is "no colour".
fn parse_hex(text: &str) -> Option<[u8; 3]> {
    let hex = text.trim().trim_start_matches('#');
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

/// Per-name icon colours: `filenames:` first, then the longest matching
/// extension suffix.
#[derive(Debug, Clone, Default)]
pub struct IconTheme {
    files: Vec<(String, [u8; 3])>,
    /// Sorted by suffix length, longest first, so the first hit is the most
    /// specific one.
    exts: Vec<(String, [u8; 3])>,
}

impl IconTheme {
    /// The shipped table; always non-empty so icons have colours out of the box.
    pub fn starter() -> IconTheme {
        let mut theme = IconTheme::default();
        for (name, color) in STARTER_FILES {
            theme.files.push(((*name).to_string(), rgb(*color)));
        }
        for (ext, color) in STARTER_EXTS {
            theme.exts.push(((*ext).to_string(), rgb(*color)));
        }
        theme.sort_exts();
        theme
    }

    fn sort_exts(&mut self) {
        self.exts
            .sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.0.cmp(&b.0)));
    }

    /// Parse an eza `theme.yml`. Only `extensions:` and `filenames:` are read,
    /// and only their `foreground:` colour; everything else is eza's business.
    /// Unparsable lines are skipped rather than fatal.
    pub fn parse(text: &str) -> IconTheme {
        let mut theme = IconTheme::default();
        let mut section = "";
        for raw in text.lines() {
            let line = raw.trim_end();
            if line.trim().is_empty() || line.trim_start().starts_with('#') {
                continue;
            }
            if !line.starts_with(' ') && !line.starts_with('\t') {
                section = line.trim().trim_end_matches(':');
                continue;
            }
            if section != "extensions" && section != "filenames" {
                continue;
            }
            let Some((key, rest)) = split_entry(line.trim()) else {
                continue;
            };
            let Some(color) = foreground(rest) else {
                continue;
            };
            if section == "extensions" {
                theme.exts.push((key, color));
            } else {
                theme.files.push((key, color));
            }
        }
        theme.sort_exts();
        theme
    }

    /// Overlay another theme: every name it defines wins, the rest of the
    /// receiver stays. Keeps the "a theme only adds" contract.
    pub fn overlay(mut self, other: IconTheme) -> IconTheme {
        for (name, color) in other.files {
            match self.files.iter_mut().find(|(n, _)| *n == name) {
                Some(slot) => slot.1 = color,
                None => self.files.push((name, color)),
            }
        }
        for (ext, color) in other.exts {
            match self.exts.iter_mut().find(|(e, _)| *e == ext) {
                Some(slot) => slot.1 = color,
                None => self.exts.push((ext, color)),
            }
        }
        self.sort_exts();
        self
    }

    /// Colour for one entry name: exact `filenames:` match first (case
    /// sensitive, like eza: it ships both `xorg.conf` and `Xorg.conf`), then
    /// the longest `extensions:` suffix (case insensitive).
    pub fn color_for(&self, name: &str) -> Option<[u8; 3]> {
        if let Some((_, color)) = self.files.iter().find(|(n, _)| n == name) {
            return Some(*color);
        }
        let lower = name.to_ascii_lowercase();
        for (ext, color) in &self.exts {
            if ext.is_empty() {
                continue;
            }
            let mut suffix = String::with_capacity(ext.len() + 1);
            suffix.push('.');
            suffix.push_str(&ext.to_ascii_lowercase());
            if lower.ends_with(&suffix) {
                return Some(*color);
            }
        }
        None
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty() && self.exts.is_empty()
    }

    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    pub fn ext_count(&self) -> usize {
        self.exts.len()
    }

    /// Paths a theme is looked up at, in priority order.
    fn candidates() -> Vec<PathBuf> {
        let mut out = Vec::new();
        if let Ok(explicit) = std::env::var("LUSTY_ICON_THEME") {
            let explicit = explicit.trim().to_string();
            if !explicit.is_empty() {
                out.push(PathBuf::from(explicit));
                return out;
            }
        }
        if let Ok(dir) = std::env::var("EZA_CONFIG_DIR") {
            if !dir.trim().is_empty() {
                out.push(Path::new(dir.trim()).join("theme.yml"));
            }
        }
        let config = std::env::var("XDG_CONFIG_HOME")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var("HOME")
                    .ok()
                    .map(|h| Path::new(&h).join(".config"))
            });
        if let Some(config) = config {
            out.push(config.join("eza/theme.yml"));
            out.push(config.join("lusty/icon-theme.yml"));
        }
        out
    }

    /// Starter table plus the first readable theme file, if any.
    /// `LUSTY_ICON_THEME=0|off` disables the file lookup (the starter table
    /// still applies); `LUSTY_ICON_THEME=/path` names one file outright.
    pub fn load() -> IconTheme {
        let starter = IconTheme::starter();
        if matches!(
            std::env::var("LUSTY_ICON_THEME")
                .unwrap_or_default()
                .trim()
                .to_ascii_lowercase()
                .as_str(),
            "0" | "off" | "no" | "false"
        ) {
            return starter;
        }
        for path in Self::candidates() {
            if let Ok(text) = std::fs::read_to_string(&path) {
                return starter.overlay(IconTheme::parse(&text));
            }
        }
        starter
    }
}

/// Split `"key": { ... }` into the key and the rest of the line. Quoted keys
/// are unquoted; a bare key is taken up to the colon.
fn split_entry(line: &str) -> Option<(String, &str)> {
    if let Some(rest) = line.strip_prefix('"') {
        let end = rest.find('"')?;
        let key = &rest[..end];
        let after = rest[end + 1..].trim_start();
        let after = after.strip_prefix(':')?;
        return Some((key.to_string(), after));
    }
    let (key, rest) = line.split_once(':')?;
    let key = key.trim();
    if key.is_empty() {
        return None;
    }
    Some((key.to_string(), rest))
}

/// Pull `foreground: "#RRGGBB"` (or `foreground: null`) out of an entry.
fn foreground(rest: &str) -> Option<[u8; 3]> {
    let idx = rest.find("foreground:")?;
    let tail = &rest[idx + "foreground:".len()..];
    let tail = tail.trim_start().strip_prefix('"')?;
    let end = tail.find('"')?;
    parse_hex(&tail[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    // `##` because the fixture itself contains `"#` colour literals.
    const SAMPLE: &str = r##"
filekinds:
  normal: { foreground: "#D8DEE9" }
extensions:
  "tar.gz": { icon: { style: { foreground: "#A0A0A0" } } }
  "gz": { icon: { style: { foreground: "#ECA517" } } }
  "rs": { icon: { style: { foreground: "#DEA584" } } }
  "nulled": { icon: { style: { foreground: null } } }
  broken line without a colon
  "nested": { icon: { style: { foreground: "#010203", bold: true } } }
filenames:
  "Makefile": { icon: { style: { foreground: "#6D8086" } } }
  "Xorg.conf": { icon: { style: { foreground: "#E54D18" } } }
  "xorg.conf": { icon: { style: { foreground: "#000001" } } }
"##;

    #[test]
    fn parses_only_the_two_sections_it_owns() {
        let t = IconTheme::parse(SAMPLE);
        assert_eq!(t.ext_count(), 4, "`nulled` has no colour and is skipped");
        assert_eq!(t.file_count(), 3);
        assert_eq!(
            t.color_for("filekinds"),
            None,
            "other sections are not names"
        );
    }

    #[test]
    fn the_longest_extension_suffix_wins() {
        let t = IconTheme::parse(SAMPLE);
        assert_eq!(t.color_for("archive.tar.gz"), Some([0xA0, 0xA0, 0xA0]));
        assert_eq!(t.color_for("x.gz"), Some([0xEC, 0xA5, 0x17]));
        assert_eq!(t.color_for("main.rs"), Some([0xDE, 0xA5, 0x84]));
        assert_eq!(t.color_for("rs"), None, "a bare name is not an extension");
        assert_eq!(t.color_for("a.rs.bak"), None);
    }

    #[test]
    fn extensions_match_case_insensitively_and_filenames_do_not() {
        let t = IconTheme::parse(SAMPLE);
        assert_eq!(t.color_for("MAIN.RS"), Some([0xDE, 0xA5, 0x84]));
        assert_eq!(t.color_for("Makefile"), Some([0x6D, 0x80, 0x86]));
        assert_eq!(t.color_for("makefile"), None);
        assert_eq!(t.color_for("Makefile.old"), None);
        assert_eq!(t.color_for("Xorg.conf"), Some([0xE5, 0x4D, 0x18]));
        assert_eq!(t.color_for("xorg.conf"), Some([0x00, 0x00, 0x01]));
    }

    #[test]
    fn a_broken_theme_still_yields_the_names_it_did_parse() {
        let t = IconTheme::parse("extensions:\n  \"rs\": { icon: { style: {\n  ???\n\t\n");
        assert_eq!(t.color_for("main.rs"), None);
        assert_eq!(t.ext_count(), 0);
    }

    #[test]
    fn the_starter_table_is_always_there_and_a_theme_only_overrides() {
        let starter = IconTheme::starter();
        assert!(starter.color_for("main.rs").is_some());
        assert!(starter.color_for("Makefile").is_some());
        let merged = starter.clone().overlay(IconTheme::parse(SAMPLE));
        assert_eq!(merged.color_for("main.rs"), Some([0xDE, 0xA5, 0x84]));
        assert_eq!(merged.color_for("archive.tar.gz"), Some([0xA0, 0xA0, 0xA0]));
        assert!(
            merged.color_for("main.c").is_some(),
            "names the theme does not mention keep their starter colour"
        );
        assert!(merged.ext_count() >= starter.ext_count());
    }

    #[test]
    fn the_starter_table_has_no_duplicate_names() {
        let starter = IconTheme::starter();
        let mut exts: Vec<&str> = starter.exts.iter().map(|(e, _)| e.as_str()).collect();
        exts.sort_unstable();
        let before = exts.len();
        exts.dedup();
        assert_eq!(
            before,
            exts.len(),
            "duplicate extension in the starter table"
        );
        let mut files: Vec<&str> = starter.files.iter().map(|(f, _)| f.as_str()).collect();
        files.sort_unstable();
        let n = files.len();
        files.dedup();
        assert_eq!(n, files.len(), "duplicate filename in the starter table");
    }
}
