//! lusty: native file/buffer picker for Neovim.
//!
//! Three front-ends share the listing stack: `--list` (benchmark/plumbing,
//! depth/skip/mount semantics, ranking, LS_COLORS coloring), `serve` (the
//! headless line-protocol backend for the nvim float) and the standalone
//! raw-mode TUI (grid/long views, preview pane, RU/EN layout).

mod contrast;
mod serve;
mod tui;

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::Instant;

use lusty_core::listing::{self, FileKind};
use lusty_core::{cache, colors, rank};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Debug dumps for the nvim parity smoke: the Lua tables in lusty/ru2en.lua
    // and lusty/icons.lua must match these.
    if args.first().map(|s| s.as_str()) == Some("--ru-map") {
        for code in 0x400u32..=0x4ff {
            if let Some(c) = char::from_u32(code) {
                if let Some(en) = tui::ru_to_en(c) {
                    println!("{c}\t{en}");
                }
            }
        }
        return;
    }
    if args.first().map(|s| s.as_str()) == Some("--fuzzy-vectors") {
        // Golden-vector dump for out-of-process ports of lusty-fuzzy (the
        // quickshell tray menu keeps a JS port; scripts/dev/check-fuzzy-parity.mjs
        // replays this output against it). Reads the corpus from --corpus or stdin.
        let mut opts = lusty_fuzzy::RankOpts::for_menus();
        let mut corpus: Option<String> = None;
        let mut i = 1;
        while i < args.len() {
            match args[i].as_str() {
                "--corpus" => {
                    i += 1;
                    corpus = args.get(i).cloned();
                }
                "--anchor" => {
                    i += 1;
                    opts.anchor = match args.get(i).map(|s| s.as_str()) {
                        Some("prefix-on-first") | Some("prefix") => {
                            lusty_fuzzy::Anchor::PrefixOnFirst
                        }
                        _ => lusty_fuzzy::Anchor::None,
                    };
                }
                "--layout" => {
                    i += 1;
                    opts.layout = match args.get(i).map(|s| s.as_str()) {
                        Some("off") => lusty_fuzzy::Layout::Off,
                        _ => lusty_fuzzy::Layout::RuToEnFallback,
                    };
                }
                _ => {}
            }
            i += 1;
        }
        let text = match corpus {
            Some(path) => match std::fs::read_to_string(&path) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("lusty --fuzzy-vectors: {path}: {e}");
                    std::process::exit(1);
                }
            },
            None => {
                use std::io::Read;
                let mut buf = String::new();
                std::io::stdin().read_to_string(&mut buf).ok();
                buf
            }
        };
        let (labels, queries) = lusty_fuzzy::vectors::parse_corpus(&text);
        print!(
            "{}",
            lusty_fuzzy::vectors::vectors_json(&labels, &queries, opts)
        );
        return;
    }
    if args.first().map(|s| s.as_str()) == Some("--icon-map") {
        println!("dir\t{}", tui::ICON_DIR);
        println!("link\t{}", tui::ICON_LINK);
        println!("file\t{}", tui::ICON_FILE);
        for (suffix, glyph) in tui::ICON_EXTS {
            println!("ext\t{suffix}\t{glyph}");
        }
        return;
    }
    if args.first().map(|s| s.as_str()) == Some("--theme-map") {
        // Resolved theme (honours LUSTY_THEME) for the nvim parity smoke.
        for (key, value) in tui::theme_dump() {
            println!("{key}\t{value}");
        }
        return;
    }
    if args.first().map(|s| s.as_str()) == Some("--color-map") {
        // LS_COLORS resolution for the nvim parity smoke: every argument is
        // "<kind>:<name>[:exec]" and the answer is the raw code (empty when no
        // rule matches).
        let palette = colors::load();
        for spec in args.iter().skip(1) {
            let mut parts = spec.splitn(3, ':');
            let kind = match parts.next().unwrap_or("f") {
                "d" => FileKind::Dir,
                "l" => FileKind::Link,
                "s" => FileKind::Socket,
                "p" => FileKind::Pipe,
                "b" => FileKind::Block,
                "c" => FileKind::Char,
                _ => FileKind::File,
            };
            let name = parts.next().unwrap_or("");
            let exec = parts.next() == Some("exec");
            let code = palette.code_for(name, kind, exec).unwrap_or("");
            println!("{spec}\t{code}");
        }
        return;
    }
    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!(
            "usage: lusty [root] [--depth N] [--skip a,b] [--rows N] [--width N]
                [--view list|grid|long] [--sort name|ext|size|time] [--reverse]
                [--dirs-first] [--columns perm,user,size,time] [--icons]
                [--query Q]
       lusty serve [root] [--depth N] [--skip a,b] [--dots] [--follow-mounts]
       lusty --list [root] [--depth N] [--skip a,b] [--query Q] [--color]
                [--sort ...] [--reverse] [--dirs-first] [--show-dots]

  root      start directory (default: current)
  --depth N listing depth (default 2)
  --skip a,b  directories skipped (default pic,tmp)
  --rows N  popup total height incl borders (default 8)
  --width N popup total width incl borders (default: full terminal width)
  --long    same as --view long (eza -l style rows)
  --view M  list (one column), grid (default, adaptive columns), or long;
            also LUSTY_VIEW=list|grid|long, C-l cycles them in the picker
  --sort S  start order: name (default), ext, size (desc), time (desc)
  --reverse reverse each depth group (eza --reverse)
  --dirs-first  group directories first per depth
  --columns C   long-view fields: perm,user,size,time (default all)
  --icons   nerd-font icons (also LUSTY_ICONS=1)
  --query Q start with Q typed in the prompt

  serve: line protocol on stdin/stdout for the nvim client (see README)

  --ru-map     print the RU->EN key table (parity smoke)
  --icon-map   print the icon table (parity smoke)
  --theme-map  print the resolved theme (parity smoke)
  --color-map  resolve LS_COLORS probes (parity smoke)

Size may also come from LUSTY_ROWS / LUSTY_WIDTH env vars;
command-line flags win.
"
        );
        return;
    }
    if args.first().map(|s| s.as_str()) == Some("--list") {
        run_list(&args);
        return;
    }
    if args.first().map(|s| s.as_str()) == Some("serve") {
        let mut root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let mut depth = 2usize;
        let mut skip = "pic,tmp".to_string();
        let mut show_dots = false;
        let mut follow_mounts = false;
        let mut i = 1;
        while i < args.len() {
            match args[i].as_str() {
                "--depth" => {
                    i += 1;
                    depth = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(2);
                }
                "--skip" => {
                    i += 1;
                    skip = args.get(i).cloned().unwrap_or_default();
                }
                "--dots" | "--show-dots" => show_dots = true,
                "--follow-mounts" => follow_mounts = true,
                other if !other.starts_with("--") => {
                    root = PathBuf::from(other);
                }
                _ => {}
            }
            i += 1;
        }
        let _ = serve::serve(
            root,
            depth,
            skip.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            show_dots,
            follow_mounts,
        );
        return;
    }
    // Interactive picker: lusty [root] [--depth N] [--skip a,b]
    // [--rows N] [--width N]
    let mut root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut depth = 2usize;
    let mut skip = "pic,tmp".to_string();
    let mut rows: Option<usize> = None;
    let mut width: Option<usize> = None;
    let mut view: Option<tui::ViewMode> = None;
    let mut reverse = false;
    let mut dirs_first = false;
    let mut sort_mode = 0u8; // 0 name, 1 ext, 2 size, 3 time
    let mut columns: Option<String> = None;
    let mut icons = false;
    let mut query = String::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--long" => view = Some(tui::ViewMode::Long),
            "--reverse" => reverse = true,
            "--dirs-first" => dirs_first = true,
            "--icons" => icons = true,
            "--query" => {
                i += 1;
                query = args.get(i).cloned().unwrap_or_default();
            }
            "--sort" => {
                i += 1;
                sort_mode = match args.get(i).map(|s| s.as_str()) {
                    Some("ext") => 1,
                    Some("size") => 2,
                    Some("time") => 3,
                    _ => 0,
                };
            }
            "--view" => {
                i += 1;
                match args.get(i).and_then(|s| tui::ViewMode::parse(s)) {
                    Some(v) => view = Some(v),
                    None => eprintln!("lusty: --view takes list, grid or long"),
                }
            }
            "--depth" => {
                i += 1;
                depth = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(2);
            }
            "--skip" => {
                i += 1;
                skip = args.get(i).cloned().unwrap_or_default();
            }
            "--rows" => {
                i += 1;
                rows = args.get(i).and_then(|s| s.parse().ok());
            }
            "--width" => {
                i += 1;
                width = args.get(i).and_then(|s| s.parse().ok());
            }
            "--columns" => {
                i += 1;
                columns = args.get(i).cloned();
            }
            other if !other.starts_with("--") => {
                root = PathBuf::from(other);
            }
            _ => {}
        }
        i += 1;
    }
    let opts = listing::Options {
        depth,
        skip_dirs: skip
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        follow_mounts: false,
        show_dots: false,
    };
    // CLI flags win over the environment; grid is the default (the adaptive
    // multi-column layout the picker always had).
    if view.is_none() {
        view = std::env::var("LUSTY_VIEW")
            .ok()
            .and_then(|v| tui::ViewMode::parse(&v));
    }
    let mut app = tui::App::new(root, opts);
    app.set_ui(rows, width);
    if let Some(v) = view {
        app.set_view(v);
    }
    app.set_sort(reverse, dirs_first);
    app.set_sort_mode(sort_mode);
    if icons {
        app.set_icons(true);
    }
    if !query.is_empty() {
        app.set_query(&query);
    }
    let cols_spec = columns.unwrap_or_else(|| std::env::var("LUSTY_COLUMNS").unwrap_or_default());
    if !cols_spec.is_empty() {
        app.set_columns(&cols_spec);
    }
    match app.run() {
        Ok(code) => std::process::exit(code),
        Err(err) => {
            eprintln!("lusty: {err}");
            std::process::exit(2);
        }
    }
}

fn run_list(args: &[String]) {
    let mut root: Option<PathBuf> = None;
    let mut depth = 2usize;
    let mut show_dots = false;
    let mut color = false;
    let mut skip = "pic,tmp".to_string();
    let mut query = String::new();
    let mut reverse = false;
    let mut dirs_first = false;
    let mut sort_mode = 0u8; // 0 name, 1 ext, 2 size, 3 time

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--depth" => {
                i += 1;
                depth = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(2);
            }
            "--show-dots" => show_dots = true,
            "--color" => color = true,
            "--query" => {
                i += 1;
                query = args.get(i).cloned().unwrap_or_default();
            }
            "--skip" => {
                i += 1;
                skip = args.get(i).cloned().unwrap_or_default();
            }
            "--reverse" => reverse = true,
            "--dirs-first" => dirs_first = true,
            "--sort" => {
                i += 1;
                sort_mode = match args.get(i).map(|s| s.as_str()) {
                    Some("ext") => 1,
                    Some("size") => 2,
                    Some("time") => 3,
                    _ => 0,
                };
            }
            other if !other.starts_with("--") && root.is_none() => {
                root = Some(PathBuf::from(other));
            }
            _ => {}
        }
        i += 1;
    }

    let root = root.unwrap_or_else(|| PathBuf::from("."));
    let opts = listing::Options {
        depth,
        skip_dirs: skip
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        follow_mounts: false,
        show_dots,
    };

    let t0 = Instant::now();
    let mut entries = cache::cached_list(&root, &opts);
    match sort_mode {
        1 => listing::sort_by_ext(&mut entries),
        2 => listing::sort_by_meta(&root, &mut entries, false),
        3 => listing::sort_by_meta(&root, &mut entries, true),
        _ => listing::reorder(&mut entries, dirs_first, reverse),
    }
    let dt_list = t0.elapsed();
    let total = entries.len();

    let idxs: Vec<usize> = if query.is_empty() {
        eprintln!("{} entries in {:?}", total, dt_list);
        (0..total).collect()
    } else {
        let t1 = Instant::now();
        let idxs = rank::rank_indices(&entries, &query);
        eprintln!(
            "{} of {} entries in {:?} (list) + {:?} (rank)",
            idxs.len(),
            total,
            dt_list,
            t1.elapsed()
        );
        idxs
    };

    let palette = if color { Some(colors::load()) } else { None };
    let esc = char::from_u32(0x1b).unwrap();

    let mut out = String::new();
    for &i in &idxs {
        let e = &entries[i];
        let code = palette.as_ref().and_then(|p| {
            let exec = e.kind == FileKind::File && is_exec(&e.path(&root));
            p.code_for(e.basename(), e.kind, exec)
        });
        push_label(&mut out, e, code, esc);
    }
    print!("{out}");
}

fn is_exec(path: &std::path::Path) -> bool {
    std::fs::metadata(path)
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

fn push_label(out: &mut String, e: &listing::Entry, code: Option<&str>, esc: char) {
    if let Some(code) = code {
        out.push(esc);
        out.push('[');
        out.push_str(code);
        out.push('m');
    }
    out.push_str(&e.label);
    if e.kind == FileKind::Dir {
        out.push('/');
    }
    if code.is_some() {
        out.push(esc);
        out.push_str("[0m");
    }
    out.push('\n');
}
