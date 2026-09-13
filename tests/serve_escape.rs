//! Protocol hardening: names containing TAB / LF / backslash are escaped on
//! the wire (so they cannot break the line framing), and non-UTF8 names are
//! sent as raw bytes instead of being lossily replaced.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

fn make_dir(tag: &str) -> PathBuf {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lusty_serve_{tag}_{}_{}", std::process::id(), n));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn spawn(dir: &Path) -> (Child, ChildStdin, BufReader<ChildStdout>) {
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
    (child, stdin, BufReader::new(stdout))
}

/// Read one line as raw bytes (a non-UTF8 path must not go through String).
fn read_raw_line(reader: &mut BufReader<ChildStdout>) -> Option<Vec<u8>> {
    let mut buf = Vec::new();
    let n = reader.read_until(b'\n', &mut buf).ok()?;
    if n == 0 {
        return None;
    }
    while matches!(buf.last(), Some(b'\n') | Some(b'\r')) {
        buf.pop();
    }
    Some(buf)
}

fn until_e(reader: &mut BufReader<ChildStdout>) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    while let Some(line) = read_raw_line(reader) {
        if line == b"E" {
            break;
        }
        out.push(line);
    }
    out
}

/// Reverse the serve-side escaping (`\t`, `\n`, `\\`).
fn unescape(b: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' && i + 1 < b.len() {
            match b[i + 1] {
                b't' => out.push(b'\t'),
                b'n' => out.push(b'\n'),
                b'\\' => out.push(b'\\'),
                c => out.push(c),
            }
            i += 2;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    out
}

#[test]
fn serve_escapes_special_and_non_utf8_names() {
    let dir = make_dir("escape");
    std::fs::write(dir.join("tab\tname"), b"x").unwrap();
    std::fs::write(dir.join("nl\nname"), b"x").unwrap();
    std::fs::write(dir.join("back\\slash"), b"x").unwrap();
    let raw_name = std::ffi::OsStr::from_bytes(b"caf\xe9.txt");
    std::fs::write(dir.join(raw_name), b"x").unwrap();

    let (mut child, mut stdin, mut reader) = spawn(&dir);
    let banner = read_raw_line(&mut reader).expect("C line");
    assert!(banner.starts_with(b"C 4 1 "), "banner: {banner:?}");

    writeln!(stdin, "Q\t0\t50\t").unwrap();
    stdin.flush().unwrap();
    let resp = until_e(&mut reader);

    let prefix = dir.as_os_str().as_bytes();
    let mut got: Vec<Vec<u8>> = Vec::new();
    for row in resp.iter().filter(|r| r.starts_with(b"R ")) {
        // "R <i> <kind> <escaped label>\t<escaped path>": exactly one raw TAB,
        // because a TAB inside a name is written as the two bytes `\t`.
        assert_eq!(
            row.iter().filter(|&&b| b == b'\t').count(),
            1,
            "row must carry one raw TAB: {row:?}"
        );
        assert!(!row.contains(&b'\n'), "no raw LF inside a row: {row:?}");

        let tab = row.iter().position(|&b| b == b'\t').unwrap();
        let mut head = row[..tab].splitn(4, |&b| b == b' ');
        assert_eq!(head.next(), Some(b"R".as_slice()));
        assert!(head.next().is_some(), "index field");
        assert!(head.next().is_some(), "kind field");
        let label = unescape(head.next().expect("label field"));
        let path = unescape(&row[tab + 1..]);

        assert!(
            path.starts_with(prefix),
            "path must stay under the root: {path:?}"
        );
        let rel = &path[prefix.len() + 1..];
        // Valid UTF-8 labels survive byte-for-byte; the non-UTF8 one is the
        // lossy display form (U+FFFD) while the path keeps the real bytes.
        if label.windows(3).any(|w| w == [0xEF, 0xBF, 0xBD]) {
            assert_eq!(rel, b"caf\xe9.txt", "lossy label, raw path");
        } else {
            assert_eq!(label.as_slice(), rel, "label == rel path");
        }
        got.push(path);
    }

    for name in [
        &b"tab\tname"[..],
        &b"nl\nname"[..],
        &b"back\\slash"[..],
        &b"caf\xe9.txt"[..],
    ] {
        let mut want = prefix.to_vec();
        want.push(b'/');
        want.extend_from_slice(name);
        assert!(got.contains(&want), "missing path {want:?} in {got:?}");
    }

    drop(stdin);
    assert!(child.wait().unwrap().success());
    let _ = std::fs::remove_dir_all(&dir);
}
