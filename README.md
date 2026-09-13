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
  icons (`LUSTY_ICONS=1`), runtime search depth (`C-d`, 1..6) and a restored
  last query (`$XDG_STATE_HOME/lusty/history`; `LUSTY_HISTORY` overrides the
  file, `LUSTY_HISTORY=0` disables)
- `serve` subcommand: headless backend for nvim floating windows — no
  terminal buffer involved, so it renders reliably even inside a web xterm
- Neovim float client: `C-l` toggles the long view (metadata via the `M`
  request, fetched only for visible rows), `C-y` cycles the sort order,
  `C-s`/sort keys follow the config, `C-Space` marks files (multi-select)
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
Q <from> <to> <query> [sort]      -> N <matched>, W <maxw>, R rows, E
M <mask> <index>...               -> K <index> <meta> per index, E
D                                 -> top-level dirs (for '/' completion), E
F <score> <path>                  frecency record, no reply
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

## Tests

```
cargo test
```

Unit tests cover ranking/colors/listing/cache; `tests/serve_m.rs` exercises
the real binary over pipes (ranking memo, metadata requests, sort cycling),
`tests/serve_escape.rs` covers the escaping and non-UTF8 path round-trip and
`tests/serve_frec.rs` the frecency-ordered empty query.

## License

MIT
