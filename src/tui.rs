//! Interactive file picker: raw-mode terminal UI with dual RU/EN layout,
//! dircolors coloring and the Lusty key bindings.
//!
//! Selection prints one line to stdout: ACTION<TAB>ABSOLUTE_PATH (action is
//! edit/tabedit/split/vsplit), then exits 0. Cancel exits 1 with no output.
//! Directory selection and C-w re-root the picker in place.

use std::io::{self, Write};
use std::path::PathBuf;

use crossterm::cursor;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{self};

use lusty_core::cache;
use lusty_core::colors::{self, Colors};
use lusty_core::listing::{Entry, FileKind, Options};
// The RU→EN table lives in the shared crate now (mirrored by the Lua front-end
// through `lusty --ru-map`); re-exported so call sites keep their names.
use lusty_core::rank;
pub use lusty_fuzzy::layout::{normalize_query_char, ru_to_en};

/// Kitty graphics protocol support: `LUSTY_KITTY` overrides (`0` disables),
/// otherwise auto-detected from the environment (kitty sets `KITTY_WINDOW_ID`
/// and a `*kitty*` TERM). Without it the pane keeps the ANSI-art fallback.
fn kitty_enabled() -> bool {
    match std::env::var("LUSTY_KITTY") {
        Ok(v) => v == "1" || v.eq_ignore_ascii_case("true"),
        Err(_) => {
            std::env::var_os("KITTY_WINDOW_ID").is_some()
                || std::env::var("TERM")
                    .map(|t| t.contains("kitty"))
                    .unwrap_or(false)
        }
    }
}

/// Where the standalone picker remembers the last typed query. `LUSTY_HISTORY`
/// overrides the path; `LUSTY_HISTORY=0` (or `off`) disables the feature.
fn history_file() -> Option<PathBuf> {
    if let Ok(v) = std::env::var("LUSTY_HISTORY") {
        let v = v.trim();
        if v == "0" || v.eq_ignore_ascii_case("off") {
            return None;
        }
        if !v.is_empty() {
            return Some(PathBuf::from(v));
        }
    }
    let base = std::env::var("XDG_STATE_HOME")
        .map(PathBuf::from)
        .ok()
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| PathBuf::from(h).join(".local/state"))
        })?;
    Some(base.join("lusty").join("history"))
}

/// Last query from the history file (first line), or "" when absent.
fn load_history() -> String {
    history_file()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| s.lines().next().map(str::to_string))
        .unwrap_or_default()
}

/// Persist the current query so the next run starts filtered the same way
/// (the nvim pickers restore their last input in the same fashion).
fn store_history(query: &str) {
    let Some(path) = history_file() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, format!("{query}\n"));
}

/// Entry layout of the standalone popup: a plain single column, an adaptive
/// multi-column grid, or the eza -l style rows with mode/size/date.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ViewMode {
    List,
    Grid,
    Long,
}

impl ViewMode {
    /// Parse the CLI/env spelling (`list`, `grid`, `long`).
    pub fn parse(s: &str) -> Option<ViewMode> {
        match s.trim() {
            "list" => Some(ViewMode::List),
            "grid" => Some(ViewMode::Grid),
            "long" => Some(ViewMode::Long),
            _ => None,
        }
    }

    /// Next mode for the C-l cycle: list -> grid -> long -> list.
    pub fn next(self) -> ViewMode {
        match self {
            ViewMode::List => ViewMode::Grid,
            ViewMode::Grid => ViewMode::Long,
            ViewMode::Long => ViewMode::List,
        }
    }
}

pub struct App {
    root: PathBuf,
    opts: Options,
    hidden: Option<Vec<Entry>>,
    dots: Option<Vec<Entry>>,
    query: String,
    ranked: Vec<usize>,
    needs_rank: bool,
    selected: usize,
    offset: usize,
    size: (usize, usize),
    cursor_row: Option<usize>,
    box_w: usize,            // popup inner width (content columns between the borders)
    box_h: usize,            // popup outer height including the two border rows
    pop_top: usize,          // 0-based screen row of the popup top border
    ui_rows: Option<usize>,  // user popup height (outer rows incl borders)
    ui_width: Option<usize>, // user popup width (outer columns incl borders)
    view: ViewMode,          // list (one column), grid (adaptive) or long (meta rows)
    icons: bool,             // nerd-font icons per entry + dir icon before the prompt
    reverse: bool,           // eza --reverse per depth
    dirs_first: bool,        // eza --group-dirs-first
    sort_mode: u8,           // 0 name, 1 ext, 2 size, 3 time
    cols_mask: u8,           // long view fields: 1 perm, 2 user, 4 size, 8 time
    maxw: usize,             // widest label (chars) in the current listing
    total: usize,            // entries in the listing the current query filters
    palette: Colors,
    preview_on: bool,         // right preview pane (C-Space / Shift+P toggles)
    preview_w: usize,         // pane width in columns
    pane_kind: u8,            // 0 off, 1 dim text (git/man/info), 2 chafa art
    pane: Vec<String>,        // pre-clipped pane rows (preview_w wide)
    pane_key: Option<String>, // cache key: path + size + geometry
    pane_rx: Option<std::sync::mpsc::Receiver<(String, lusty_core::preview::Pane, Option<String>)>>, // in-flight render
    pane_kitty: Option<String>, // kitty graphics sequence for an image pane
    kitty: bool,                // terminal speaks the kitty graphics protocol
    kitty_drawn: bool,          // an image is currently placed
    kitty_key: Option<String>,  // pane key the placed image belongs to
}

impl App {
    pub fn new(root: PathBuf, opts: Options) -> App {
        let palette = colors::load();
        App {
            root,
            opts,
            hidden: None,
            dots: None,
            query: load_history(),
            ranked: Vec::new(),
            needs_rank: true,
            selected: 0,
            offset: 0,
            size: (80, 24),
            cursor_row: None,
            box_w: 78,
            box_h: OUTER_ROWS,
            pop_top: 0,
            ui_rows: None,
            ui_width: None,
            view: ViewMode::Grid,
            icons: std::env::var("LUSTY_ICONS")
                .map(|v| v == "1")
                .unwrap_or(false),
            reverse: false,
            dirs_first: false,
            sort_mode: 0,
            cols_mask: 15,
            maxw: 0,
            total: 0,
            palette,
            preview_on: false,
            preview_w: 40,
            pane_kind: 0,
            pane: Vec::new(),
            pane_key: None,
            pane_rx: None,
            pane_kitty: None,
            kitty: kitty_enabled(),
            kitty_drawn: false,
            kitty_key: None,
        }
    }

    /// Override the popup size. CLI flags win over LUSTY_ROWS/LUSTY_WIDTH
    /// env vars; None keeps the default (8 outer rows, full terminal
    /// columns, i.e. 5 entry rows, the prompt and 100 content columns).
    /// Apply eza-style order tweaks to in-memory listings.
    pub fn set_sort(&mut self, reverse: bool, dirs_first: bool) {
        self.reverse = reverse;
        self.dirs_first = dirs_first;
    }

    pub fn set_sort_mode(&mut self, mode: u8) {
        self.sort_mode = mode;
    }

    /// Set the long-view columns from a comma list (perm,user,size,time).
    pub fn set_columns(&mut self, spec: &str) {
        let mut mask = 0u8;
        for tok in spec.split(',').map(|t| t.trim()).filter(|t| !t.is_empty()) {
            match tok {
                "perm" => mask |= 1,
                "user" => mask |= 2,
                "size" => mask |= 4,
                "time" => mask |= 8,
                _ => {}
            }
        }
        if mask != 0 {
            self.cols_mask = mask;
        }
    }

    /// Set the entry layout (CLI `--view`/`--long`, `LUSTY_VIEW`).
    pub fn set_view(&mut self, view: ViewMode) {
        self.view = view;
    }

    /// Enable nerd-font icons (CLI flag; `LUSTY_ICONS=1` also enables them).
    pub fn set_icons(&mut self, on: bool) {
        self.icons = on;
    }

    /// Start with a query typed in the prompt (CLI `--query`).
    pub fn set_query(&mut self, query: &str) {
        self.query = query.to_string();
        self.needs_rank = true;
        self.selected = 0;
        self.offset = 0;
    }

    pub fn set_ui(&mut self, rows: Option<usize>, width: Option<usize>) {
        let env_usize = |name: &str| {
            std::env::var(name)
                .ok()
                .and_then(|v| v.trim().parse::<usize>().ok())
        };
        self.ui_rows = rows.or_else(|| env_usize("LUSTY_ROWS"));
        self.ui_width = width.or_else(|| env_usize("LUSTY_WIDTH"));
    }

    fn show_dots(&self) -> bool {
        self.query.starts_with('.')
    }

    /// The listing backing the current view (hidden or dot-shown), listed on
    /// demand and cached per root.
    fn listing(&mut self) -> &Vec<Entry> {
        let dots = self.show_dots();
        let is_empty = if dots {
            self.dots.is_none()
        } else {
            self.hidden.is_none()
        };
        if is_empty {
            let opts = Options {
                depth: self.opts.depth,
                skip_dirs: self.opts.skip_dirs.clone(),
                follow_mounts: self.opts.follow_mounts,
                show_dots: dots,
            };
            let mut listed = cache::cached_list(&self.root, &opts);
            match self.sort_mode {
                1 => lusty_core::listing::sort_by_ext(&mut listed),
                2 => lusty_core::listing::sort_by_meta(&self.root, &mut listed, false),
                3 => lusty_core::listing::sort_by_meta(&self.root, &mut listed, true),
                _ => lusty_core::listing::reorder(&mut listed, self.dirs_first, self.reverse),
            }
            // Cache the widest label once per listing: max_cols/col_width ask
            // for it on every redraw, and a full scan per keystroke on a
            // 150k-entry tree is pure waste.
            self.maxw = listed
                .iter()
                .map(|e| str_width(&e.label))
                .max()
                .unwrap_or(0);
            if dots {
                self.dots = Some(listed);
            } else {
                self.hidden = Some(listed);
            }
            self.needs_rank = true;
        }
        if dots {
            self.dots.as_ref().unwrap()
        } else {
            self.hidden.as_ref().unwrap()
        }
    }

    fn ranked_len(&mut self) -> usize {
        self.ensure_ranked();
        self.ranked.len()
    }

    fn ensure_ranked(&mut self) {
        if !self.needs_rank {
            return;
        }
        let query = self.query.clone();
        let entries = self.listing();
        let total = entries.len();
        self.ranked = rank::rank_indices(entries, &query);
        self.total = total;
        self.needs_rank = false;
        if self.selected >= self.ranked.len() && !self.ranked.is_empty() {
            self.selected = self.ranked.len() - 1;
        }
    }

    fn entry_at(&mut self, list_i: usize) -> Entry {
        self.ensure_ranked();
        let i = self.ranked[list_i];
        let entries = self.listing();
        entries[i].clone()
    }

    fn re_root(&mut self, dir: PathBuf) {
        self.root = dir;
        self.hidden = None;
        self.dots = None;
        self.query.clear();
        self.ranked = Vec::new();
        self.needs_rank = true;
        self.selected = 0;
        self.offset = 0;
    }

    fn move_sel(&mut self, delta: isize) {
        let n = self.ranked_len();
        if n == 0 {
            self.selected = 0;
            return;
        }
        self.selected = ((self.selected as isize + delta).rem_euclid(n as isize)) as usize;
    }

    /// Move `delta` whole columns in the column-major grid (see `grid_index`):
    /// a column holds `row_count` entries, right wraps to the first column and
    /// left to the last one. A target column shorter than the current row (the
    /// listing does not fill the last column) clamps to the last entry.
    fn column_nav(&mut self, delta: isize, row_count: usize) {
        self.selected = column_target(self.selected, delta, self.ranked_len(), row_count);
    }

    /// '/' descends into a directory when the typed prefix uniquely names one
    /// (exact name or unique prefix among the immediate children), shell
    /// style; a lone '/' moves to the filesystem root, Lusty style. When the
    /// prefix is not unique, '/' is typed as an ordinary character.
    fn slash_enter(&mut self) {
        let q = self.query.clone();
        if q.is_empty() {
            let root = std::path::PathBuf::from("/");
            if self.root != root {
                self.re_root(root);
            }
            return;
        }
        let ql = q.to_lowercase();
        // Take a local copy of the root instead of cloning the whole listing:
        // on a 150k-entry tree that clone was a visible stall per '/', and the
        // path from `Entry::path` keeps non-UTF8 directory names intact.
        let root = self.root.clone();
        let mut cand: Option<PathBuf> = None;
        let mut dup = false;
        for e in self.listing().iter() {
            if e.kind != FileKind::Dir || e.depth != 1 {
                continue;
            }
            let nl = e.basename().to_lowercase();
            if nl == ql || nl.starts_with(&ql) {
                if cand.is_some() {
                    dup = true;
                } else {
                    cand = Some(e.path(&root));
                }
            }
        }
        if let (Some(path), false) = (cand, dup) {
            self.re_root(path);
            return;
        }
        // not a unique directory: let '/' be typed as an ordinary character
        self.query.push('/');
        self.needs_rank = true;
        self.selected = 0;
        self.offset = 0;
    }

    fn open(&mut self, action: &str) -> io::Result<()> {
        if self.ranked.is_empty() {
            return Ok(());
        }
        let entry = self.entry_at(self.selected);
        if entry.kind == FileKind::Dir {
            self.re_root(entry.path(&self.root));
            return Ok(());
        }
        // No alternate screen here: the picker usually runs inside a nvim
        // terminal buffer (possibly itself inside a web xterm), whose
        // alt-screen emulation is unreliable. The shim deletes the buffer on
        // exit anyway, so clear the frame and print the selection.
        // clear the panel first, then park the cursor at its first row (right
        // under the command line) so the selection prints like normal shell
        // output, not below a gap of erased panel rows
        self.clear_panel(&mut io::stdout())?;
        let esc = char::from_u32(0x1b).unwrap();
        write!(io::stdout(), "{esc}[{};1H", self.panel_top() + 1)?;
        terminal::disable_raw_mode()?;
        execute!(io::stdout(), cursor::Show)?;
        let mut so = io::stdout().lock();
        // Raw bytes: the consumer (shell/nvim) opens this path verbatim, so a
        // non-UTF8 name must not go through the lossy `display()`.
        use std::os::unix::ffi::OsStrExt;
        so.write_all(action.as_bytes())?;
        so.write_all(b"\t")?;
        so.write_all(entry.path(&self.root).as_os_str().as_bytes())?;
        so.write_all(b"\n")?;
        so.flush()?;
        store_history(&self.query);
        std::process::exit(0);
    }

    pub fn run(&mut self) -> io::Result<i32> {
        terminal::enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, cursor::Hide)?;
        // Ask for the prompt row and the real grid size in one DSR round trip:
        // the cursor sits on the empty line right below the command line the
        // picker was launched from, and the ioctl size can be stale when
        // running inside a nvim terminal buffer.
        let (cursor_row, probed) = probe_terminal();
        self.cursor_row = cursor_row;
        if let Some((cols, rows)) = probed {
            self.size = (cols, rows);
        } else {
            let (c, r) = terminal::size().unwrap_or((80, 24));
            self.size = ((c as usize).max(40), (r as usize).max(10));
        }
        let h = self.size.1;
        self.compute_box();
        // Place the box directly under the command line; when it would
        // run past the bottom of the screen, scroll the content up first
        // (fzf --height behaviour). probe_terminal parked the cursor on the
        // bottom row, so plain newlines scroll.
        let esc = char::from_u32(0x1b).unwrap();
        let r0 = self
            .cursor_row
            .map(|r| r.min(h.saturating_sub(1)))
            .unwrap_or(h.saturating_sub(self.box_h));
        let scroll_by = (r0 + self.box_h).saturating_sub(h);
        if scroll_by > 0 && probed.is_some() {
            let mut s = String::with_capacity(scroll_by + 1);
            for _ in 0..scroll_by {
                s.push('\n');
            }
            write!(stdout, "{s}")?;
            stdout.flush()?;
            self.pop_top = r0 - scroll_by;
        } else if scroll_by > 0 {
            self.pop_top = h.saturating_sub(self.box_h);
        } else {
            self.pop_top = r0;
        }
        let result = self.loop_events(&mut stdout);
        // Remember the query for the next run (cancel path; `open` stores it
        // before its own exit).
        store_history(&self.query);
        // Clear first, then park the cursor at the top of the popup area
        // so the shell continues directly under the command line.
        self.clear_panel(&mut stdout)?;
        write!(stdout, "{esc}[{};1H", self.panel_top() + 1)?;
        terminal::disable_raw_mode()?;
        execute!(stdout, cursor::Show)?;
        result
    }
}

/// Ask the terminal where the cursor is and how big the grid is. Both DSR
/// queries are written up front and their replies read in one pass, so a
/// terminal that does not answer costs a single timeout instead of two.
/// Returns (0-based cursor row, (cols, rows)).
fn probe_terminal() -> (Option<usize>, Option<(usize, usize)>) {
    use std::time::{Duration, Instant};

    let esc = char::from_u32(0x1b).unwrap();
    let mut out = io::stdout();
    // First reply: the real cursor position (the panel anchor). Moving to
    // 9999;9999 clamps the second reply to the bottom-right corner = size.
    if write!(out, "{esc}[6n{esc}[9999;9999H{esc}[6n").is_err() || out.flush().is_err() {
        return (None, None);
    }

    let deadline = Instant::now() + Duration::from_millis(250);
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    while buf.len() < 48 && Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let ms = remaining.as_millis().min(60) as i32;
        let mut fds = [libc::pollfd {
            fd: 0,
            events: libc::POLLIN,
            revents: 0,
        }];
        // SAFETY: fds is a valid array of pollfds for the stdin fd.
        let rc = unsafe { libc::poll(fds.as_mut_ptr(), 1, ms) };
        if rc <= 0 {
            continue; // timeout or poll error
        }
        // Read raw from fd 0 (not io::stdin, whose BufReader would swallow the
        // rest of a reply and strand it outside the kernel queue).
        // SAFETY: byte is a valid 1-byte buffer for the stdin fd.
        let n = unsafe { libc::read(0, byte.as_mut_ptr().cast(), 1) };
        if n <= 0 {
            break;
        }
        buf.push(byte[0]);
        if String::from_utf8_lossy(&buf).matches('R').count() >= 2 {
            break;
        }
    }

    let s = String::from_utf8_lossy(&buf);
    let replies: Vec<(usize, usize)> = s.split('R').filter_map(parse_dsr).collect();
    let cursor_row = replies.first().map(|(row, _)| row.saturating_sub(1));
    let size = replies.get(1).map(|(row, col)| (*col, *row));
    (cursor_row, size)
}

/// Parse the `<row>;<col>` tail of an `ESC[<row>;<col>R` reply.
fn parse_dsr(part: &str) -> Option<(usize, usize)> {
    let inner = part.rsplit('[').next()?;
    let mut parts = inner.split(';');
    let row: usize = parts.next()?.trim().parse().ok()?;
    let col: usize = parts.next()?.trim().parse().ok()?;
    Some((row, col))
}

/// Column-major grid cell order shared by `draw`, `column_nav` and
/// `clamp_offset`: the grid is `rows` entries tall, entries fill a column
/// top-to-bottom and continue in the next column to the right; `offset` is the
/// index of the cell at the top-left corner.
fn grid_index(offset: usize, rows: usize, r: usize, c: usize) -> usize {
    offset + c * rows + r
}

/// Target index for a left/right move of `delta` whole columns in the grid;
/// wraps around the total column count and clamps when the target column is
/// shorter than the current row (the listing does not fill the last column).
fn column_target(selected: usize, delta: isize, n: usize, rows: usize) -> usize {
    if n == 0 || rows == 0 {
        return 0;
    }
    let total_cols = n.div_ceil(rows);
    let cur_col = selected / rows;
    let cur_row = selected % rows;
    let new_col = (cur_col as isize + delta).rem_euclid(total_cols as isize) as usize;
    (new_col * rows + cur_row).min(n - 1)
}

/// First index of the grid window that keeps `selected` visible. The window
/// shows `cols` whole columns of `rows` entries and starts on a column
/// boundary, so navigating never shifts cells by a single entry; it follows the
/// selection only once the latter leaves the window.
fn grid_offset(selected: usize, offset: usize, n: usize, rows: usize, cols: usize) -> usize {
    if n == 0 || rows == 0 || cols == 0 {
        return 0;
    }
    let total_cols = n.div_ceil(rows);
    let max_offset = total_cols.saturating_sub(cols) * rows;
    let mut off = (offset / rows) * rows;
    if selected < off {
        off = (selected / rows) * rows;
    } else if selected >= off + rows * cols {
        off = (selected / rows + 1 - cols) * rows;
    }
    off.min(max_offset)
}

impl App {
    /// Recompute the popup box size from the current screen size and the
    /// user overrides (--rows/--width or LUSTY_ROWS/LUSTY_WIDTH). Both
    /// dimensions are clamped so the outer box never exceeds the terminal;
    /// defaults are OUTER_ROWS rows and the full terminal width.
    fn compute_box(&mut self) {
        let (w, h) = self.size;
        let outer_h = match self.ui_rows {
            Some(r) => r.clamp(4, 200).min(h),
            None => OUTER_ROWS.min(h),
        };
        self.box_h = outer_h.max(1);
        let outer_w = match self.ui_width {
            Some(uw) => uw.clamp(10, 400).min(w),
            None => w, // default: span the whole terminal width
        };
        self.box_w = outer_w.saturating_sub(2);
        let envw = std::env::var("LUSTY_PREVIEW_WIDTH")
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok());
        let pw = envw.unwrap_or((self.box_w / 3).clamp(24, 80));
        self.preview_w = pw.clamp(20, self.box_w.saturating_sub(21).max(20));
    }

    fn list_rows(&self) -> usize {
        // Content rows inside the borders minus the prompt line at the
        // bottom (box_h includes the two border rows).
        self.box_h.saturating_sub(3).max(1)
    }

    /// Width of the entry-grid area: the whole box, or the left part when
    /// the right preview pane is shown (list + divider + pane = box_w).
    fn content_w(&self) -> usize {
        if self.preview_on {
            self.box_w.saturating_sub(self.preview_w + 1)
        } else {
            self.box_w
        }
    }

    /// 0-based screen row of the popup top border, fixed at startup to
    /// sit directly under the command line the picker was launched from.
    fn panel_top(&self) -> usize {
        self.pop_top
    }

    /// Adaptive columns: as many as the content needs (ceil(total/rows)),
    /// no more than fit the popup width given the widest name (capped at 20).
    fn max_cols(&mut self) -> usize {
        let w = self.content_w();
        let rows = self.list_rows();
        let total = self.ranked.len().max(1);
        let needed = total.div_ceil(rows).max(1);
        let name_w = self.max_name_w().clamp(1, 20);
        let byw = ((w + 2) / (name_w + 4)).max(1);
        needed.min(byw).clamp(1, 8)
    }

    /// Entry columns for the current view: the grid adapts to the popup, the
    /// list and long views are a single column.
    fn view_cols(&mut self) -> usize {
        match self.view {
            ViewMode::Grid => self.max_cols(),
            ViewMode::List | ViewMode::Long => 1,
        }
    }

    fn col_width(&mut self) -> usize {
        let cols = self.max_cols();
        let w = self.content_w();
        // pitch = col_w + 2 separator; ensure cols*col_w + 2*(cols-1) <= w
        let text_w = w.saturating_sub(2 * (cols - 1));
        (text_w / cols).max(6)
    }

    /// Start a preview render for the current selection when the key (path,
    /// size, geometry) changed. `preview::render` may spawn chafa/git/man, so
    /// it runs on a worker thread; `poll_pane` picks the result up from the
    /// event loop, which keeps the UI responsive while a big image or repo
    /// diff is being rendered.
    fn refresh_pane(&mut self) {
        if !self.preview_on || self.ranked.is_empty() || self.selected >= self.ranked.len() {
            self.clear_pane();
            return;
        }
        let i = self.ranked[self.selected];
        let e = self.listing()[i].clone();
        let path = e.path(&self.root);
        // Size plus mtime: an edit that keeps the size (and even a same-second
        // save) must invalidate the cached pane.
        let stamp = std::fs::metadata(&path)
            .map(|m| {
                use std::os::unix::fs::MetadataExt;
                format!("{}.{}.{}", m.size(), m.mtime(), m.mtime_nsec())
            })
            .unwrap_or_else(|_| "?".to_string());
        let rows = self.list_rows();
        let pw = self.preview_w;
        let key = format!(
            "{}|{}|{}|{}|{}",
            path.display(),
            pw,
            rows,
            e.kind == FileKind::Dir,
            stamp
        );
        if self.pane_key.as_deref() == Some(key.as_str()) {
            return; // already rendered, or the same render is in flight
        }
        let is_dir = e.kind == FileKind::Dir;
        let use_kitty = self.kitty;
        let (tx, rx) = std::sync::mpsc::channel();
        let worker_key = key.clone();
        std::thread::spawn(move || {
            let pane = lusty_core::preview::render(&path, is_dir, pw, rows);
            let kitty = if use_kitty && lusty_core::preview::is_image(&path) {
                lusty_core::preview::kitty_image(&path, pw, rows)
            } else {
                None
            };
            let _ = tx.send((worker_key, pane, kitty));
        });
        self.pane_rx = Some(rx);
        self.pane_key = Some(key);
    }

    fn clear_pane(&mut self) {
        self.pane.clear();
        self.pane_kind = 0;
        self.pane_key = None;
        self.pane_rx = None;
        self.pane_kitty = None;
    }

    /// Install a finished preview when it still matches the current key;
    /// returns true when the pane changed and the caller should redraw.
    fn poll_pane(&mut self) -> bool {
        let Some(rx) = self.pane_rx.take() else {
            return false;
        };
        match rx.try_recv() {
            Ok((key, pane, kitty)) => {
                if !self.preview_on || self.pane_key.as_deref() != Some(key.as_str()) {
                    return false; // stale: the selection moved on or preview closed
                }
                self.install_pane(&pane, kitty);
                true
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                self.pane_rx = Some(rx); // still rendering
                false
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => false,
        }
    }

    /// Format and clip a rendered pane into the row buffer. With a kitty
    /// graphics sequence the pane is blanked instead: the image (drawn above
    /// the cell background, below text) shows through.
    fn install_pane(&mut self, pane: &lusty_core::preview::Pane, kitty: Option<String>) {
        let rows = self.list_rows();
        let pw = self.preview_w;
        self.pane_kitty = kitty;
        if self.pane_kitty.is_some() {
            // Force a (re)placement: the key may already equal pane_key from a
            // request that was in flight while the old image was still drawn.
            self.kitty_key = None;
            self.pane = vec![" ".repeat(pw); rows];
            self.pane_kind = 2;
            return;
        }
        let dim_code = format!("{}[38;2;140;150;165m", char::from_u32(0x1b).unwrap());
        let mut out: Vec<String> = Vec::with_capacity(rows);
        for src in pane.lines.iter().take(rows) {
            let mut s = String::new();
            if pane.dim {
                s.push_str(&dim_code);
            }
            s.push_str(src);
            ansi_pad(&mut s, pw);
            out.push(s);
        }
        while out.len() < rows {
            out.push(" ".repeat(pw));
        }
        self.pane = out;
        self.pane_kind = if pane.dim { 1 } else { 2 };
    }

    /// Widest label (in chars) over the full listing of the current root,
    /// cached when the listing is built (see `listing`).
    fn max_name_w(&self) -> usize {
        self.maxw
    }

    /// Keep the selected entry inside the visible column-major window (see
    /// `grid_offset`); the window start stays on a column boundary. The list
    /// and long views render a single column, so their window is `rows` wide.
    fn clamp_offset(&mut self, rows: usize) {
        let cols = self.view_cols();
        self.offset = grid_offset(self.selected, self.offset, self.ranked.len(), rows, cols);
    }

    fn loop_events(&mut self, out: &mut io::Stdout) -> io::Result<i32> {
        loop {
            self.draw(out)?;
            // Wait for a key or a finished preview render; the short poll lets
            // an async pane result land without a keypress.
            loop {
                if self.poll_pane() {
                    break;
                }
                if !event::poll(std::time::Duration::from_millis(50))? {
                    continue;
                }
                let ev = event::read()?;
                match ev {
                    Event::Key(KeyEvent {
                        code, modifiers, ..
                    }) => {
                        let ctrl = modifiers.contains(KeyModifiers::CONTROL);
                        match (code, ctrl) {
                            (KeyCode::Esc, _) => return Ok(1),
                            (KeyCode::Char('c' | 'g'), true) => return Ok(1),
                            (KeyCode::Enter | KeyCode::Tab, _) => self.open("edit")?,
                            (KeyCode::Char('t'), true) => self.open("tabedit")?,
                            (KeyCode::Char('o'), true) => self.open("split")?,
                            (KeyCode::Char('v'), true) => self.open("vsplit")?,
                            (KeyCode::Char('n'), true)
                            | (KeyCode::Char('j'), true)
                            | (KeyCode::Down, _) => {
                                self.move_sel(1);
                            }
                            (KeyCode::Char('p'), true)
                            | (KeyCode::Char('k'), true)
                            | (KeyCode::Up, _) => {
                                self.move_sel(-1);
                            }
                            (KeyCode::Char('f'), true) | (KeyCode::Right, _) => {
                                let rows = self.list_rows();
                                self.column_nav(1, rows);
                            }
                            (KeyCode::Char('b'), true) | (KeyCode::Left, _) => {
                                let rows = self.list_rows();
                                self.column_nav(-1, rows);
                            }
                            (KeyCode::Char('w'), true) => {
                                // First C-w clears the typed query (shell/vim
                                // word-delete feel); only a second C-w with an
                                // empty query moves up a directory.
                                if !self.query.is_empty() {
                                    self.query.clear();
                                    self.needs_rank = true;
                                    self.selected = 0;
                                    self.offset = 0;
                                } else if let Some(parent) = self.root.parent() {
                                    if parent != self.root {
                                        self.re_root(parent.to_path_buf());
                                    }
                                }
                            }
                            // C-l cycles the entry layout list -> grid -> long.
                            (KeyCode::Char('l'), true) => self.view = self.view.next(),
                            // C-y cycles the sort order like the nvim float: drop
                            // the cached listing so the next draw re-sorts it.
                            (KeyCode::Char('y'), true) => {
                                self.sort_mode = (self.sort_mode + 1) % 4;
                                self.hidden = None;
                                self.dots = None;
                                self.needs_rank = true;
                                self.selected = 0;
                                self.offset = 0;
                            }
                            (KeyCode::Char('u'), true) => {
                                if !self.query.is_empty() {
                                    self.query.clear();
                                    self.needs_rank = true;
                                    self.selected = 0;
                                }
                            }
                            // C-d cycles the search depth (1..6) like the nvim
                            // float; the listing cache is keyed by depth, so the
                            // next draw re-lists (or hits that depth's cache).
                            (KeyCode::Char('d'), true) => {
                                self.opts.depth = self.opts.depth % 6 + 1;
                                self.hidden = None;
                                self.dots = None;
                                self.needs_rank = true;
                                self.selected = 0;
                                self.offset = 0;
                            }
                            // readline-ish: C-h = backspace, Home/End = first/last,
                            // PgUp/PgDn = one page of the grid
                            (KeyCode::Char('h'), true) => {
                                if self.query.pop().is_some() {
                                    self.needs_rank = true;
                                    self.selected = 0;
                                }
                            }
                            (KeyCode::Home, _) | (KeyCode::Char('a'), true) => {
                                if self.ranked_len() > 0 {
                                    self.selected = 0;
                                }
                            }
                            (KeyCode::End, _) | (KeyCode::Char('e'), true) => {
                                let n = self.ranked_len();
                                if n > 0 {
                                    self.selected = n - 1;
                                }
                            }
                            (KeyCode::PageUp, _) => {
                                let n = self.ranked_len();
                                let page = self.list_rows();
                                if n > 0 {
                                    self.selected = self.selected.saturating_sub(page);
                                }
                            }
                            (KeyCode::PageDown, _) => {
                                let n = self.ranked_len();
                                let page = self.list_rows();
                                if n > 0 {
                                    self.selected = (self.selected + page).min(n - 1);
                                }
                            }
                            (KeyCode::Backspace, _) => {
                                if self.query.pop().is_some() {
                                    self.needs_rank = true;
                                    self.selected = 0;
                                }
                            }
                            (KeyCode::Char('/'), false) => self.slash_enter(),
                            // Right preview pane: C-Space (NUL) or Shift+P toggles.
                            (KeyCode::Char(' '), true) | (KeyCode::Char('P'), false) => {
                                self.preview_on = !self.preview_on;
                                self.clear_pane();
                            }
                            (KeyCode::Char(c), false) => {
                                if let Some(c) = normalize_query_char(c) {
                                    self.query.push(c);
                                    self.needs_rank = true;
                                    self.selected = 0;
                                }
                            }
                            _ => {}
                        }
                    }
                    Event::Resize(c, r) => {
                        // Keep the box inside the new grid: recompute dimensions
                        // from the fresh size and pull the box up if it no
                        // longer fits below its current top row.
                        self.size = (c as usize, r as usize);
                        self.compute_box();
                        let h = self.size.1;
                        if self.pop_top + self.box_h > h {
                            self.pop_top = h.saturating_sub(self.box_h);
                        }
                    }
                    _ => {}
                }
                self.clamp_offset(self.list_rows());
                break;
            }
        }
    }

    /// Render one frame. Generic over the writer so tests can render into a
    /// buffer and assert the layout without a terminal.
    fn draw<W: Write>(&mut self, out: &mut W) -> io::Result<()> {
        self.ensure_ranked();
        if self.preview_on {
            self.refresh_pane();
        }
        let root_path = self.root.clone();
        let esc = char::from_u32(0x1b).unwrap();
        let w = self.box_w;
        let lw = self.content_w();
        let top = self.pop_top;
        let bh = self.box_h;
        let rows = self.list_rows();
        // The grid adapts to the popup; the list and long views use one
        // full-width column.
        let cols = self.view_cols();
        let col_w = if self.view == ViewMode::Grid {
            self.col_width()
        } else {
            lw
        };
        let bg = "48;2;0;0;0"; // opaque black popup background
        let border = "38;2;58;68;84"; // #3a4454 frame, darker than the meta text
        let revert = format!("{esc}[22;23;24;39;{bg}m"); // default fg on popup bg
        let mut frame = String::with_capacity((w + 64) * (bh + 2));

        // Top border: box-drawing frame around w content columns.
        frame.push_str(&format!("{esc}[{};1H", top + 1));
        frame.push_str(&format!("{esc}[{bg}m{esc}[{border}m"));
        frame.push('\u{250c}'); // \u250c
        for _ in 0..w {
            frame.push('\u{2500}'); // \u2500
        }
        frame.push('\u{2510}'); // \u2510
        frame.push_str(&format!("{esc}[0m{esc}[K"));
        // Content rows: entry grid on top, prompt as the bottom line.
        for r in 0..(bh.saturating_sub(2)) {
            frame.push_str(&format!("{esc}[{};1H", top + 2 + r));
            frame.push_str(&format!("{esc}[{bg}m{esc}[{border}m"));
            frame.push('\u{2502}'); // \u2502
            let mut line = String::new();
            if r < rows {
                for c in 0..cols {
                    // Column-major, the same mapping `grid_index` and the
                    // column navigation use: the grid is `rows` cells tall and
                    // grows to the right.
                    let pos = grid_index(self.offset, rows, r, c);
                    let mut cell = String::new();
                    if pos < self.ranked.len() {
                        let i = self.ranked[pos];
                        let e = self.listing()[i].clone();
                        if self.view == ViewMode::Long {
                            if let Some(m) =
                                lusty_core::listing::meta_line(&e.path(&root_path), self.cols_mask)
                            {
                                cell.push_str(&format!("{esc}[38;2;108;126;150m{m}"));
                                cell.push_str(&revert);
                                cell.push(' ');
                            }
                        }
                        if pos == self.selected {
                            cell.push_str(&format!("{esc}[{}m", sel_style()));
                        } else {
                            let exec = e.kind == FileKind::File && is_exec(&e.path(&root_path));
                            if let Some(code) = self.palette.code_for(e.basename(), e.kind, exec) {
                                cell.push_str(&format!("{esc}[{code}m"));
                            }
                        }
                        if self.icons {
                            cell.push_str(icon_for(&e));
                            cell.push(' ');
                        }
                        // Terminal-safe display copy: a crafted file name with
                        // ESC must not inject control sequences into the frame.
                        let label = sanitize_display(&e.label);
                        if let Some((ms, me)) = query_match(&label, &self.query) {
                            cell.push_str(&label[..ms]);
                            let mt = theme();
                            if let Some([r, g, b]) = mt.match_fg {
                                cell.push_str(&format!("{esc}[38;2;{r};{g};{b}m"));
                            }
                            if mt.match_underline {
                                cell.push_str(&format!("{esc}[4m"));
                            }
                            cell.push_str(&label[ms..me]);
                            if mt.match_underline {
                                cell.push_str(&format!("{esc}[24m"));
                            }
                            if mt.match_fg.is_some() {
                                cell.push_str(&format!("{esc}[39m"));
                            }
                            cell.push_str(&label[me..]);
                        } else {
                            cell.push_str(&label);
                        }
                        if e.kind == FileKind::Dir && !self.icons {
                            cell.push('/');
                        }
                    }
                    // Pad first and reset after: the selected cell keeps its
                    // background across the whole column instead of stopping
                    // at the end of the name.
                    ansi_pad(&mut cell, col_w);
                    cell.push_str(&revert);
                    line.push_str(&cell);
                    if c + 1 < cols {
                        line.push_str("  ");
                    }
                }
                if self.preview_on {
                    // Left: the entry grid clipped to the list area.
                    ansi_pad(&mut line, lw);
                    frame.push_str(&line);
                    line.clear();
                    // Divider between list and preview pane.
                    frame.push_str(&format!("{esc}[{border}m"));
                    frame.push('\u{2502}'); // \u2502
                    frame.push_str(&format!("{esc}[0m"));
                    // Right: one pre-rendered pane row.
                    if let Some(prow) = self.pane.get(r) {
                        frame.push_str(prow);
                    }
                    frame.push_str(&format!("{esc}[{bg}m"));
                } else {
                    ansi_pad(&mut line, w);
                    frame.push_str(&line);
                }
            } else if r == rows {
                line = self.prompt_line();
                ansi_pad(&mut line, w);
                frame.push_str(&line);
            }
            frame.push_str(&format!("{esc}[{border}m"));
            frame.push('\u{2502}'); // \u2502
            frame.push_str(&format!("{esc}[0m{esc}[K"));
        }
        // Bottom border.
        frame.push_str(&format!("{esc}[{};1H", top + bh));
        frame.push_str(&format!("{esc}[{bg}m{esc}[{border}m"));
        frame.push('\u{2514}'); // \u2514
        for _ in 0..w {
            frame.push('\u{2500}'); // \u2500
        }
        frame.push('\u{2518}'); // \u2518
        frame.push_str(&format!("{esc}[0m{esc}[K"));
        // Kitty image placement: emitted once per image (the escape sequence is
        // large, so redraws must not resend it). A stale image is deleted when
        // the selection moves to a non-image or the preview closes.
        let emit =
            self.pane_kitty.is_some() && self.kitty_key.as_deref() != self.pane_key.as_deref();
        if emit {
            if self.kitty_drawn {
                frame.push_str("\x1b_Ga=d\x1b\\");
            }
            frame.push_str(&format!("{esc}[{};{}H", top + 2, lw + 3));
            if let Some(seq) = &self.pane_kitty {
                frame.push_str(seq);
            }
            self.kitty_key = self.pane_key.clone();
            self.kitty_drawn = true;
        } else if self.pane_kitty.is_none() && self.kitty_drawn {
            frame.push_str("\x1b_Ga=d\x1b\\");
            self.kitty_drawn = false;
            self.kitty_key = None;
        }
        write!(out, "{frame}")?;
        out.flush()
    }

    /// Erase the panel lines (used when the picker exits).
    fn clear_panel(&self, out: &mut io::Stdout) -> io::Result<()> {
        let bh = self.box_h;
        let top = self.pop_top;
        let esc = char::from_u32(0x1b).unwrap();
        let mut s = String::new();
        for r in 0..bh {
            s.push(esc);
            s.push_str(&format!("[{};1H", top + r + 1));
            s.push(esc);
            s.push_str("[K");
        }
        if self.kitty_drawn {
            // Do not leave a placed image behind after the picker exits.
            s.push_str("\x1b_Ga=d\x1b\\");
        }
        write!(out, "{s}")?;
        out.flush()
    }

    fn prompt_line(&self) -> String {
        let esc = char::from_u32(0x1b).unwrap();
        let mut out = String::new();
        if self.icons {
            out.push(esc);
            out.push_str("[38;2;108;126;150m");
            out.push_str(ICON_DIR);
            out.push(' ');
        }
        let push_painted = |text: &str, code: &str, out: &mut String| {
            if text.is_empty() {
                return;
            }
            out.push(esc);
            out.push('[');
            out.push_str(code);
            out.push('m');
            out.push_str(text);
        };
        let mut path = self.root.display().to_string();
        if let Ok(home) = std::env::var("HOME") {
            let home = home.trim_end_matches('/');
            if !home.is_empty() && path.starts_with(home) {
                let rest = &path[home.len()..];
                // Only a component boundary counts: with HOME=/home/neg the
                // path /home/negwork must not become ~work.
                if rest.is_empty() {
                    path = "~".to_string();
                } else if rest.starts_with('/') {
                    path = format!("~{rest}");
                }
            }
        }
        if let Some(rest) = path.strip_prefix('~') {
            push_painted("~", "38;2;40;115;115", &mut out);
            path = rest.to_string();
        }
        let path = sanitize_display(&path);
        let mut current = String::new();
        for ch in path.chars() {
            if ch == '/' {
                push_painted(&current, "38;2;149;167;188", &mut out);
                push_painted("/", "38;2;0;95;175", &mut out);
                current.clear();
            } else {
                current.push(ch);
            }
        }
        push_painted(&current, "38;2;149;167;188", &mut out);
        push_painted(" \u{f105} ", "38;2;0;95;175", &mut out);
        push_painted(&self.query, "1;38;2;255;255;255", &mut out);
        // Search depth (C-d cycles it) and the matched/total counter, dimmed
        // like the border.
        push_painted(
            &format!(
                "  d{}  {}/{}",
                self.opts.depth,
                self.ranked.len(),
                self.total
            ),
            "38;2;108;126;150",
            &mut out,
        );
        out.push(esc);
        out.push_str("[22;23;24;39m");
        out
    }
}

/// Outer popup height: two border rows plus five entry rows and the prompt.
const OUTER_ROWS: usize = 8;

/// Selection theme loaded from the Lusty TOML (env `LUSTY_THEME`; by default
/// the nvim copy at `$XDG_CONFIG_HOME/nvim/lua/lusty/theme.toml`, then the
/// standalone one at `$XDG_CONFIG_HOME/lusty/theme.toml`). The file uses the
/// same simple key=value lines the nvim side reads, so both interfaces share
/// one theme; `--theme-map` dumps the resolved values for the parity smoke.
struct Theme {
    sel_bg: Option<[u8; 3]>,
    sel_fg: Option<[u8; 3]>,
    sel_bold: bool,
    sel_underline: bool,
    sel_reverse: bool,
    match_fg: Option<[u8; 3]>,
    match_underline: bool,
}

fn hex3(s: &str) -> Option<[u8; 3]> {
    let s = s.trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    Some([
        u8::from_str_radix(&s[0..2], 16).ok()?,
        u8::from_str_radix(&s[2..4], 16).ok()?,
        u8::from_str_radix(&s[4..6], 16).ok()?,
    ])
}

fn theme_default() -> Theme {
    Theme {
        sel_bg: Some([0x00, 0x5f, 0xaf]),
        sel_fg: Some([0xd1, 0xe5, 0xff]),
        sel_bold: true,
        sel_underline: false,
        sel_reverse: false,
        match_fg: None,
        match_underline: true,
    }
}

/// Theme file resolution shared with the nvim side: `LUSTY_THEME` wins, then
/// the deployed nvim copy, then the standalone one.
fn theme_path() -> String {
    if let Ok(p) = std::env::var("LUSTY_THEME") {
        if !p.trim().is_empty() {
            return p;
        }
    }
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .ok()
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| PathBuf::from(h).join(".config"))
        });
    if let Some(base) = base {
        let nvim = base.join("nvim/lua/lusty/theme.toml");
        if nvim.exists() {
            return nvim.display().to_string();
        }
        return base.join("lusty/theme.toml").display().to_string();
    }
    String::new()
}

fn load_theme() -> Theme {
    let path = theme_path();
    let mut t = theme_default();
    let Ok(content) = std::fs::read_to_string(path) else {
        return t;
    };
    let mut section = String::new();
    for raw in content.lines() {
        let line = raw.trim();
        if let Some(rest) = line.strip_prefix('[') {
            section = rest.trim_end_matches(']').to_string();
            continue;
        }
        let Some((key, val)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let val = val.trim().trim_matches('"');
        match (section.as_str(), key) {
            ("lusty.selection", "bg") => t.sel_bg = hex3(val),
            ("lusty.selection", "fg") => t.sel_fg = hex3(val),
            ("lusty.selection", "bold") => t.sel_bold = val == "true",
            ("lusty.selection", "underline") => t.sel_underline = val == "true",
            ("lusty.selection", "reverse") => t.sel_reverse = val == "true",
            ("lusty.match", "fg") => t.match_fg = hex3(val),
            ("lusty.match", "underline") => t.match_underline = val == "true",
            _ => {}
        }
    }
    t
}

/// Resolved theme as `key\tvalue` lines, used by `--theme-map` and the nvim
/// parity smoke (values are `#rrggbb`, `true`/`false` or `none`).
pub fn theme_dump() -> Vec<(String, String)> {
    let t = load_theme();
    let hex = |c: Option<[u8; 3]>| {
        c.map(|[r, g, b]| format!("#{r:02x}{g:02x}{b:02x}"))
            .unwrap_or_else(|| "none".to_string())
    };
    vec![
        ("selection.bg".to_string(), hex(t.sel_bg)),
        ("selection.fg".to_string(), hex(t.sel_fg)),
        ("selection.bold".to_string(), t.sel_bold.to_string()),
        (
            "selection.underline".to_string(),
            t.sel_underline.to_string(),
        ),
        ("selection.reverse".to_string(), t.sel_reverse.to_string()),
        ("match.fg".to_string(), hex(t.match_fg)),
        ("match.underline".to_string(), t.match_underline.to_string()),
    ]
}

static THEME: std::sync::OnceLock<Theme> = std::sync::OnceLock::new();
fn theme() -> &'static Theme {
    THEME.get_or_init(load_theme)
}

/// SGR params for the selected cell, built from the loaded theme (or a
/// default neg-blue bar when no theme file is found).
fn sel_style() -> String {
    let mut s = String::new();
    if theme().sel_bold {
        s.push_str("1;");
    }
    if theme().sel_reverse {
        s.push_str("7;");
    }
    if theme().sel_underline {
        s.push_str("4;");
    }
    if let Some([r, g, b]) = theme().sel_bg {
        s.push_str(&format!("48;2;{};{};{};", r, g, b));
    }
    if let Some([r, g, b]) = theme().sel_fg {
        s.push_str(&format!("38;2;{};{};{};", r, g, b));
    }
    if s.is_empty() {
        s.push_str("1;48;2;0;95;175;38;2;209;229;255");
    }
    s.pop();
    s
}

/// Byte range of the first plain case-insensitive occurrence of `query` in the
/// basename of `label`, or None. Like the nvim float, a query starting with
/// '.' (dot-toggle) is ignored. Matching is ASCII-case-insensitive on the raw
/// bytes: the query is ASCII (the input normalizer drops other scripts) and a
/// match can only start on a char boundary, so non-ASCII labels are fine.
fn query_match(label: &str, query: &str) -> Option<(usize, usize)> {
    if query.is_empty() || query.starts_with('.') {
        return None;
    }
    let base_start = label.rfind('/').map(|i| i + 1).unwrap_or(0);
    let hay = &label.as_bytes()[base_start..];
    let needle = query.as_bytes();
    if needle.is_empty() || needle.len() > hay.len() {
        return None;
    }
    for i in 0..=(hay.len() - needle.len()) {
        if hay[i..i + needle.len()].eq_ignore_ascii_case(needle) {
            return Some((base_start + i, base_start + i + needle.len()));
        }
    }
    None
}

pub const ICON_DIR: &str = "\u{f115}";
pub const ICON_FILE: &str = "\u{f15b}";
pub const ICON_LINK: &str = "\u{f481}";

/// Extension icons, checked in order (first suffix match wins). Must stay in
/// sync with `lusty/icons.lua`; `--icon-map` prints this table for the nvim
/// parity smoke.
pub const ICON_EXTS: &[(&str, &str)] = &[
    (".md", "\u{f48a}"),
    (".rs", "\u{e7a8}"),
    (".lua", "\u{e620}"),
    (".scd", "\u{e620}"),
    (".sc", "\u{e620}"),
    (".jpg", "\u{f1c5}"),
    (".jpeg", "\u{f1c5}"),
    (".png", "\u{f1c5}"),
    (".webp", "\u{f1c5}"),
    (".gif", "\u{f1c5}"),
    (".mp3", "\u{f001}"),
    (".flac", "\u{f001}"),
    (".wav", "\u{f001}"),
    (".mp4", "\u{f03d}"),
    (".mkv", "\u{f03d}"),
    (".webm", "\u{f03d}"),
    (".zip", "\u{f410}"),
    (".tar", "\u{f410}"),
    (".gz", "\u{f410}"),
    (".7z", "\u{f410}"),
];

/// Nerd-font glyph for an entry; falls back to the generic file icon.
fn icon_for(e: &lusty_core::listing::Entry) -> &'static str {
    match e.kind {
        lusty_core::listing::FileKind::Dir => ICON_DIR,
        lusty_core::listing::FileKind::Link => ICON_LINK,
        _ => {
            let low = e.basename().to_ascii_lowercase();
            ICON_EXTS
                .iter()
                .find(|(suffix, _)| low.ends_with(*suffix))
                .map(|(_, glyph)| *glyph)
                .unwrap_or(ICON_FILE)
        }
    }
}

fn is_exec(path: &std::path::Path) -> bool {
    std::fs::metadata(path)
        .map(|m| std::os::unix::fs::PermissionsExt::mode(&m.permissions()) & 0o111 != 0)
        .unwrap_or(false)
}

/// Display width of one character in terminal cells. A compact approximation
/// covering what a picker meets: ASCII/Latin 1, CJK/kana/Hangul and emoji 2,
/// combining marks and joiners 0. The nvim float uses `strdisplaywidth`; this
/// keeps the standalone grid aligned for the same names.
pub(crate) fn char_width(c: char) -> usize {
    let u = c as u32;
    // Zero-width: combining marks, joiners, variation selectors, skin tones.
    if (0x0300..=0x036F).contains(&u)
        || (0x1AB0..=0x1AFF).contains(&u)
        || (0x1DC0..=0x1DFF).contains(&u)
        || (0x20D0..=0x20FF).contains(&u)
        || (0xFE00..=0xFE0F).contains(&u)
        || (0xFE20..=0xFE2F).contains(&u)
        || u == 0x200B
        || u == 0x200C
        || u == 0x200D
        || (0x1F3FB..=0x1F3FF).contains(&u)
    {
        return 0;
    }
    // East Asian Wide/Fullwidth and emoji.
    if (0x1100..=0x115F).contains(&u)
        || (0x2E80..=0xA4CF).contains(&u)
        || (0xAC00..=0xD7A3).contains(&u)
        || (0xF900..=0xFAFF).contains(&u)
        || (0xFE10..=0xFE19).contains(&u)
        || (0xFE30..=0xFE6F).contains(&u)
        || (0xFF00..=0xFF60).contains(&u)
        || (0xFFE0..=0xFFE6).contains(&u)
        || (0x1F300..=0x1FAFF).contains(&u)
        || (0x20000..=0x3FFFD).contains(&u)
    {
        return 2;
    }
    1
}

/// Display width of a string in terminal cells.
pub(crate) fn str_width(s: &str) -> usize {
    s.chars().map(char_width).sum()
}

/// Terminal-safe copy of a label or path for the frame: a byte like ESC in a
/// file name would otherwise let the file inject control sequences into the
/// terminal. Returns the input unchanged (borrowed) when there is nothing to
/// sanitize; TAB becomes a space, any other control char a '?'.
fn sanitize_display(s: &str) -> std::borrow::Cow<'_, str> {
    if !s.chars().any(char::is_control) {
        return std::borrow::Cow::Borrowed(s);
    }
    std::borrow::Cow::Owned(
        s.chars()
            .map(|c| {
                if c == '\t' {
                    ' '
                } else if c.is_control() {
                    '?'
                } else {
                    c
                }
            })
            .collect(),
    )
}

pub(crate) fn ansi_pad(line: &mut String, width: usize) {
    let src: Vec<char> = line.chars().collect();
    let mut out = String::with_capacity(src.len() + width);
    let mut vis = 0usize;
    let mut in_esc = false;
    for &c in &src {
        if in_esc {
            out.push(c);
            // '[' after ESC is the CSI introducer, not a final byte; the
            // escape ends at the first real final byte (0x40..=0x7e).
            if c == '[' {
                continue;
            }
            if (0x40..=0x7e).contains(&(c as u32)) {
                in_esc = false;
            }
            continue;
        }
        if c == '\x1b' {
            in_esc = true;
            out.push(c);
            continue;
        }
        if vis >= width {
            continue; // truncate visible content past the width
        }
        let w = char_width(c);
        if w == 0 {
            out.push(c); // stays attached to the previous glyph
            continue;
        }
        if vis + w > width {
            continue; // a wide char that does not fit: padding fills the cell
        }
        out.push(c);
        vis += w;
    }
    while vis < width {
        out.push(' ');
        vis += 1;
    }
    *line = out;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ru_layout_maps_to_en() {
        assert_eq!(normalize_query_char('и'), Some('b'));
        assert_eq!(normalize_query_char('ю'), Some('.'));
        assert_eq!(normalize_query_char('б'), Some(','));
        assert_eq!(normalize_query_char('е'), Some('t'));
        // Uppercase RU (Shift) maps to uppercase EN.
        assert_eq!(normalize_query_char('И'), Some('B'));
        // EN letters pass through.
        assert_eq!(normalize_query_char('b'), Some('b'));
        assert_eq!(normalize_query_char('.'), Some('.'));
    }

    #[test]
    fn non_ascii_unmapped_is_dropped() {
        assert_eq!(normalize_query_char('ä'), None);
    }
}

#[cfg(test)]
mod history_test {
    #[test]
    fn history_round_trips_and_disables() {
        let dir = std::env::temp_dir().join("lusty_history_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("history");

        std::env::set_var("LUSTY_HISTORY", &file);
        super::store_history("sub/fi");
        assert_eq!(super::load_history(), "sub/fi");

        // Missing file reads as an empty query.
        std::fs::remove_file(&file).unwrap();
        assert_eq!(super::load_history(), "");

        // "0" disables the feature entirely.
        std::env::set_var("LUSTY_HISTORY", "0");
        super::store_history("ignored");
        assert!(!file.exists());
        assert_eq!(super::load_history(), "");

        std::env::remove_var("LUSTY_HISTORY");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod pane_test {
    use super::*;

    fn fixture(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("lusty_async_pane_{tag}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("note.txt"), b"hello async preview\n").unwrap();
        dir
    }

    fn app_for(dir: &std::path::Path) -> App {
        let opts = Options {
            depth: 1,
            skip_dirs: vec![],
            follow_mounts: false,
            show_dots: false,
        };
        let mut app = App::new(dir.to_path_buf(), opts);
        // The query may come from the history file when other tests set it;
        // the pane test only cares about the selected entry.
        app.query.clear();
        app.needs_rank = true;
        app.preview_on = true;
        app.set_ui(Some(14), Some(80));
        app.compute_box();
        app.ensure_ranked();
        app
    }

    #[test]
    fn async_preview_installs_a_pane() {
        let dir = fixture("install");
        let mut app = app_for(&dir);
        assert!(!app.ranked.is_empty(), "fixture listed");
        app.selected = 0;
        app.refresh_pane();
        assert!(app.pane_rx.is_some(), "render spawned on a worker thread");

        let mut landed = false;
        for _ in 0..300 {
            if app.poll_pane() {
                landed = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(landed, "preview result picked up");
        assert!(
            app.pane.iter().any(|l| l.contains("hello async preview")),
            "pane shows the file content: {:?}",
            app.pane
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn clear_pane_drops_the_pending_render() {
        let dir = fixture("clear");
        let mut app = app_for(&dir);
        app.selected = 0;
        app.refresh_pane();
        app.clear_pane();
        assert!(app.pane_rx.is_none(), "receiver dropped");
        assert!(app.pane_key.is_none(), "key dropped");
        assert!(!app.poll_pane(), "nothing to install");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn kitty_env_override() {
        std::env::set_var("LUSTY_KITTY", "0");
        assert!(!kitty_enabled(), "explicit off");
        std::env::set_var("LUSTY_KITTY", "1");
        assert!(kitty_enabled(), "explicit on");
        std::env::remove_var("LUSTY_KITTY");
    }
}

#[cfg(test)]
mod grid_test {
    use super::*;

    #[test]
    fn cells_fill_columns_top_to_bottom() {
        // 3 rows per column: index 1 is below index 0 and index 3 starts the
        // second column at the same row (the fill order `draw` uses).
        assert_eq!(grid_index(0, 3, 0, 0), 0);
        assert_eq!(grid_index(0, 3, 1, 0), 1);
        assert_eq!(grid_index(0, 3, 2, 0), 2);
        assert_eq!(grid_index(0, 3, 0, 1), 3);
        assert_eq!(grid_index(5, 3, 0, 1), 8);
    }

    #[test]
    fn cells_read_down_then_right() {
        // The screen for a..f with 3 rows and 2 columns must read
        //   a  d
        //   b  e
        //   c  f
        // i.e. down the first column first. The transposed layout
        // (a b / c d / e f) is what `draw` used to produce while the arrow
        // keys already moved column-wise.
        let labels = ["a", "b", "c", "d", "e", "f"];
        let (rows, cols) = (3, 2);
        let screen: Vec<String> = (0..rows)
            .map(|r| {
                (0..cols)
                    .map(|c| labels[grid_index(0, rows, r, c)])
                    .collect()
            })
            .collect();
        assert_eq!(screen, ["ad", "be", "cf"]);
    }

    #[test]
    fn column_nav_moves_between_columns_and_wraps() {
        // 7 entries, 3 rows -> 3 columns, the last one short (row 0 only).
        assert_eq!(column_target(0, 1, 7, 3), 3, "right: next column, same row");
        assert_eq!(column_target(1, 1, 7, 3), 4);
        assert_eq!(column_target(3, 1, 7, 3), 6);
        assert_eq!(column_target(6, 1, 7, 3), 0, "right wraps to column 0");
        assert_eq!(
            column_target(0, -1, 7, 3),
            6,
            "left wraps to the last column"
        );
        assert_eq!(column_target(2, 2, 7, 3), 6, "short target column clamps");
        assert_eq!(column_target(0, 1, 0, 3), 0, "empty listing is a no-op");
    }

    #[test]
    fn window_scrolls_by_whole_columns() {
        // 12 entries, 5 rows, 2 visible columns: the first page is 0..10,
        // the second starts at the third column (index 5).
        assert_eq!(grid_offset(5, 0, 12, 5, 2), 0, "second column, same page");
        assert_eq!(
            grid_offset(10, 0, 12, 5, 2),
            5,
            "third column scrolls a page"
        );
        assert_eq!(grid_offset(11, 5, 12, 5, 2), 5, "last entry stays visible");
        assert_eq!(
            grid_offset(3, 5, 12, 5, 2),
            0,
            "scrolls back to the selection"
        );
        assert_eq!(
            grid_offset(11, 0, 12, 5, 8),
            0,
            "everything fits: no scroll"
        );
    }

    #[test]
    fn selection_stays_visible_in_the_window() {
        for n in 1..40 {
            for rows in 1..8 {
                for cols in 1..6 {
                    for selected in 0..n {
                        let offset = grid_offset(selected, 0, n, rows, cols);
                        let rel = selected - offset;
                        assert!(
                            rel < rows * cols,
                            "n={n} rows={rows} cols={cols} selected={selected} offset={offset}"
                        );
                        assert_eq!(
                            grid_offset(selected, offset, n, rows, cols),
                            offset,
                            "idempotent for n={n} rows={rows} cols={cols} selected={selected}"
                        );
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod view_test {
    use super::*;

    #[test]
    fn parses_the_cli_spellings() {
        assert_eq!(ViewMode::parse("list"), Some(ViewMode::List));
        assert_eq!(ViewMode::parse("grid"), Some(ViewMode::Grid));
        assert_eq!(ViewMode::parse("long"), Some(ViewMode::Long));
        assert_eq!(ViewMode::parse(" grid "), Some(ViewMode::Grid));
        assert_eq!(ViewMode::parse("fancy"), None);
        assert_eq!(ViewMode::parse(""), None);
    }

    #[test]
    fn the_cycle_visits_every_mode_once() {
        let mut mode = ViewMode::List;
        let seen: Vec<ViewMode> = (0..3)
            .map(|_| {
                let current = mode;
                mode = mode.next();
                current
            })
            .collect();
        assert_eq!(seen, [ViewMode::List, ViewMode::Grid, ViewMode::Long]);
        assert_eq!(mode, ViewMode::List, "the cycle returns to the start");
    }

    #[test]
    fn list_and_long_are_one_column_while_grid_adapts() {
        let dir = std::env::temp_dir().join("lusty_view_cols_test");
        let _ = std::fs::create_dir_all(&dir);
        let mut app = App::new(
            dir.clone(),
            Options {
                depth: 1,
                skip_dirs: vec![],
                follow_mounts: false,
                show_dots: false,
            },
        );
        app.box_w = 120;
        app.box_h = 8;
        app.maxw = 8;
        app.ranked = (0..40usize).collect(); // only the length matters here

        app.view = ViewMode::Grid;
        assert!(
            app.view_cols() > 1,
            "a wide popup with 40 entries packs columns"
        );
        app.view = ViewMode::List;
        assert_eq!(app.view_cols(), 1, "list never splits into columns");
        app.view = ViewMode::Long;
        assert_eq!(app.view_cols(), 1, "long keeps one entry per line");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Escapes of a rendered frame, minus the `ESC[<row>;1H` cursor moves:
    /// the result is one raw string per visual row, styling intact.
    fn frame_parts(app: &mut App) -> Vec<String> {
        let mut buf: Vec<u8> = Vec::new();
        app.draw(&mut buf).expect("render into a buffer");
        let text = String::from_utf8_lossy(&buf);
        let esc = char::from_u32(0x1b).unwrap();
        let mut rows: Vec<String> = Vec::new();
        let mut cur = String::new();
        let mut chars = text.chars();
        while let Some(ch) = chars.next() {
            if ch != esc {
                cur.push(ch);
                continue;
            }
            let mut seq = String::from(esc);
            if let Some(n) = chars.next() {
                seq.push(n);
            }
            let mut final_byte = None;
            for n in chars.by_ref() {
                seq.push(n);
                if n.is_ascii_alphabetic() || n == '~' {
                    final_byte = Some(n);
                    break;
                }
            }
            if final_byte == Some('H') {
                if !cur.is_empty() {
                    rows.push(std::mem::take(&mut cur));
                }
            } else {
                cur.push_str(&seq);
            }
        }
        if !cur.is_empty() {
            rows.push(cur);
        }
        rows
    }

    /// Frame rows with the styling stripped.
    fn frame_rows(app: &mut App) -> Vec<String> {
        frame_parts(app).iter().map(|r| plain(r)).collect()
    }

    /// Text of `s` with every SGR/erase escape dropped.
    fn plain(s: &str) -> String {
        let esc = char::from_u32(0x1b).unwrap();
        let mut out = String::new();
        let mut chars = s.chars();
        while let Some(ch) = chars.next() {
            if ch != esc {
                out.push(ch);
                continue;
            }
            for n in chars.by_ref() {
                if n.is_ascii_alphabetic() || n == '~' {
                    break;
                }
            }
        }
        out
    }

    /// Fixture with 20 entries. The directory is per test (`tag`): the tests
    /// run in parallel, and a shared temp dir had them deleting each other's
    /// listing mid-render.
    fn layout_fixture(tag: &str) -> (App, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("lusty_mode_layout_{tag}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for i in 0..20 {
            std::fs::write(dir.join(format!("f{i:02}.txt")), b"x").unwrap();
        }
        let mut app = App::new(
            dir.clone(),
            Options {
                depth: 1,
                skip_dirs: vec![],
                follow_mounts: false,
                show_dots: false,
            },
        );
        app.box_w = 60;
        app.box_h = 8; // 5 entry rows
        app.pop_top = 0;
        app.kitty = false;
        // A history file from a previous run must not filter the fixture.
        app.query.clear();
        app.needs_rank = true;
        (app, dir)
    }

    #[test]
    fn list_renders_one_entry_per_row() {
        let (mut app, dir) = layout_fixture("list");
        app.view = ViewMode::List;
        let rows = frame_rows(&mut app);
        let hits: Vec<&String> = rows.iter().filter(|r| r.contains(".txt")).collect();
        assert_eq!(hits.len(), 5, "list fills every entry row: {rows:?}");
        for row in hits {
            assert_eq!(
                row.matches(".txt").count(),
                1,
                "one entry per row in list mode: {row:?}"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn grid_packs_several_entries_per_row() {
        let (mut app, dir) = layout_fixture("grid");
        app.view = ViewMode::Grid;
        let rows = frame_rows(&mut app);
        let widest = rows
            .iter()
            .map(|r| r.matches(".txt").count())
            .max()
            .unwrap_or(0);
        assert!(
            widest >= 2,
            "grid must pack columns, widest row had {widest}: {rows:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn long_prepends_the_meta_fields() {
        let (mut app, dir) = layout_fixture("long");
        app.view = ViewMode::Long;
        let rows = frame_rows(&mut app);
        let hit = rows
            .iter()
            .find(|r| r.contains(".txt"))
            .expect("an entry row");
        assert!(
            hit.contains("-rw"),
            "long rows start with a perm field: {hit:?}"
        );
        assert_eq!(hit.matches(".txt").count(), 1, "one entry per line");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_selected_cell_keeps_its_background_across_the_line() {
        let (mut app, dir) = layout_fixture("sel");
        app.view = ViewMode::List;
        let rows = frame_parts(&mut app);
        let row = rows
            .iter()
            .find(|r| r.contains("f00.txt"))
            .expect("the selected entry row");
        let esc = char::from_u32(0x1b).unwrap();
        let start = row
            .find(&format!("{esc}[{}m", sel_style()))
            .expect("selection style on the selected row");
        let rest = &row[start..];
        let reset = rest
            .find(&format!("{esc}[22;23;24;39;"))
            .expect("style reset after the selection");
        let span = plain(&rest[..reset]);
        assert_eq!(
            str_width(&span),
            app.content_w(),
            "the highlight fills the line instead of stopping at the name: {span:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn prompt_counts_matched_over_total() {
        let (mut app, dir) = layout_fixture("count");
        // Empty query: everything matches the listing itself.
        let rows = frame_rows(&mut app);
        assert!(
            rows.iter().any(|r| r.contains("20/20")),
            "prompt shows matched/total: {rows:?}"
        );
        // Nothing matches, but the total stays the listing size.
        app.query = "zzzzz".to_string();
        app.needs_rank = true;
        let rows = frame_rows(&mut app);
        assert!(
            rows.iter().any(|r| r.contains("0/20")),
            "prompt keeps the total while filtering: {rows:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod width_test {
    use super::*;

    #[test]
    fn display_width_counts_cells() {
        assert_eq!(str_width("abc"), 3);
        assert_eq!(str_width("中"), 2);
        assert_eq!(str_width("中文"), 4);
        assert_eq!(str_width("e\u{301}"), 1, "combining acute is zero-width");
        assert_eq!(str_width("🙂"), 2);
        assert_eq!(char_width('\u{200d}'), 0, "zero-width joiner");
    }

    #[test]
    fn ansi_pad_uses_cells() {
        let mut s = String::from("中中");
        ansi_pad(&mut s, 3); // 4 cells: the second wide char does not fit
        assert_eq!(s, "中 ");
        let mut s = String::from("中");
        ansi_pad(&mut s, 4);
        assert_eq!(s, "中  ");
    }

    #[test]
    fn sanitize_display_strips_controls() {
        // ESC and friends in a file name must never reach the frame.
        assert_eq!(sanitize_display("plain name.txt"), "plain name.txt");
        assert_eq!(sanitize_display("a\u{1b}[2Jb\tc"), "a?[2Jb c");
        assert_eq!(sanitize_display("суб/файл.txt"), "суб/файл.txt");
    }

    #[test]
    fn query_match_handles_non_ascii() {
        let label = "суб/файл.txt";
        let (s, e) = query_match(label, "файл").expect("match in a non-ASCII label");
        assert_eq!(&label[s..e], "файл");

        let ascii = "café.txt";
        let (s, e) = query_match(ascii, "CAF").expect("ASCII case-insensitive");
        assert_eq!(&ascii[s..e], "caf");

        assert!(query_match("a/.hidden", ".").is_none(), "dot query ignored");
        assert!(query_match("abc", "zzz").is_none());
    }

    #[test]
    fn dsr_replies_parse() {
        let s = "\x1b[7;3R\x1b[24;80R";
        let replies: Vec<(usize, usize)> = s.split('R').filter_map(parse_dsr).collect();
        assert_eq!(replies, vec![(7, 3), (24, 80)]);
    }
}
