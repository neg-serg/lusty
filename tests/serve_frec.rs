//! Serve frecency: the client sends `F <score> <path>` records after the
//! banner, and the empty query then orders the higher-scored paths first
//! while keeping the shallower depth first.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};

fn root_dir() -> PathBuf {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lusty_serve_frec_{}_{}", std::process::id(), n));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::write(dir.join("aaa.txt"), b"x").unwrap();
    std::fs::write(dir.join("zzz.txt"), b"x").unwrap();
    std::fs::write(dir.join("tab\tname.txt"), b"x").unwrap();
    std::fs::write(dir.join("sub/deep.txt"), b"x").unwrap();
    dir
}

fn spawn(
    dir: &Path,
) -> (
    Child,
    ChildStdin,
    std::io::Lines<BufReader<std::process::ChildStdout>>,
) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_lusty"))
        .arg("serve")
        .arg(dir)
        .arg("--depth")
        .arg("2")
        .arg("--skip")
        .arg("")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn lusty serve");
    let stdin = child.stdin.take().expect("serve stdin");
    let stdout = child.stdout.take().expect("serve stdout");
    (child, stdin, BufReader::new(stdout).lines())
}

fn until_e(lines: &mut std::io::Lines<BufReader<std::process::ChildStdout>>) -> Vec<String> {
    let mut out = Vec::new();
    for line in lines {
        let line = line.expect("read serve line");
        if line == "E" {
            break;
        }
        out.push(line);
    }
    out
}

/// Reverse the serve escaping for the `R` path field.
fn unescape(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' && i + 1 < b.len() {
            match b[i + 1] {
                b't' => out.push('\t'),
                b'n' => out.push('\n'),
                b'\\' => out.push('\\'),
                c => out.push(c as char),
            }
            i += 2;
        } else {
            out.push(b[i] as char);
            i += 1;
        }
    }
    out
}

/// Paths of the R rows in order.
fn row_paths(resp: &[String]) -> Vec<String> {
    resp.iter()
        .filter(|l| l.starts_with("R "))
        .map(|l| {
            let tab = l.find('\t').expect("row separator");
            unescape(&l[tab + 1..])
        })
        .collect()
}

fn send(stdin: &mut ChildStdin, line: &str) {
    writeln!(stdin, "{line}").unwrap();
    stdin.flush().unwrap();
}

#[test]
fn frecency_orders_the_empty_query_within_depth() {
    let dir = root_dir();
    let (mut child, mut stdin, mut lines) = spawn(&dir);
    let _ = lines.next().unwrap().unwrap(); // C banner

    // No F yet: canonical (depth, name) order.
    send(&mut stdin, "Q\t0\t50\t");
    let canonical = row_paths(&until_e(&mut lines));
    assert_eq!(
        canonical,
        vec![
            dir.join("aaa.txt").display().to_string(),
            dir.join("sub").display().to_string(),
            dir.join("tab\tname.txt").display().to_string(),
            dir.join("zzz.txt").display().to_string(),
            dir.join("sub/deep.txt").display().to_string(),
        ],
        "canonical order without frecency"
    );

    // Score zzz and the TAB name (escaped on the wire, as native.lua sends it).
    let zzz = dir.join("zzz.txt").display().to_string();
    let tab = dir
        .join("tab\tname.txt")
        .display()
        .to_string()
        .replace('\t', "\\t");
    send(&mut stdin, &format!("F\t100\t{zzz}"));
    send(&mut stdin, &format!("F\t50\t{tab}"));

    // New F records must re-rank even though the query is unchanged.
    send(&mut stdin, "Q\t0\t50\t");
    let ranked = row_paths(&until_e(&mut lines));
    assert_eq!(ranked[0], zzz, "most frequent file first");
    assert_eq!(ranked[1], dir.join("tab\tname.txt").display().to_string());
    // Shallow contract: every depth-1 path precedes the depth-2 one.
    let deep = ranked.iter().position(|p| p.ends_with("deep.txt")).unwrap();
    assert_eq!(deep, ranked.len() - 1, "depth 2 still last: {ranked:?}");
    assert_eq!(
        ranked.last().unwrap(),
        &dir.join("sub/deep.txt").display().to_string()
    );

    drop(stdin);
    assert!(child.wait().unwrap().success());
    let _ = std::fs::remove_dir_all(&dir);
}
