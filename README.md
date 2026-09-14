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
  art via SGR→extmarks)
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
            [--long] [--sort name|ext|size|time] [--reverse] [--dirs-first]
            [--columns perm,user,size,time]
```

Enter/Tab opens, C-t/C-o/C-v open in tabs/splits, C-n/C-p move, C-u clears,
Esc/C-c/C-g cancels, C-l toggles the long view, C-y cycles the sort order
(name/ext/size/time; `--sort` picks the starting order), C-d cycles the search
depth (1..6). The last typed query is restored on the next run.

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
cargo test
```

Unit tests cover ranking/colors/listing/cache/preview; `tests/serve_m.rs`
exercises the real binary over pipes (ranking memo, metadata requests, sort
cycling), `tests/serve_escape.rs` covers the escaping and non-UTF8 path
round-trip, `tests/serve_frec.rs` the frecency-ordered empty query and
`tests/serve_preview.rs` the `V` framing and clipping. CI (GitHub Actions) runs
`cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test`.

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
