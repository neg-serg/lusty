//! Serve preview: the `V` request answers with a count-framed preview pane
//! (`V <lines> <dim>`, one `L <text>` row per line, `E`), the `X preview`
//! capability line announces the feature and a content line equal to `E` must
//! not terminate the response early.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};

fn root_dir() -> PathBuf {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir =
        std::env::temp_dir().join(format!("lusty_serve_preview_{}_{}", std::process::id(), n));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("subdir")).unwrap();
    // A content line equal to the terminator exercises the framing.
    std::fs::write(dir.join("x.txt"), b"E\nhello world\n").unwrap();
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

fn send(stdin: &mut ChildStdin, line: &str) {
    writeln!(stdin, "{line}").unwrap();
    stdin.flush().unwrap();
}

/// Entry index of a label from the Q rows.
fn index_of(resp: &[String], label: &str) -> usize {
    for l in resp {
        let Some(rest) = l.strip_prefix("R ") else {
            continue;
        };
        let mut sp = rest.splitn(3, ' ');
        let i: usize = sp.next().unwrap().parse().unwrap();
        let _kind = sp.next().unwrap();
        let lab = sp.next().unwrap().split('\t').next().unwrap();
        if lab == label {
            return i;
        }
    }
    panic!("no R row for {label} in {resp:?}");
}

/// Read a `V` response and return (dim, lines).
fn read_preview(
    lines: &mut std::io::Lines<BufReader<std::process::ChildStdout>>,
) -> (bool, Vec<String>) {
    let header = lines.next().expect("V header").expect("read V header");
    let caps = header.strip_prefix("V ").expect("V header prefix");
    let mut it = caps.split(' ');
    let n: usize = it.next().unwrap().parse().unwrap();
    let dim = it.next().unwrap() == "1";
    let mut body = Vec::with_capacity(n);
    for _ in 0..n {
        let l = lines
            .next()
            .expect("preview line")
            .expect("read preview line");
        let text = l
            .strip_prefix("L ")
            .unwrap_or_else(|| panic!("L prefix: {l:?}"));
        body.push(text.to_string());
    }
    let end = lines
        .next()
        .expect("V terminator")
        .expect("read V terminator");
    assert_eq!(end, "E", "terminator after exactly {n} lines");
    (dim, body)
}

#[test]
fn serve_preview_frames_content() {
    let dir = root_dir();
    let (mut child, mut stdin, mut lines) = spawn(&dir);
    let c = lines.next().unwrap().unwrap();
    assert!(c.starts_with("C "), "banner: {c}");
    assert_eq!(
        lines.next().unwrap().unwrap(),
        "X preview",
        "capability line"
    );

    send(&mut stdin, "Q\t0\t50\t");
    let resp = until_e(&mut lines);
    let file = index_of(&resp, "x.txt");
    let subdir = index_of(&resp, "subdir");

    // Text file: content, and the literal "E" line survives as "L E".
    send(&mut stdin, &format!("V\t{file}\t40\t5"));
    let (dim, body) = read_preview(&mut lines);
    assert!(dim, "text preview is dim");
    assert_eq!(body, vec!["E".to_string(), "hello world".to_string()]);

    // The width clips the rows (the server floor is 8 columns).
    send(&mut stdin, &format!("V\t{file}\t8\t5"));
    let (_, body) = read_preview(&mut lines);
    assert_eq!(body, vec!["E".to_string(), "hello wo".to_string()]);

    // Directory placeholder.
    send(&mut stdin, &format!("V\t{subdir}\t40\t5"));
    let (dim, body) = read_preview(&mut lines);
    assert!(dim);
    assert_eq!(body.len(), 1);
    assert!(body[0].starts_with("directory:"), "{body:?}");

    drop(stdin);
    assert!(child.wait().unwrap().success());
    let _ = std::fs::remove_dir_all(&dir);
}
