//! Path maths shared by the pickers: lexical absolute/relative paths, the
//! `~` shorthand, common ancestors and shell quoting.
//!
//! Ported from `fzf-navigator` (see CREDITS.md): `rel_path`, `tildify` and
//! `common_parent` mirror the shell helpers behind its `insert_selection`
//! action, and keep the same invariants:
//!
//! - an inserted path is relative to the directory the picker was **launched**
//!   from, never to the directory currently being browsed, so `cd`-ing around
//!   inside the picker cannot break the command line it is pasted into;
//! - a bare executable name gets a `./` prefix, because `tool` in `$PWD` is
//!   not what the shell would run — `PATH` is;
//! - `..` segments are folded lexically: the picker never resolves symlinks
//!   behind the user's back, so a path it prints is the path it showed.

use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

/// Fold `.` and `..` out of `path` without touching the filesystem.
/// Leading `..` segments of a relative path are preserved (there is nothing
/// above `.` to cancel them against).
fn normalize(path: &Path) -> PathBuf {
    let mut parts: Vec<OsString> = Vec::new();
    let mut rooted = false;
    for comp in path.components() {
        match comp {
            Component::Prefix(p) => parts.push(p.as_os_str().to_owned()),
            Component::RootDir => {
                rooted = true;
                parts.clear();
            }
            Component::CurDir => {}
            Component::ParentDir => match parts.last() {
                Some(last) if last != ".." => {
                    parts.pop();
                }
                _ if rooted => {}
                _ => parts.push(OsString::from("..")),
            },
            Component::Normal(s) => parts.push(s.to_owned()),
        }
    }
    let mut out = PathBuf::new();
    if rooted {
        out.push("/");
    }
    for p in parts {
        out.push(p);
    }
    out
}

/// Lexically absolute form of `path`: `base` is prepended when the path is
/// relative and every `.`/`..` is folded away. Symlinks are left alone, which
/// is what the line protocol promises: the path a row shows is the path the
/// client opens.
pub fn abs_path(path: &Path, base: &Path) -> PathBuf {
    if path.is_absolute() {
        normalize(path)
    } else {
        normalize(&base.join(path))
    }
}

/// `target` expressed relative to `base`, both taken as directories-in-name
/// only (see [`abs_path`]). The result is `.` for a path equal to the base and
/// starts with `..` when the target sits above it — the same answer
/// `File::Spec->abs2rel` gives in the shell implementation.
pub fn rel_path(target: &Path, base: &Path) -> PathBuf {
    let target = abs_path(target, base);
    let base = abs_path(base, base);
    let tc: Vec<Component> = target.components().collect();
    let bc: Vec<Component> = base.components().collect();
    let common = tc.iter().zip(bc.iter()).take_while(|(a, b)| a == b).count();
    let mut out = PathBuf::new();
    for _ in common..bc.len() {
        out.push("..");
    }
    for c in &tc[common..] {
        out.push(c.as_os_str());
    }
    if out.as_os_str().is_empty() {
        out.push(".");
    }
    out
}

/// `~/x` shorthand for a path under `home`; a sibling like `/home/negwork` is
/// left alone, because only a component boundary may be replaced.
pub fn tildify(path: &str, home: &str) -> String {
    let home = home.trim_end_matches('/');
    if home.is_empty() {
        return path.to_string();
    }
    if path == home {
        return "~".to_string();
    }
    match path.strip_prefix(home) {
        Some(rest) if rest.starts_with('/') => format!("~{rest}"),
        _ => path.to_string(),
    }
}

/// Deepest directory that contains both paths (the shell helper's
/// `common_parent`), `/` when they share nothing.
pub fn common_parent(a: &Path, b: &Path) -> PathBuf {
    let ac: Vec<Component> = a.components().collect();
    let bc: Vec<Component> = b.components().collect();
    let common = ac.iter().zip(bc.iter()).take_while(|(x, y)| x == y).count();
    let mut out = PathBuf::new();
    for c in &ac[..common] {
        out.push(c.as_os_str());
    }
    if out.as_os_str().is_empty() {
        out.push("/");
    }
    out
}

/// How the current directory relates to the one the picker started in; the
/// prompt paints the two cases differently, exactly like the shell version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptRel {
    /// Current directory is the starting one.
    Same,
    /// Current directory is below the starting one: anchor + subpath.
    Child,
    /// Current directory is an ancestor of the starting one: `*` marks the
    /// starting point below it.
    Ancestor,
    /// The two only share a parent: `*>` marks the fork.
    Fork,
}

/// The prompt's path pieces: an anchor to paint as the "solid" part and the
/// trailing subpath painted as the moving part.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptPath {
    pub anchor: String,
    pub subpath: String,
    pub rel: PromptRel,
}

/// Split `current` against `start` for the prompt (port of the shell
/// `__fzf_navigator_prompt` layout). Both paths are compared after `~`
/// folding so `$HOME` and `/home/u` produce the same prompt.
pub fn prompt_path(start: &str, current: &str) -> PromptPath {
    if start == current {
        return PromptPath {
            anchor: start.to_string(),
            subpath: String::new(),
            rel: PromptRel::Same,
        };
    }
    let common = common_parent(Path::new(start), Path::new(current));
    let common_s = common.to_string_lossy().to_string();
    if common_s == start {
        let sub = current
            .strip_prefix(start)
            .unwrap_or(current)
            .trim_start_matches('/');
        return PromptPath {
            anchor: start.to_string(),
            subpath: sub.to_string(),
            rel: PromptRel::Child,
        };
    }
    if common_s == current {
        let sub = start
            .strip_prefix(current)
            .unwrap_or(start)
            .trim_start_matches('/');
        return PromptPath {
            anchor: current.to_string(),
            subpath: sub.to_string(),
            rel: PromptRel::Ancestor,
        };
    }
    let sub = current
        .strip_prefix(&common_s)
        .unwrap_or(current)
        .trim_start_matches('/');
    PromptPath {
        anchor: common_s,
        subpath: sub.to_string(),
        rel: PromptRel::Fork,
    }
}

/// Quote one word so a POSIX shell reads it back verbatim (the `printf %q`
/// contract the shell version relies on): only `[A-Za-z0-9_@%+=:,./-]` stay
/// bare, everything else is wrapped in single quotes with `'` escaped.
pub fn shell_quote(word: &str) -> String {
    if word.is_empty() {
        return "''".to_string();
    }
    let bare = word.chars().all(|c| {
        c.is_ascii_alphanumeric()
            || matches!(c, '_' | '@' | '%' | '+' | '=' | ':' | ',' | '.' | '/' | '-')
    });
    if bare {
        return word.to_string();
    }
    let mut out = String::with_capacity(word.len() + 2);
    out.push('\'');
    for ch in word.chars() {
        if ch == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(ch);
        }
    }
    out.push('\'');
    out
}

/// Shell-quoted, space-joined list — what `copy_selection` puts on the
/// clipboard and what the command line ends up with.
pub fn quote_all<I, S>(words: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    words
        .into_iter()
        .map(|w| shell_quote(w.as_ref()))
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// One path as it should be typed after `cd`-ing to `launch` (the ported
/// `insert_selection` rule): relative to `launch`, with `./` in front of a
/// bare executable name so the shell cannot pick up a `PATH` twin instead.
pub fn insert_path(target: &Path, launch: &Path) -> String {
    let rel = rel_path(target, launch);
    let text = rel.to_string_lossy().to_string();
    if !text.contains('/') && is_executable_file(target) {
        format!("./{text}")
    } else {
        text
    }
}

/// [`insert_path`] for a whole selection, in mark order.
pub fn insert_paths(targets: &[PathBuf], launch: &Path) -> Vec<String> {
    targets.iter().map(|t| insert_path(t, launch)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_paths_walk_up_and_down() {
        let base = Path::new("/a/b");
        assert_eq!(rel_path(Path::new("/a/b/c"), base), PathBuf::from("c"));
        assert_eq!(rel_path(Path::new("/a/b/c/d"), base), PathBuf::from("c/d"));
        assert_eq!(rel_path(Path::new("/a"), base), PathBuf::from(".."));
        assert_eq!(rel_path(Path::new("/a/x/y"), base), PathBuf::from("../x/y"));
        assert_eq!(rel_path(Path::new("/a/b"), base), PathBuf::from("."));
        assert_eq!(
            rel_path(Path::new("/x"), Path::new("/")),
            PathBuf::from("x")
        );
    }

    #[test]
    fn relative_inputs_are_taken_against_the_base() {
        let base = Path::new("/repo/src");
        assert_eq!(
            rel_path(Path::new("lib/a.rs"), base),
            PathBuf::from("lib/a.rs")
        );
        assert_eq!(
            abs_path(Path::new("./a/../b"), base),
            PathBuf::from("/repo/src/b")
        );
        // Two levels up from /repo/src is /, so the `x` lands at the root.
        assert_eq!(abs_path(Path::new("../../x"), base), PathBuf::from("/x"));
    }

    #[test]
    fn dotdot_cannot_climb_above_the_root() {
        assert_eq!(
            abs_path(Path::new("/../.."), Path::new("/")),
            PathBuf::from("/")
        );
        assert_eq!(
            abs_path(Path::new("../.."), Path::new("/a")),
            PathBuf::from("/")
        );
    }

    #[test]
    fn tildify_only_folds_a_component_boundary() {
        assert_eq!(tildify("/home/neg", "/home/neg"), "~");
        assert_eq!(tildify("/home/neg/src", "/home/neg"), "~/src");
        assert_eq!(tildify("/home/negwork", "/home/neg"), "/home/negwork");
        assert_eq!(tildify("/etc", ""), "/etc");
    }

    #[test]
    fn common_parent_finds_the_deepest_shared_directory() {
        assert_eq!(
            common_parent(Path::new("/a/b/c"), Path::new("/a/d")),
            PathBuf::from("/a")
        );
        assert_eq!(
            common_parent(Path::new("/a/b"), Path::new("/a/b/c")),
            PathBuf::from("/a/b")
        );
        assert_eq!(
            common_parent(Path::new("/x"), Path::new("/y")),
            PathBuf::from("/")
        );
    }

    #[test]
    fn prompt_path_names_the_three_relations() {
        let child = prompt_path("~/src", "~/src/lusty");
        assert_eq!(child.rel, PromptRel::Child);
        assert_eq!(child.anchor, "~/src");
        assert_eq!(child.subpath, "lusty");

        let up = prompt_path("/a/b/c", "/a/b");
        assert_eq!(up.rel, PromptRel::Ancestor);
        assert_eq!(up.anchor, "/a/b");
        assert_eq!(up.subpath, "c");

        let fork = prompt_path("/a/x", "/b/y");
        assert_eq!(fork.rel, PromptRel::Fork);
        assert_eq!(fork.anchor, "/");
        assert_eq!(fork.subpath, "b/y");

        assert_eq!(prompt_path("/a", "/a").rel, PromptRel::Same);
    }

    #[test]
    fn shell_quoting_survives_spaces_and_quotes() {
        assert_eq!(shell_quote("plain"), "plain");
        assert_eq!(shell_quote("a b"), "'a b'");
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
        assert_eq!(shell_quote(""), "''");
        assert_eq!(shell_quote("a$b`c"), "'a$b`c'");
        assert_eq!(quote_all(["a b", "c"]), "'a b' c");
    }

    #[test]
    fn a_bare_executable_gets_a_dot_slash_prefix() {
        let dir = std::env::temp_dir().join(format!("lusty-paths-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        let tool = dir.join("tool");
        std::fs::write(&tool, b"#!/bin/sh\n").unwrap();
        let notes = dir.join("notes.txt");
        std::fs::write(&notes, b"x").unwrap();
        let sub_tool = dir.join("sub/tool");
        std::fs::write(&sub_tool, b"#!/bin/sh\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        for p in [&tool, &sub_tool] {
            std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        assert_eq!(insert_path(&tool, &dir), "./tool");
        assert_eq!(insert_path(&notes, &dir), "notes.txt");
        assert_eq!(insert_path(&sub_tool, &dir), "sub/tool");
        assert_eq!(insert_path(&dir.join("sub"), &dir), "sub");
        assert_eq!(insert_path(&dir.join("vendor"), &dir), "vendor");
        assert_eq!(
            insert_path(&notes, &dir.join("sub")),
            "../notes.txt",
            "relative to the launch dir, not to the browsed one"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
