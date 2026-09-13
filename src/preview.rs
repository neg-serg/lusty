//! Right-pane preview for the standalone TUI (roadmap item E).
//!
//! Content types rendered as plain/ANSI text lines that fit inside the
//! bordered popup:
//!   - images: chafa ANSI art (full-colour half-blocks; runs in any
//!     terminal, incl. a degraded path when chafa is missing). Native
//!     kitty-protocol placement is a later upgrade.
//!   - git diff of the selected file when it lives in a work tree
//!   - rendered man-page sources (.1..9 / .man / .gz)
//!
//! The TUI clips lines to the pane width and height; renderers here only
//! produce the raw output lines.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Command output (git diff, man) is capped at this many bytes.
const MAX_CMD_BYTES: usize = 512 * 1024;

fn max_image_bytes() -> u64 {
    std::env::var("LUSTY_PREVIEW_MAX_BYTES")
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(8 * 1024 * 1024)
}

const IMAGE_EXTS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "avif", "ico",
];

pub fn is_image(path: &Path) -> bool {
    match path.extension().and_then(|e| e.to_str()) {
        Some(e) => {
            let low = e.to_ascii_lowercase();
            IMAGE_EXTS.contains(&low.as_str())
        }
        None => false,
    }
}

/// True for man-page sources: NAME.(1..9|man)[.gz] or a path under a
/// "man/man<digit>" directory.
pub fn is_man_source(path: &Path) -> bool {
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    let mut base = name;
    if let Some(b) = base.strip_suffix(".gz") {
        base = b;
    }
    let ext = base.rsplit('.').next().unwrap_or("");
    let by_ext =
        (ext.len() == 1 && ext != "0" && ext.as_bytes()[0].is_ascii_digit()) || ext == "man";
    if by_ext {
        return true;
    }
    path.to_string_lossy().contains("/man/man")
}

/// First ancestor directory containing a .git (dir or worktree file).
pub fn git_root(start: &Path) -> Option<PathBuf> {
    let mut dir = start;
    loop {
        if dir.join(".git").exists() {
            return Some(dir.to_path_buf());
        }
        dir = dir.parent()?;
    }
}

/// Render result: `dim` marks plain text (git/man/info) that the TUI dims;
/// chafa art lines carry their own colours (dim = false).
pub struct Pane {
    pub dim: bool,
    pub lines: Vec<String>,
}

pub fn render(path: &Path, is_dir: bool, width: usize, height: usize) -> Pane {
    if is_dir {
        return Pane {
            dim: true,
            lines: vec![format!("directory: {}", path.display())],
        };
    }
    if is_image(path) {
        return Pane {
            dim: false,
            lines: image_lines(path, width, height),
        };
    }
    if is_man_source(path) {
        return Pane {
            dim: true,
            lines: man_lines(path),
        };
    }
    // A file with unstaged changes shows the diff; otherwise (clean file, no
    // work tree, untracked) show the file content, which is what a picker
    // preview is expected to show.
    if let Some(repo) = git_root(path.parent().unwrap_or(path)) {
        let diff = git_diff_lines(path, &repo);
        let clean = diff.len() == 1
            && matches!(
                diff[0].as_str(),
                "(no unstaged changes)" | "(git diff failed)"
            );
        return Pane {
            dim: true,
            lines: if clean {
                content_lines(path, height)
            } else {
                diff
            },
        };
    }
    Pane {
        dim: true,
        lines: content_lines(path, height),
    }
}

/// Largest slice read for the content preview.
const MAX_TEXT_BYTES: usize = 256 * 1024;

/// First `height` lines of a text file (lossy UTF-8, trailing spaces trimmed).
/// A NUL byte in the head marks binary content, which gets a placeholder
/// instead of terminal garbage.
fn content_lines(path: &Path, height: usize) -> Vec<String> {
    use std::io::Read;
    let Ok(mut f) = std::fs::File::open(path) else {
        return vec!["(unreadable)".to_string()];
    };
    let mut buf = vec![0u8; MAX_TEXT_BYTES];
    let n = f.read(&mut buf).unwrap_or(0);
    buf.truncate(n);
    if buf.iter().take(4096).any(|&b| b == 0) {
        let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(n as u64);
        return vec![format!("(binary file, {size} bytes)")];
    }
    let text = String::from_utf8_lossy(&buf);
    let lines: Vec<String> = text
        .lines()
        .take(height)
        .map(|l| l.trim_end().to_string())
        .collect();
    if lines.is_empty() {
        vec!["(empty file)".to_string()]
    } else {
        lines
    }
}

fn image_lines(path: &Path, width: usize, height: usize) -> Vec<String> {
    let len = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    if len > max_image_bytes() {
        return vec![format!(
            "(image is {} bytes > LUSTY_PREVIEW_MAX_BYTES)",
            len
        )];
    }
    let size = format!("{}x{}", width.max(1), height.max(1));
    let out = Command::new("chafa")
        .args(["--format", "symbols", "--colors", "256", "--size"])
        .arg(&size)
        .arg(path)
        .output();
    match out {
        Ok(o) if o.status.success() && !o.stdout.is_empty() => {
            let text = String::from_utf8_lossy(&o.stdout).into_owned();
            text.lines().map(|l| l.trim_end().to_string()).collect()
        }
        Ok(_) => vec!["(image preview unavailable)".to_string()],
        Err(_) => vec!["(chafa not found: no image preview)".to_string()],
    }
}

fn git_diff_lines(path: &Path, repo: &Path) -> Vec<String> {
    let rel = path.strip_prefix(repo).unwrap_or(path);
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["diff", "--no-color", "--no-ext-diff", "--"])
        .arg(rel)
        .output();
    match out {
        Ok(o) if o.status.success() => {
            let mut text = String::from_utf8_lossy(&o.stdout).into_owned();
            if text.len() > MAX_CMD_BYTES {
                text.truncate(MAX_CMD_BYTES);
            }
            if text.trim().is_empty() {
                text = "(no unstaged changes)".to_string();
            }
            text.lines().map(|l| l.trim_end().to_string()).collect()
        }
        Ok(_) => vec!["(git diff failed)".to_string()],
        Err(_) => vec!["(git not found)".to_string()],
    }
}

fn man_lines(path: &Path) -> Vec<String> {
    let out = Command::new("man")
        .env("MANWIDTH", "80")
        .env("MANPAGER", "cat")
        .args(["-l"])
        .arg(path)
        .output();
    match out {
        Ok(o) if o.status.success() => {
            let mut text = String::from_utf8_lossy(&o.stdout).into_owned();
            if text.len() > MAX_CMD_BYTES {
                text.truncate(MAX_CMD_BYTES);
            }
            // Collapse groff overstrike bold/underline (X BS X) so the
            // pane stays readable: drop the backspace and keep one copy.
            let mut clean = String::with_capacity(text.len());
            for ch in text.chars() {
                if ch as u32 == 8 {
                    clean.pop();
                } else {
                    clean.push(ch);
                }
            }
            let lines: Vec<String> = clean
                .lines()
                .map(|l| l.trim_end().to_string())
                .filter(|l| !l.trim().is_empty())
                .collect();
            if lines.is_empty() {
                vec!["(man page rendered empty)".to_string()]
            } else {
                lines
            }
        }
        Ok(_) => vec!["(man render failed)".to_string()],
        Err(_) => vec!["(man not found)".to_string()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn image_ext_detection() {
        assert!(is_image(Path::new("/a/b/photo.PNG")));
        assert!(is_image(Path::new("x.webp")));
        assert!(is_image(Path::new("img.jpeg")));
        assert!(!is_image(Path::new("note.md")));
        assert!(!is_image(Path::new("noext")));
    }

    #[test]
    fn man_source_detection() {
        assert!(is_man_source(Path::new("/usr/share/man/man1/ls.1.gz")));
        assert!(is_man_source(Path::new("prog.5")));
        assert!(is_man_source(Path::new("doc.man")));
        assert!(!is_man_source(Path::new("README.md")));
        assert!(!is_man_source(Path::new("main.rs")));
    }

    #[test]
    fn git_root_walks_up() {
        let tmp = std::env::temp_dir().join(format!("lusty_prev_gr_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("a/b")).unwrap();
        std::fs::write(tmp.join(".git"), b"gitdir: x").unwrap();
        let root = git_root(&tmp.join("a/b"));
        assert_eq!(root, Some(tmp.clone()));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn text_file_renders_its_content() {
        let dir = std::env::temp_dir().join(format!("lusty_prev_text_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("note.txt");
        std::fs::write(&file, b"first line\nsecond line\nthird line\n").unwrap();

        // No .git anywhere above the temp dir, so the fallback is the content.
        let pane = render(&file, false, 40, 2);
        assert!(pane.dim);
        assert_eq!(pane.lines, vec!["first line", "second line"]);

        // Binary content is a placeholder, not raw bytes.
        let bin = dir.join("blob.bin");
        std::fs::write(&bin, [0u8, 1, 2, 3]).unwrap();
        let pane = render(&bin, false, 40, 4);
        assert!(
            pane.lines[0].starts_with("(binary file"),
            "{:?}",
            pane.lines
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
