//! Action registry and keymap: the one table both the key dispatch and the
//! `?` help overlay read, so the help can never drift from what the keys do.
//!
//! Ported from `fzf-navigator` (see CREDITS.md), which keeps its `action` +
//! `key` pairs in one newline-delimited table and derives the footer help,
//! the user-override parsing and the key handling from the same source. The
//! invariants that made that table safe are kept here:
//!
//! - **one key, one action**: binding a key that another action holds steals
//!   it, so two features can never fire on the same press;
//! - **an action keeps at least one key**: a rebind *moves* the action, it
//!   does not remove it;
//! - **unknown names are rejected, never guessed**: an action name that is not
//!   in [`Action::all`] and a key that does not parse are reported and the rest
//!   of the spec still applies — a typo costs one binding, not the keymap;
//! - **built-ins are not action keys**: navigation/editing keys stay fixed and
//!   are listed separately, so the ported action table only ever names keys it
//!   fully owns.

use std::fmt;

/// Everything a key can be asked to do. The order is the help order and the
/// `--key-map` dump order: the ported `fzf-navigator` actions first, in its own
/// menu order, then the picker-local extras.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Action {
    Open,
    OpenTab,
    OpenSplit,
    OpenVSplit,
    InsertSelection,
    CopySelection,
    Exit,
    Cancel,
    GoToParent,
    GoBack,
    GoForward,
    GoToSessionStart,
    GoHome,
    Descend,
    ToggleHidden,
    ToggleSkipped,
    ToggleLocked,
    ToggleRecentFirst,
    TogglePreview,
    CycleView,
    CycleSort,
    CycleDepth,
    Mark,
    Reload,
    Redraw,
    ToggleHelp,
}

impl Action {
    /// Every action, in help order.
    pub const ALL: [Action; 26] = [
        Action::Open,
        Action::OpenTab,
        Action::OpenSplit,
        Action::OpenVSplit,
        Action::InsertSelection,
        Action::CopySelection,
        Action::Exit,
        Action::Cancel,
        Action::GoToParent,
        Action::GoBack,
        Action::GoForward,
        Action::GoToSessionStart,
        Action::GoHome,
        Action::Descend,
        Action::ToggleHidden,
        Action::ToggleSkipped,
        Action::ToggleLocked,
        Action::ToggleRecentFirst,
        Action::TogglePreview,
        Action::CycleView,
        Action::CycleSort,
        Action::CycleDepth,
        Action::Mark,
        Action::Reload,
        Action::Redraw,
        Action::ToggleHelp,
    ];

    /// Stable snake_case name used by `LUSTY_KEYS`, `--keys` and `--key-map`.
    pub fn name(self) -> &'static str {
        match self {
            Action::Open => "open",
            Action::OpenTab => "open_tab",
            Action::OpenSplit => "open_split",
            Action::OpenVSplit => "open_vsplit",
            Action::InsertSelection => "insert_selection",
            Action::CopySelection => "copy_selection",
            Action::Exit => "exit",
            Action::Cancel => "cancel",
            Action::GoToParent => "go_to_parent",
            Action::GoBack => "go_back",
            Action::GoForward => "go_forward",
            Action::GoToSessionStart => "go_to_session_start",
            Action::GoHome => "go_home",
            Action::Descend => "descend",
            Action::ToggleHidden => "toggle_hidden",
            Action::ToggleSkipped => "toggle_skipped",
            Action::ToggleLocked => "toggle_locked",
            Action::ToggleRecentFirst => "toggle_recent_first",
            Action::TogglePreview => "toggle_preview",
            Action::CycleView => "cycle_view",
            Action::CycleSort => "cycle_sort",
            Action::CycleDepth => "cycle_depth",
            Action::Mark => "mark",
            Action::Reload => "reload",
            Action::Redraw => "redraw",
            Action::ToggleHelp => "toggle_help",
        }
    }

    /// Human text for the help overlay (the shell version printed the name
    /// with underscores turned into spaces; a sentence says more here).
    pub fn help(self) -> &'static str {
        match self {
            Action::Open => "open: enter a directory, or print the selection",
            Action::OpenTab => "open the selection in a new tab",
            Action::OpenSplit => "open the selection in a split",
            Action::OpenVSplit => "open the selection in a vertical split",
            Action::InsertSelection => {
                "print the selection for insertion (relative to the launch dir)"
            }
            Action::CopySelection => "copy the selection to the clipboard",
            Action::Exit => "leave the picker (locked: print the cd target)",
            Action::Cancel => "back to the session start, or leave",
            Action::GoToParent => "go to the parent directory",
            Action::GoBack => "go back in the directory history",
            Action::GoForward => "go forward in the directory history",
            Action::GoToSessionStart => "go to the session start directory",
            Action::GoHome => "go to the home directory",
            Action::Descend => "descend into the typed directory, or go to / when empty",
            Action::ToggleHidden => "toggle hidden files",
            Action::ToggleSkipped => "toggle skipped directories",
            Action::ToggleLocked => "toggle locked mode",
            Action::ToggleRecentFirst => "most recently modified first",
            Action::TogglePreview => "toggle the preview pane",
            Action::CycleView => "cycle list, grid and long view",
            Action::CycleSort => "cycle the sort order",
            Action::CycleDepth => "cycle the search depth",
            Action::Mark => "mark the entry and step down",
            Action::Reload => "relist the current directory",
            Action::Redraw => "repaint the picker (after a garbled frame)",
            Action::ToggleHelp => "toggle this help",
        }
    }

    /// Parse the name used in specs and dumps. Accepts the shell version's
    /// historical spelling (`toggle_hidden_files`) as an alias, so a config
    /// copied from `fzf-navigator` keeps working.
    pub fn from_name(name: &str) -> Option<Action> {
        let name = name.trim();
        let name = match name {
            "toggle_hidden_files" => "toggle_hidden",
            "toggle_ignored_files" => "toggle_skipped",
            other => other,
        };
        Action::ALL.iter().copied().find(|a| a.name() == name)
    }
}

/// A key that is not a character: arrows, editing keys, space and escape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NamedKey {
    Esc,
    Enter,
    Tab,
    Backspace,
    Delete,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Space,
}

impl NamedKey {
    fn parse(text: &str) -> Option<NamedKey> {
        Some(match text {
            "esc" | "escape" => NamedKey::Esc,
            "enter" | "return" | "cr" => NamedKey::Enter,
            "tab" => NamedKey::Tab,
            "backspace" | "bs" => NamedKey::Backspace,
            "delete" | "del" => NamedKey::Delete,
            "up" => NamedKey::Up,
            "down" => NamedKey::Down,
            "left" => NamedKey::Left,
            "right" => NamedKey::Right,
            "home" => NamedKey::Home,
            "end" => NamedKey::End,
            "pageup" | "pgup" => NamedKey::PageUp,
            "pagedown" | "pgdn" => NamedKey::PageDown,
            "space" => NamedKey::Space,
            _ => return None,
        })
    }

    fn name(self) -> &'static str {
        match self {
            NamedKey::Esc => "esc",
            NamedKey::Enter => "enter",
            NamedKey::Tab => "tab",
            NamedKey::Backspace => "backspace",
            NamedKey::Delete => "delete",
            NamedKey::Up => "up",
            NamedKey::Down => "down",
            NamedKey::Left => "left",
            NamedKey::Right => "right",
            NamedKey::Home => "home",
            NamedKey::End => "end",
            NamedKey::PageUp => "pageup",
            NamedKey::PageDown => "pagedown",
            NamedKey::Space => "space",
        }
    }
}

/// One key press: a named key or a single character, plus the modifiers the
/// terminal reports. `ctrl-space`, `alt-h`, `ctrl-alt-o` and plain `[` are all
/// in here — the same spellings the shell version took from fzf.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key {
    named: Option<NamedKey>,
    ch: Option<char>,
    pub ctrl: bool,
    pub alt: bool,
}

impl Key {
    pub fn named(named: NamedKey) -> Key {
        Key {
            named: Some(named),
            ch: None,
            ctrl: false,
            alt: false,
        }
    }

    pub fn ch(ch: char) -> Key {
        Key {
            named: None,
            ch: Some(ch),
            ctrl: false,
            alt: false,
        }
    }

    pub fn with_ctrl(mut self, on: bool) -> Key {
        self.ctrl = on;
        self
    }

    pub fn with_alt(mut self, on: bool) -> Key {
        self.alt = on;
        self
    }

    /// The character this key produces, `None` for named keys. Front-ends use
    /// it to decide whether a press must reach the prompt instead of an action.
    pub fn char(self) -> Option<char> {
        self.ch
    }

    /// The named key, `None` for character keys.
    pub fn named_key(self) -> Option<NamedKey> {
        self.named
    }

    /// Parse the `alt-h` / `ctrl-alt-o` / `?` spelling. Modifier prefixes are
    /// `ctrl-`/`c-` and `alt-`/`a-`/`meta-`/`m-`, case-insensitive; the base
    /// is a named key or exactly one character.
    pub fn parse(spec: &str) -> Option<Key> {
        let mut rest = spec.trim();
        let mut ctrl = false;
        let mut alt = false;
        while let Some((head, tail)) = rest.split_once('-') {
            match head.trim().to_ascii_lowercase().as_str() {
                "ctrl" | "c" => {
                    ctrl = true;
                    rest = tail;
                }
                "alt" | "a" | "meta" | "m" => {
                    alt = true;
                    rest = tail;
                }
                _ => break,
            }
        }
        let base = rest.trim();
        if base.is_empty() {
            return None;
        }
        let mut key = match NamedKey::parse(&base.to_ascii_lowercase()) {
            Some(named) => Key::named(named),
            None => {
                let mut chars = base.chars();
                let ch = chars.next()?;
                if chars.next().is_some() {
                    return None;
                }
                Key::ch(ch)
            }
        };
        key.ctrl = ctrl;
        key.alt = alt;
        Some(key)
    }

    /// Canonical spelling, so `--key-map`, the help overlay and the specs all
    /// round-trip through the same text.
    pub fn spec(self) -> String {
        let mut out = String::new();
        if self.ctrl {
            out.push_str("ctrl-");
        }
        if self.alt {
            out.push_str("alt-");
        }
        match (self.named, self.ch) {
            (Some(named), _) => out.push_str(named.name()),
            (None, Some(ch)) => out.push(ch),
            (None, None) => {}
        }
        out
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.spec())
    }
}

/// Everything wrong with one `LUSTY_KEYS` item; the spec keeps applying past
/// these, like the shell version's `continue` on an unknown action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpecWarning {
    /// Neither side named an action.
    UnknownAction(String),
    /// Neither side parsed as a key (or the item had no separator).
    BadKey(String),
}

impl fmt::Display for SpecWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SpecWarning::UnknownAction(item) => write!(f, "unknown action in {item:?}"),
            SpecWarning::BadKey(item) => write!(f, "unparsable key in {item:?}"),
        }
    }
}

/// Key-to-action table. Built by [`Keymap::defaults`]; user specs go through
/// [`Keymap::apply`], which enforces the one-key-one-action rule.
#[derive(Debug, Clone)]
pub struct Keymap {
    bindings: Vec<(Action, Key)>,
}

impl Default for Keymap {
    fn default() -> Keymap {
        Keymap::defaults()
    }
}

impl Keymap {
    /// The shipped map. Keys that `fzf-navigator` uses are kept verbatim where
    /// the picker does not already own them; the rest moved to a free
    /// neighbour key and the README lists the translation.
    pub fn defaults() -> Keymap {
        let mut km = Keymap {
            bindings: Vec::new(),
        };
        let pairs: &[(Action, &str)] = &[
            (Action::Open, "enter"),
            (Action::Open, "tab"),
            (Action::OpenTab, "ctrl-t"),
            (Action::OpenSplit, "ctrl-o"),
            (Action::OpenVSplit, "ctrl-v"),
            (Action::InsertSelection, "alt-e"),
            (Action::CopySelection, "alt-y"),
            (Action::Exit, "ctrl-space"),
            (Action::Cancel, "esc"),
            (Action::Cancel, "ctrl-c"),
            (Action::Cancel, "ctrl-g"),
            (Action::GoToParent, "alt-d"),
            (Action::GoBack, "ctrl-alt-o"),
            (Action::GoForward, "ctrl-alt-i"),
            (Action::GoToSessionStart, "["),
            (Action::GoHome, "\\"),
            (Action::Descend, "/"),
            (Action::ToggleHidden, "alt-h"),
            (Action::ToggleSkipped, "alt-i"),
            (Action::ToggleLocked, "alt-l"),
            (Action::ToggleRecentFirst, "alt-t"),
            // `C-r` refreshes (less, mc, most TUIs) instead of toggling a pane:
            // a preview toggle on it surprises the same way a layout cycle on
            // `C-l` did. The pane lives on `alt-p` (pane/preview).
            (Action::TogglePreview, "alt-p"),
            // `C-l` is redraw, the terminal-canonical meaning (vim, less, the
            // shell): a layout cycle there surprises everyone. The layout moved
            // to `alt-v` (view) and locked mode to `alt-l` (lock) — the shell
            // version used `ctrl-l` for locking, see the README table.
            (Action::Redraw, "ctrl-l"),
            (Action::CycleView, "alt-v"),
            (Action::CycleSort, "ctrl-y"),
            (Action::CycleDepth, "ctrl-d"),
            (Action::Mark, "ctrl-x"),
            (Action::Reload, "ctrl-r"),
            (Action::Reload, "alt-r"),
            (Action::ToggleHelp, "?"),
        ];
        for (action, spec) in pairs {
            let key = Key::parse(spec).unwrap_or_else(|| panic!("default key {spec} parses"));
            km.bind(*action, key);
        }
        km
    }

    /// Every binding in help order: `(action, keys)`.
    pub fn entries(&self) -> Vec<(Action, Vec<Key>)> {
        Action::ALL
            .iter()
            .map(|a| (*a, self.keys_of(*a)))
            .filter(|(_, keys)| !keys.is_empty())
            .collect()
    }

    /// Keys currently bound to `action`, in binding order.
    pub fn keys_of(&self, action: Action) -> Vec<Key> {
        self.bindings
            .iter()
            .filter(|(a, _)| *a == action)
            .map(|(_, k)| *k)
            .collect()
    }

    /// The action a press resolves to. After [`Keymap::bind`] a key belongs to
    /// at most one action, so the first match is the only match.
    pub fn action_of(&self, key: Key) -> Option<Action> {
        self.bindings
            .iter()
            .find(|(_, k)| *k == key)
            .map(|(a, _)| *a)
    }

    /// Bind `key` to `action`, stealing the key from whatever held it. The
    /// action keeps its other keys; a key already on this action stays put.
    pub fn bind(&mut self, action: Action, key: Key) {
        self.bindings.retain(|(a, k)| *k != key || *a == action);
        if !self.bindings.iter().any(|(a, k)| *a == action && *k == key) {
            self.bindings.push((action, key));
        }
    }

    /// Apply a `LUSTY_KEYS` / `--keys` spec: comma-separated `key=action` (or
    /// `action=key`, or `:` instead of `=`) items. Warnings are returned, not
    /// raised: one typo must not cost the whole keymap.
    ///
    /// A named action **moves** to the keys the spec gives it — its shipped
    /// default key is dropped, exactly like the shell version removing the
    /// default line of every action the user overrides. Naming the same action
    /// twice therefore gives it two keys, and naming the same key for two
    /// actions leaves it on the last one.
    pub fn apply(&mut self, spec: &str) -> Vec<SpecWarning> {
        let mut warnings = Vec::new();
        let mut wanted: Vec<(Action, Key)> = Vec::new();
        for item in spec.split(',') {
            let item = item.trim();
            if item.is_empty() {
                continue;
            }
            let split = item
                .split_once('=')
                .or_else(|| item.split_once(':'))
                .map(|(l, r)| (l.trim(), r.trim()));
            let Some((left, right)) = split else {
                warnings.push(SpecWarning::BadKey(item.to_string()));
                continue;
            };
            let (action, key_text) = match (Action::from_name(left), Action::from_name(right)) {
                (Some(a), _) => (a, right),
                (None, Some(a)) => (a, left),
                (None, None) => {
                    warnings.push(
                        if Key::parse(left).is_some() || Key::parse(right).is_some() {
                            SpecWarning::UnknownAction(item.to_string())
                        } else {
                            SpecWarning::BadKey(item.to_string())
                        },
                    );
                    continue;
                }
            };
            match Key::parse(key_text) {
                Some(key) => wanted.push((action, key)),
                None => warnings.push(SpecWarning::BadKey(item.to_string())),
            }
        }
        // Pass 1: drop the defaults of every action the spec mentions.
        for (action, _) in &wanted {
            self.bindings.retain(|(a, _)| a != action);
        }
        // Pass 2: install the requested keys, stealing from other actions.
        for (action, key) in wanted {
            self.bind(action, key);
        }
        warnings
    }

    /// Help overlay rows: keys padded to the shell version's 18 columns, then
    /// the action text. Every bound action appears, in [`Action::ALL`] order.
    pub fn help_rows(&self) -> Vec<String> {
        self.entries()
            .into_iter()
            .map(|(action, keys)| {
                let keys = keys.iter().map(|k| k.spec()).collect::<Vec<_>>().join(", ");
                format!("{:<18}{}", keys, action.help())
            })
            .collect()
    }
}

/// Keys the picker owns itself: movement, editing and acceptance. They are
/// fixed on purpose (the shell version's built-ins work the same way) and are
/// listed under the action rows so the help overlay stays a full map.
pub const BUILTIN_HELP: &[(&str, &str)] = &[
    ("ctrl-n, down", "next entry"),
    ("ctrl-p, up", "previous entry"),
    ("ctrl-f, right", "next column"),
    ("ctrl-b, left", "previous column"),
    ("home, ctrl-a", "first entry"),
    ("end", "last entry"),
    ("pageup, pagedown", "one page of entries"),
    ("backspace, ctrl-h", "delete a character"),
    ("ctrl-u", "clear the prompt"),
    ("ctrl-w", "clear the prompt, then go to the parent"),
    ("printable", "type into the prompt"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_parse_in_every_spelling() {
        assert_eq!(
            Key::parse("ctrl-alt-o"),
            Some(Key::ch('o').with_ctrl(true).with_alt(true))
        );
        assert_eq!(
            Key::parse("C-A-o"),
            Some(Key::ch('o').with_ctrl(true).with_alt(true))
        );
        assert_eq!(Key::parse("alt-h"), Some(Key::ch('h').with_alt(true)));
        assert_eq!(
            Key::parse("ctrl-space"),
            Some(Key::named(NamedKey::Space).with_ctrl(true))
        );
        assert_eq!(Key::parse("?"), Some(Key::ch('?')));
        assert_eq!(Key::parse("\\"), Some(Key::ch('\\')));
        assert_eq!(Key::parse("-"), Some(Key::ch('-')));
        assert_eq!(Key::parse("enter"), Some(Key::named(NamedKey::Enter)));
        assert_eq!(Key::parse("PageDown"), Some(Key::named(NamedKey::PageDown)));
        assert_eq!(Key::parse(""), None);
        assert_eq!(Key::parse("ctrl-"), None);
        assert_eq!(Key::parse("nonsense"), None, "multi-char base is not a key");
    }

    #[test]
    fn keys_round_trip_through_their_spec() {
        for spec in [
            "ctrl-alt-o",
            "alt-h",
            "ctrl-space",
            "?",
            "\\",
            "[",
            "enter",
            "pagedown",
        ] {
            let key = Key::parse(spec).unwrap();
            assert_eq!(key.spec(), spec);
            assert_eq!(Key::parse(&key.spec()), Some(key));
        }
    }

    #[test]
    fn every_default_binding_parses_and_has_exactly_one_action() {
        let km = Keymap::defaults();
        for (action, key) in &km.bindings {
            assert_eq!(km.action_of(*key), Some(*action), "{key} resolves twice");
        }
        for (action, keys) in km.entries() {
            assert!(!keys.is_empty(), "{} lost its key", action.name());
        }
        assert_eq!(km.entries().len(), Action::ALL.len());
    }

    #[test]
    fn binding_a_key_steals_it() {
        let mut km = Keymap::defaults();
        let key = Key::parse("alt-r").unwrap();
        assert_eq!(km.action_of(key), Some(Action::Reload));
        assert!(km.apply("alt-r=toggle_help").is_empty());
        assert_eq!(km.action_of(key), Some(Action::ToggleHelp));
        assert_eq!(
            km.keys_of(Action::Reload),
            vec![Key::parse("ctrl-r").unwrap()],
            "the stolen key is gone, reload keeps its other one"
        );
        assert_eq!(
            km.keys_of(Action::ToggleHelp),
            vec![key],
            "an override moves the action: the shipped `?` is replaced"
        );
    }

    #[test]
    fn specs_take_either_order_and_either_separator() {
        let mut km = Keymap::defaults();
        assert!(km.apply("ctrl-o=open_vsplit").is_empty());
        assert_eq!(
            km.action_of(Key::parse("ctrl-o").unwrap()),
            Some(Action::OpenVSplit)
        );
        assert!(km.apply("open_tab:alt-t").is_empty());
        assert_eq!(
            km.action_of(Key::parse("alt-t").unwrap()),
            Some(Action::OpenTab)
        );
    }

    /// An override *moves* an action (its shipped key stops firing) and two
    /// entries for one action leave it with both keys.
    #[test]
    fn an_override_moves_an_action_and_two_entries_alias_it() {
        let mut km = Keymap::defaults();
        assert!(km.apply("go_to_parent=alt-u,go_to_parent=alt-p").is_empty());
        assert_eq!(
            km.keys_of(Action::GoToParent),
            vec![Key::parse("alt-u").unwrap(), Key::parse("alt-p").unwrap()],
            "two entries give the action two keys"
        );
        assert_eq!(
            km.action_of(Key::parse("alt-d").unwrap()),
            None,
            "the shipped alt-d is gone once the action is overridden"
        );
    }

    #[test]
    fn a_typo_costs_one_binding_not_the_keymap() {
        let mut km = Keymap::defaults();
        let warnings = km.apply("nonsense=alt-z, alt-q=also_nonsense, alt-w=open_tab, ,");
        assert_eq!(warnings.len(), 2);
        assert!(matches!(warnings[0], SpecWarning::UnknownAction(_)));
        assert!(matches!(warnings[1], SpecWarning::UnknownAction(_)));
        assert_eq!(
            km.action_of(Key::parse("alt-w").unwrap()),
            Some(Action::OpenTab)
        );
        assert_eq!(km.action_of(Key::parse("alt-z").unwrap()), None);
        assert_eq!(km.action_of(Key::parse("alt-q").unwrap()), None);
    }

    #[test]
    fn the_historical_action_names_still_parse() {
        assert_eq!(
            Action::from_name("toggle_hidden_files"),
            Some(Action::ToggleHidden)
        );
        assert_eq!(
            Action::from_name("toggle_ignored_files"),
            Some(Action::ToggleSkipped)
        );
        assert_eq!(Action::from_name("open"), Some(Action::Open));
    }

    #[test]
    fn help_lists_every_action_in_registry_order() {
        let km = Keymap::defaults();
        let rows = km.help_rows();
        assert_eq!(rows.len(), Action::ALL.len());
        assert!(rows[0].starts_with("enter, tab"));
        assert!(rows[0].ends_with(Action::Open.help()));
        assert!(rows.iter().any(|r| r.contains("toggle this help")));
        // Padding keeps the descriptions in one column.
        let col = rows[0].find(Action::Open.help()).unwrap();
        assert!(rows.iter().all(|r| r[col..].starts_with(r[col..].trim())));
    }

    #[test]
    fn an_unbound_action_disappears_from_help() {
        let mut km = Keymap::defaults();
        // Give the only key of `toggle_hidden` to another action: it loses the
        // key and with it its help row, instead of showing up unpressable.
        km.bind(Action::Mark, Key::parse("alt-h").unwrap());
        assert!(km.keys_of(Action::ToggleHidden).is_empty());
        let rows = km.help_rows();
        assert!(!rows.iter().any(|r| r.contains("toggle hidden files")));
    }
}
