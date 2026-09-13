//! Serve-protocol integration test for the Q dirs_first/reverse flags.
//! They reshape only the canonical (sort 0) listing, mirroring the
//! standalone TUI; other sort keys ignore them.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};

fn root_dir() -> std::path::PathBuf {
    // One dir per call: integration tests run in parallel threads and a
    // shared pid-based path made them race.
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir =
        std::env::temp_dir().join(format!("lusty_serve_dirs_rev_{}_{}", std::process::id(), n));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("zeta_dir")).unwrap();
    std::fs::write(dir.join("alpha.txt"), b"hello").unwrap();
    std::fs::write(dir.join("beta.lua"), b"hello world").unwrap();
    std::fs::write(dir.join("gamma.rs"), b"fn main(){}").unwrap();
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
        .arg("1")
        .arg("--skip")
        .arg("")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn lusty serve");
    let stdin = child.stdin.take().expect("serve stdin");
    let stdout = child.stdout.take().expect("serve stdout");
    let lines = BufReader::new(stdout).lines();
    (child, stdin, lines)
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

fn ask(stdin: &mut ChildStdin, req: &str) {
    writeln!(stdin, "{req}").unwrap();
    stdin.flush().unwrap();
}

fn labels(resp: &[String]) -> Vec<&str> {
    resp.iter()
        .filter(|l| l.starts_with("R "))
        .map(|l| {
            let rest = &l[2..];
            let mut sp = rest.splitn(3, ' ');
            let _ = sp.next().unwrap(); // index
            let _ = sp.next().unwrap(); // kind
            sp.next().unwrap().split('\t').next().unwrap()
        })
        .collect()
}

fn close(child: &mut Child, stdin: ChildStdin) {
    drop(stdin);
    assert!(child.wait().unwrap().success());
}

#[test]
fn q_dirs_first_groups_dirs() {
    let dir = root_dir();
    let (mut child, mut stdin, mut lines) = spawn(&dir);
    let _ = lines.next().unwrap().unwrap(); // C line
    let _ = lines.next().unwrap().unwrap(); // X capability line

    ask(&mut stdin, "Q\t0\t50\t\t0\t1\t0"); // sort 0, dirs_first
    let resp = until_e(&mut lines);
    assert_eq!(
        labels(&resp),
        ["zeta_dir", "alpha.txt", "beta.lua", "gamma.rs"]
    );

    // Back to canonical: the dirs reorder must not stack.
    ask(&mut stdin, "Q\t0\t50\t\t0\t0\t0");
    let resp = until_e(&mut lines);
    assert_eq!(
        labels(&resp),
        ["alpha.txt", "beta.lua", "gamma.rs", "zeta_dir"]
    );

    close(&mut child, stdin);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn q_reverse_reverses_depth_group() {
    let dir = root_dir();
    let (mut child, mut stdin, mut lines) = spawn(&dir);
    let _ = lines.next().unwrap().unwrap(); // C line
    let _ = lines.next().unwrap().unwrap(); // X capability line

    ask(&mut stdin, "Q\t0\t50\t\t0\t0\t1"); // sort 0, reverse
    let resp = until_e(&mut lines);
    assert_eq!(
        labels(&resp),
        ["zeta_dir", "gamma.rs", "beta.lua", "alpha.txt"]
    );

    close(&mut child, stdin);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn q_flags_inert_for_other_sorts() {
    let dir = root_dir();
    let (mut child, mut stdin, mut lines) = spawn(&dir);
    let _ = lines.next().unwrap().unwrap(); // C line
    let _ = lines.next().unwrap().unwrap(); // X capability line

    ask(&mut stdin, "Q\t0\t50\t\t1\t0\t0"); // ext sort, no flags
    let plain_resp = until_e(&mut lines);
    let plain = labels(&plain_resp);
    ask(&mut stdin, "Q\t0\t50\t\t1\t1\t1"); // same sort, flags ignored
    let flagged_resp = until_e(&mut lines);
    let flagged = labels(&flagged_resp);
    assert_eq!(plain, flagged);

    close(&mut child, stdin);
    let _ = std::fs::remove_dir_all(&dir);
}
