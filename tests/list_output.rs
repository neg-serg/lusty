//! `--list` is plumbing, but its output is read on terminals often enough:
//! a file name carrying ESC must not be able to drive the terminal.

use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;

fn make_dir(tag: &str) -> PathBuf {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lusty_list_{tag}_{}_{}", std::process::id(), n));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn list(dir: &PathBuf, extra: &[&str]) -> std::process::Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_lusty"))
        .arg("--list")
        .arg(dir)
        .args(extra)
        .output()
        .expect("run lusty --list")
}

#[test]
fn list_output_replaces_control_bytes() {
    let dir = make_dir("ctrl");
    std::fs::write(
        dir.join(std::ffi::OsStr::from_bytes(b"esc_\x1b[2J_name")),
        b"x",
    )
    .unwrap();
    std::fs::write(dir.join("tab\tname"), b"x").unwrap();
    std::fs::write(dir.join("plain.txt"), b"x").unwrap();

    let out = list(&dir, &["--depth", "1"]);
    assert!(out.status.success(), "exit: {:?}", out.status);
    assert!(
        !out.stdout.contains(&0x1b),
        "raw ESC on stdout: {:?}",
        String::from_utf8_lossy(&out.stdout)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("esc_?[2J_name"),
        "the ESC is replaced by '?': {text:?}"
    );
    assert!(
        text.contains("tab name"),
        "the TAB becomes a space: {text:?}"
    );
    assert!(
        text.contains("plain.txt"),
        "plain names unchanged: {text:?}"
    );

    // Colour output wraps the label in SGR, so the same rule has to hold there.
    let colored = list(&dir, &["--depth", "1", "--color"]);
    assert!(colored.status.success());
    let raw = String::from_utf8_lossy(&colored.stdout);
    assert!(
        raw.contains("esc_?[2J_name"),
        "sanitized even when wrapped in SGR: {raw:?}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
