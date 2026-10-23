# lusty

Native file/buffer picker for Neovim: a fast Rust listing backend
(`lusty serve` over a plain-text line protocol) plus a standalone
terminal UI (raw-mode, eza-style).

Succeeds the original Vim [LustyExplorer](https://github.com/sjbach/Lusty)
UX: bottom float, incremental fuzzy filtering, RU/EN layouts — but with a
parallel walker, LS_COLORS coloring and metadata views.

## Features

- Depth-limited parallel listing (`rayon`), skip-dirs, mount-point pruning
- Fuzzy ranking with first-letter prefix anchoring (RU keyboard layout maps
  to EN automatically)
- Standalone TUI: grid or long view, eza-style columns
  (`--columns perm,user,size,time`, env `LUSTY_COLUMNS`),
  sorting (`--sort name|ext|size|time`, `--reverse`, `--dirs-first`),
  icons (`LUSTY_ICONS=1` or `--icons`), runtime search depth (`C-d`, 1..6), a
  starting query (`--query`), a restored last query
  (`$XDG_STATE_HOME/lusty/history`; `LUSTY_HISTORY` overrides the file,
  `LUSTY_HISTORY=0` disables), a display-cell-aware grid (CJK/emoji names stay
  aligned), the shared `theme.toml` (selection + match colours, same file as the
  nvim float) and an asynchronous right-hand preview pane (chafa / git diff /
  man) that places images with the kitty graphics protocol when the terminal
  supports it (`LUSTY_KITTY=0` disables, ANSI art otherwise)
- `serve` subcommand: headless backend for nvim floating windows — no
  terminal buffer involved, so it renders reliably even inside a web xterm
- Neovim float client: `C-l` toggles the long view (metadata via the `M`
  request, fetched only for visible rows), `C-y` cycles the sort order,
  `C-s`/sort keys follow the config, `C-Space` marks files (multi-select),
  `C-r` toggles the preview pane (file content / git diff / man / colour chafa
  art via SGR→extmarks). These are the float's own keys — the standalone picker
  below uses `alt-v`, `alt-p` and `alt-l` for the same toggles, because `C-l`
  and `C-r` keep their terminal meanings there.
- Frecency-aware empty query: the client ships its open-frequency journal with
  `F` records and the listing puts the higher-scored paths first inside each
  depth level (disable with `LUSTY_FRECENCY=0` / `g:LustyExplorerFrecency=0`)

## Build

```
cargo build --release
```

Nix: `nix build .#lusty` (via `default.nix`,
`rustPlatform.buildRustPackage`).

## Standalone usage

```
lusty [root] [--depth N] [--skip a,b] [--rows N] [--width N]
            [--view list|grid|long] [--sort name|ext|size|time] [--reverse]
            [--dirs-first] [--columns perm,user,size,time]
            [--icons] [--query Q] [--locked] [--show-hidden]
            [--keys key=action,...]
```

Enter/Tab opens, C-t/C-o/C-v open in tabs/splits, C-n/C-p move, C-u clears,
`alt-v` cycles the layout (list → grid → long; `--view` or `LUSTY_VIEW` picks
the start, `--long` is an alias for `--view long`), C-y cycles the sort order
(name/ext/size/time; `--sort` picks the starting order), C-d cycles the search
depth (1..6), C-l repaints the picker, `alt-p` toggles the preview pane.
Esc/C-c/C-g walk back to the directory
the picker was launched from and only leave the picker when pressed there
(`--locked` starts in locked mode). The last typed query is restored on the
next run.

The browse-mode keys — history, parent/home/root jumps, `H`/`I`/`L`/`R`
toggles, `?` help, path insertion and clipboard copy — are ported from
[fzf-navigator](https://github.com/benward2301/fzf-navigator); see
[Browse mode](#browse-mode-ported-from-fzf-navigator) for the key table, the
invariants behind it and `LUSTY_KEYS` remapping.

C-x marks the entry under the cursor and steps to the next one. Marks survive
re-filtering, sorting and re-rooting; the prompt shows `+N`, and Enter then
prints one `ACTION<TAB>PATH` line per marked entry, in the order they were
marked. Marked cells keep a `+` in a one-cell column, so the state does not
depend on the tint alone.

`list` is one entry per row, `grid` (the default) packs as many adaptive
columns as the popup width allows — filled top-to-bottom, columns left to
right — and `long` puts the eza `-l` style mode/size/date in front of each
name, under a `PERM USER SIZE MODIFIED NAME` header whose columns line up with
the rows. The selected cell is highlighted across its whole width, and the
prompt ends with the search depth and a `matched/total` counter, so a filter
that hides everything is visible at a glance.

A window with a single entry row (`--rows 4`) becomes a horizontal strip in
every mode: `list` and `long` drop the meta columns and the header there
instead of showing one result at a time. The meta columns return with two
entry rows, the header with three.

## Browse mode (ported from fzf-navigator)

The standalone picker can behave like the file browser it grew out of: a panel
that stays open, walks directories instead of exiting on every `cd`, and shows
what mode it is in. The interaction layer comes from
[fzf-navigator](https://github.com/benward2301/fzf-navigator) (MIT, © Ben Ward)
— see `CREDITS.md` for the exact mapping and for the
[broot](https://github.com/Canop/broot) heritage of the panel model.

Keys, with the shell version's original binding when it differs:

| Action | lusty | fzf-navigator | What it does |
|---|---|---|---|
| `open` | `enter`, `tab` | `space` | enter a directory, or print the selection |
| `open_tab` / `open_split` / `open_vsplit` | `ctrl-t` / `ctrl-o` / `ctrl-v` | `ctrl-o` (`open_and_exit`) | open the selection in a tab / split / vsplit |
| `insert_selection` | `alt-e` | `enter` | print the selection for insertion, relative to the **launch** dir |
| `copy_selection` | `alt-y` | `ctrl-y` | copy the selection (absolute, shell-quoted) via OSC 52 |
| `exit` | `ctrl-space` | `ctrl-space` | leave; in locked mode print the `cd` target |
| `cancel` | `esc`, `ctrl-c`, `ctrl-g` | `esc` | back to the launch dir, and leave only from there |
| `go_to_parent` | `alt-d` | `ctrl-d` | go to the parent directory |
| `go_back` / `go_forward` | `ctrl-alt-o` / `ctrl-alt-i` | same | walk the directory history |
| `go_to_session_start` | `[` | `[` | jump to the launch directory |
| `go_home` | `\` | `\` | jump to `$HOME` |
| `descend` | `/` | `/` (`go_to_root`) | descend into the typed directory, or `/` when the prompt is empty |
| `toggle_hidden` | `alt-h` | `alt-h` | show dot entries without typing a leading dot (`H`) |
| `toggle_skipped` | `alt-i` | `alt-i` | walk the `--skip` directories too (`I`) |
| `toggle_locked` | `alt-l` | `ctrl-l` | locked mode (`L`) |
| `toggle_recent_first` | `alt-t` | `ctrl-r` | newest-first order (`R`) |
| `toggle_preview` | `alt-p` | — | preview pane |
| `cycle_view` / `cycle_sort` / `cycle_depth` | `alt-v` / `ctrl-y` / `ctrl-d` | — | the layout, order and depth cycles lusty already had |
| `mark` | `ctrl-x` | — | mark the entry and step down |
| `reload` | `ctrl-r`, `alt-r` | `alt-r` | drop the caches and relist |
| `redraw` | `ctrl-l` | — | repaint the picker (the terminal-canonical `C-l`) |
| `toggle_help` | `?` | `?` | show the keymap overlay |

Invariants (each one is asserted in the test suite):

- **`C-l` repaints and `C-r` refreshes; neither switches a mode.** `C-l` means
  "redraw" in vim, in `less` and in the shell, and `C-r` means "re-read" in
  `less`, `mc` and most TUIs, so a layout cycle on `C-l` and a preview toggle on
  `C-r` both surprised people. Layout moved to `alt-v` (view), the preview pane
  to `alt-p` (pane), locked mode to `alt-l` (lock, the shell version's
  `ctrl-l`); `LUSTY_KEYS` puts any of them back.
- **one key, one action**; an action always keeps at least one key. A
  `LUSTY_KEYS` entry *moves* its action (the shipped key stops firing) and a
  second entry for the same action adds an alias — the shell version's
  override rule.
- **a typo costs one binding, not the keymap**: an unknown action or an
  unparsable key is reported on stderr and the rest of the spec still applies.
- **the help overlay is the keymap**: `?` renders `Action::ALL` order from the
  same table the dispatcher reads, so a rebind shows up immediately and an
  unbound action disappears. The overlay is scrollable and grows the popup
  instead of clipping itself to five rows.
- **printable action keys are shadows of real characters**: `/`, `[`, `\` and
  `?` act only while the prompt is empty. With text in the prompt they are
  typed, so `,`, `.`, `/` and every other character stay reachable
  (`route_key` in `src/tui.rs`, and `type_into` is the only path a character
  takes into the query). `/` keeps its one exception: it descends when the
  typed prefix names a unique directory, and otherwise types a slash.
- **inserted paths are anchored at the launch directory**, never at the browsed
  one — `cd`-ing around inside the picker cannot break the command line it is
  pasted into — and a bare executable name gets a `./` prefix so the shell does
  not resolve a `PATH` twin.
- **a fresh visit truncates the forward history tail**; `back`/`forward` stop at
  the ends instead of wrapping; the launch directory stays reachable from
  anywhere, which is what makes `cancel` a jump instead of an exit.
- **no state is encoded in colour alone**: `H`/`I`/`L`/`R`, `+N` and `-N` are
  letters and digits, and the icon tint never replaces the selection foreground
  (the WCAG floors above still hold).

Environment and flags for this layer:

| Variable / flag | Effect |
|---|---|
| `LUSTY_KEYS`, `--keys SPEC` | rebind actions: `key=action,key=action` (both orders and `:` work) |
| `LUSTY_LOCK_CWD=1`, `--locked` | start in locked mode |
| `LUSTY_SHOW_HIDDEN=1`, `--show-hidden` | start with dot entries shown |
| `LUSTY_ICON_THEME` | path to an eza `theme.yml`; `0`/`off` keeps only the built-in table |
| `--key-map` | print `action<TAB>keys` for the resolved keymap (nvim parity) |
| `--icon-color-map NAME...` | resolve icon colours per name (nvim parity) |

Two extra actions on the line protocol make the shell contract usable:
`insert<TAB>RELATIVE_PATH` (relative to the launch directory) and
`cd<TAB>ABSOLUTE_PATH` (only on `exit` in locked mode, when the browsed
directory changed).

Icon colours follow the `theme.yml` that `fzf-navigator` ships for eza: lusty
reads the file from `$EZA_CONFIG_DIR`, `$XDG_CONFIG_HOME/eza/theme.yml` or
`$XDG_CONFIG_HOME/lusty/icon-theme.yml` (first one that exists), and falls back
to a built-in table of the common extensions. Exact `filenames:` matches win
over extensions, the longest suffix wins over the shortest, and a name the theme
does not know keeps the `LS_COLORS` foreground.

### Colour invariants

Every colour the popup paints is held to a computed WCAG 2.1 floor
(`src/contrast.rs`, asserted in `contrast_invariants`): secondary text (meta,
header, counter) ≥ 4.5:1 on the popup, the selection bar ≥ 3:1 against it with
its text ≥ 4.5:1 on the bar, the `+` mark glyph ≥ 4.5:1 on the mark tint, and
the frame line at a 1.5:1 decorative floor. The cursor carries bold (or
underline/reverse) and the mark carries a glyph, so neither state is encoded in
colour alone. A `theme.toml` field that breaks its floor falls back to the
default value, so a bad theme degrades to readable colours instead of an
unreadable frame.

## serve protocol

Plain lines on stdin/stdout, one request per line:

```
C <total> <depth> <root>          ready banner
X preview                         capability line (right after the banner)
Q <from> <to> <query> [sort]      -> N <matched>, W <maxw>, R rows, E
M <mask> <index>...               -> K <index> <meta> per index, E
D                                 -> top-level dirs (for '/' completion), E
F <score> <path>                  frecency record, no reply
V <index> <w> <h>                 -> V <lines> <dim>, one "L <text>" per row, E
```

`sort`: 0 name, 1 ext, 2 size (desc), 3 time (desc). `meta` mask bits:
1 perm, 2 user, 4 size, 8 time. Rows carry absolute paths after a tab, so
the client never joins paths itself.

Backslash, TAB and LF inside a label, a path or a `D` name are escaped as
`\\`, `\t`, `\n`, so a file name containing them cannot break the framing.
Paths travel as raw Unix bytes: a name that is not valid UTF-8 is still
openable (the label, used for display only, is the lossy form). `F` records use
the same escaping in the client-to-server direction.

Labels are display text, so the remaining control bytes in them are replaced by
`?` before they leave the backend (and before `--list` prints them): a file
named `esc_ESC[2J_name` must not be able to drive the terminal or the client
buffer. Paths keep their bytes — the client opens those, it does not paint
them.

With at least one `F` record the empty query orders by (depth, frecency score,
canonical index): the shallower depth contract stays, and inside a depth the
most frequent/recent paths lead.

`V` renders the preview pane: a file with unstaged changes shows the git diff, a
clean file (or one outside a work tree) shows its content, images show chafa art
and man sources show the rendered page. SGR sequences are passed through (the
nvim client turns them into extmarks, so chafa art is coloured); other ANSI
escapes are dropped and rows are clipped by visible characters. Rows are
prefixed with `L ` so a content line equal to `E` cannot end the response early;
the client only sends `V` after seeing `X preview`.

## Tests

```
cargo test --workspace
```

Unit tests cover ranking/colors/listing/cache/preview and live in
`crates/lusty-core` (the bin keeps only the TUI and serve tests, so the
workspace flag is what runs them all); `tests/serve_m.rs`
exercises the real binary over pipes (ranking memo, metadata requests, sort
cycling), `tests/serve_escape.rs` covers the escaping and non-UTF8 path
round-trip, `tests/serve_frec.rs` the frecency-ordered empty query and
`tests/serve_preview.rs` the `V` framing and clipping. CI (GitHub Actions) runs
`cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`
and `cargo test --workspace`.

`--ru-map` / `--icon-map` / `--theme-map` / `--color-map` dump the shared
tables, the resolved theme and LS_COLORS probes for the nvim parity smokes
(`files/nvim/lua/lusty/tests/*_parity_smoke.lua`), which keep the Rust
standalone and the Lua float in sync.

## Benchmarks

```
scripts/bench.sh                 # repo + /etc/nixos at depth 2
BENCH_ROOTS=/nix/store scripts/bench.sh
```

Uses `hyperfine` when present (falls back to a millisecond loop) over
`lusty --list`, which is the listing path without the terminal UI.

## License

MIT
