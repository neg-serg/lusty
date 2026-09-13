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
  icons (`LUSTY_ICONS=1`)
- `serve` subcommand: headless backend for nvim floating windows — no
  terminal buffer involved, so it renders reliably even inside a web xterm
- Neovim float client: `C-l` toggles the long view (metadata via the `M`
  request, fetched only for visible rows), `C-y` cycles the sort order,
  `C-s`/sort keys follow the config

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
(name/ext/size/time; `--sort` picks the starting order).

## serve protocol

Plain lines on stdin/stdout, one request per line:

```
C <total> <depth> <root>          ready banner
Q <from> <to> <query> [sort]      -> N <matched>, W <maxw>, R rows, E
M <mask> <index>...               -> K <index> <meta> per index, E
D                                 -> top-level dirs (for '/' completion), E
```

`sort`: 0 name, 1 ext, 2 size (desc), 3 time (desc). `meta` mask bits:
1 perm, 2 user, 4 size, 8 time. Rows carry absolute paths after a tab, so
the client never joins paths itself.

Backslash, TAB and LF inside a label, a path or a `D` name are escaped as
`\\`, `\t`, `\n`, so a file name containing them cannot break the framing.
Paths travel as raw Unix bytes: a name that is not valid UTF-8 is still
openable (the label, used for display only, is the lossy form).

## Tests

```
cargo test
```

Unit tests cover ranking/colors/listing/cache; `tests/serve_m.rs` exercises
the real binary over pipes (ranking memo, metadata requests, sort cycling) and
`tests/serve_escape.rs` covers the escaping and non-UTF8 path round-trip.

## License

MIT
