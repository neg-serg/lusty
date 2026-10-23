//! The parity dumps the nvim front-end reads: `--key-map` (action -> keys) and
//! `--icon-color-map` (name -> the theme.yml icon colour).
//!
//! They are the contract that keeps the two front-ends on one keymap and one
//! icon palette, so they are pinned here: every action appears exactly once,
//! the ported keys are the ones the README documents, and `LUSTY_KEYS`
//! overrides show up in the dump.

use std::process::{Command, Stdio};

fn run(args: &[&str], envs: &[(&str, &str)]) -> String {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_lusty"));
    cmd.args(args).stdin(Stdio::null());
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("run lusty");
    assert!(
        out.status.success(),
        "lusty {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn rows(dump: &str) -> Vec<(String, String)> {
    dump.lines()
        .map(|line| {
            let (action, keys) = line.split_once('\t').expect("tab-separated dump");
            (action.to_string(), keys.to_string())
        })
        .collect()
}

#[test]
fn the_key_map_covers_every_action_once() {
    let dump = run(&["--key-map"], &[]);
    let rows = rows(&dump);
    assert!(rows.len() >= 20, "the whole registry is dumped: {dump}");
    for (action, keys) in &rows {
        assert!(!keys.is_empty(), "{action} has no key: {dump}");
        assert_eq!(
            rows.iter().filter(|(a, _)| a == action).count(),
            1,
            "{action} listed twice"
        );
    }
    // The ported defaults, one per ported action.
    let keys_of = |name: &str| {
        rows.iter()
            .find(|(a, _)| a == name)
            .map(|(_, k)| k.clone())
            .unwrap_or_else(|| panic!("{name} missing from {dump}"))
    };
    assert_eq!(keys_of("go_to_parent"), "alt-d");
    assert_eq!(keys_of("go_back"), "ctrl-alt-o");
    assert_eq!(keys_of("go_forward"), "ctrl-alt-i");
    assert_eq!(keys_of("go_to_session_start"), "[");
    assert_eq!(keys_of("go_home"), "\\");
    assert_eq!(keys_of("descend"), "/");
    assert_eq!(keys_of("cancel"), "esc ctrl-c ctrl-g");
    assert_eq!(keys_of("exit"), "ctrl-space");
    assert_eq!(keys_of("toggle_hidden"), "alt-h");
    assert_eq!(keys_of("toggle_skipped"), "alt-i");
    assert_eq!(keys_of("toggle_locked"), "alt-l");
    assert_eq!(keys_of("toggle_recent_first"), "alt-t");
    assert_eq!(keys_of("cycle_view"), "alt-v");
    assert_eq!(keys_of("redraw"), "ctrl-l");
    assert_eq!(keys_of("toggle_preview"), "alt-p");
    assert_eq!(keys_of("reload"), "ctrl-r alt-r");
    assert_eq!(keys_of("toggle_help"), "?");
    assert_eq!(keys_of("insert_selection"), "alt-e");
    assert_eq!(keys_of("copy_selection"), "alt-y");
    // Letters stay typeable: no bare alphanumeric key may be an action.
    for (action, keys) in &rows {
        for key in keys.split(' ') {
            let printable_alpha =
                key.chars().count() == 1 && key.chars().all(|c| c.is_ascii_alphanumeric());
            assert!(
                !printable_alpha,
                "{action} is bound to the typeable key {key}"
            );
        }
    }
}

#[test]
fn lusty_keys_overrides_are_reflected_in_the_dump() {
    let dump = run(
        &["--key-map"],
        &[("LUSTY_KEYS", "go_to_parent=alt-u,toggle_help:alt-z")],
    );
    let parsed = rows(&dump);
    let keys_of = |name: &str| {
        parsed
            .iter()
            .find(|(a, _)| a == name)
            .map(|(_, k)| k.clone())
            .unwrap()
    };
    assert_eq!(keys_of("go_to_parent"), "alt-u");
    assert_eq!(
        keys_of("toggle_help"),
        "alt-z",
        "an override replaces the shipped key instead of aliasing it"
    );
    // A typo costs one binding, not the dump.
    let dump = run(&["--key-map"], &[("LUSTY_KEYS", "nonsense=alt-q")]);
    assert!(rows(&dump).iter().any(|(a, _)| a == "toggle_help"));
}

#[test]
fn icon_colours_come_from_the_shipped_theme() {
    let dump = run(
        &[
            "--icon-color-map",
            "main.rs",
            "Makefile",
            "no-such-extension",
        ],
        &[("LUSTY_ICON_THEME", "off")],
    );
    let rows = rows(&dump);
    assert_eq!(rows[0], ("main.rs".to_string(), "dea584".to_string()));
    assert_eq!(rows[1], ("Makefile".to_string(), "6d8086".to_string()));
    assert_eq!(
        rows[2],
        ("no-such-extension".to_string(), String::new()),
        "an unknown name has no colour, so the LS_COLORS foreground stays"
    );
}
