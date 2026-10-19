//! A root that is not a directory is a usage error: every front-end has to say
//! so and exit 2 instead of printing an empty listing and exiting 0.

use std::path::PathBuf;
use std::process::{Command, Stdio};

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lusty_root_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_lusty"))
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("run lusty")
}

#[test]
fn a_missing_root_is_a_usage_error() {
    let missing = "/nonexistent-lusty-root";
    // `--list`, `serve` and the TUI all start from a root: each must refuse.
    for args in [
        vec!["--list", missing],
        vec!["serve", missing],
        vec![missing],
    ] {
        let out = run(&args);
        assert_eq!(
            out.status.code(),
            Some(2),
            "args {args:?} should exit 2, got {:?}",
            out.status
        );
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(
            err.contains("lusty:") && err.contains(missing),
            "args {args:?}: stderr {err:?}"
        );
        assert!(
            out.stdout.is_empty(),
            "args {args:?}: no listing on stdout, got {:?}",
            String::from_utf8_lossy(&out.stdout)
        );
    }
}

#[test]
fn a_file_root_is_a_usage_error() {
    let dir = tmp("file");
    let file = dir.join("not-a-dir.txt");
    std::fs::write(&file, b"x").unwrap();
    let path = file.to_str().expect("utf-8 temp path");

    for args in [vec!["--list", path], vec!["serve", path], vec![path]] {
        let out = run(&args);
        assert_eq!(
            out.status.code(),
            Some(2),
            "args {args:?}: {:?}",
            out.status
        );
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(
            err.contains("not a directory"),
            "args {args:?}: stderr {err:?}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_real_directory_still_works() {
    let dir = tmp("ok");
    std::fs::write(dir.join("f.txt"), b"x").unwrap();
    let out = run(&["--list", dir.to_str().unwrap(), "--depth", "1"]);
    assert_eq!(out.status.code(), Some(0), "{:?}", out.status);
    assert!(String::from_utf8_lossy(&out.stdout).contains("f.txt"));
    let _ = std::fs::remove_dir_all(&dir);
}
