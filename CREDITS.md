# Credits and provenance

lusty is licensed under the MIT license (see `LICENSE`). Two upstream projects
shaped the browse-mode interaction layer; their licenses and the exact parts
that were ported are recorded here.

## fzf-navigator — the interaction layer was ported from it

<https://github.com/benward2301/fzf-navigator> — an fzf-driven file system
navigator for zsh/bash by **Ben Ward**, MIT licensed.

`fzf-navigator` is the direct source of the browsing UX that lusty's standalone
picker now implements. The port keeps its semantics (and its invariants) rather
than its shell implementation; the shell-specific parts — fzf bindings, zsh/bash
line editing, `eza` invocation — have no counterpart here.

| lusty | fzf-navigator |
|---|---|
| `crates/lusty-core/src/keys.rs` | the `action:key` table, `FZF_NAVIGATOR_BINDINGS` override rules, the `?` help footer built from that table |
| `crates/lusty-core/src/history.rs` | `history` + `history_index` in the session tmpdir: back / forward / session start / cancel |
| `crates/lusty-core/src/paths.rs` | `__fzf_navigator_relpath`, `tildify`, `common_parent`, the `printf %q` quoting of `copy_selection` |
| `crates/lusty-core/src/icon_theme.rs` | the `theme.yml` truecolor icon palette |
| `src/tui.rs` — locked mode | `toggle_locked` / `locked` and the `cd` target it prints on exit |
| `src/tui.rs` — status flags | the `H` / `I` / `L` / `R` indicators and the `-N` hidden-entry count |
| `src/tui.rs` — toggles | `toggle_hidden_files`, `toggle_ignored_files`, `toggle_recent_first`, `reload` |
| `src/tui.rs` — `insert` / `copy_selection` | relative-path insertion, shell quoting, clipboard copy |
| `src/tui.rs` — help overlay | `toggle_help` replacing the footer with the keymap |

Behaviour that deliberately differs (lusty is a fuzzy *picker*, not a shell
browser):

- keys that print a character (`/`, `[`, `\`, `?`) act only while the prompt is
  empty; with text in the prompt they are typed, so every character stays
  reachable (`route_key` in `src/tui.rs`);
- `ctrl-d`, `ctrl-l`, `ctrl-r` and `ctrl-y` keep their original lusty meanings
  or the terminal-canonical ones (depth cycle; **redraw**; **refresh**; sort
  cycle), so the ported actions moved to `alt-d`, `alt-l`, `alt-t` and `alt-p`;
  the README lists the full translation and `LUSTY_KEYS` rebinds any of them;
- navigation happens in-process instead of relaunching the picker, so the
  history is a small stack instead of two temp files.

### MIT license of fzf-navigator

```
MIT License

Copyright (c) 2026 Ben Ward

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## broot — design reference

<https://github.com/Canop/broot> by **Denys Séguret (Canop)**, MIT licensed.

broot is the reference point for the *shape* of a tree browser: a persistent
panel that you navigate in place, a status line that reports the current mode
and the number of hidden/ignored entries, and a help screen that is generated
from the same action table the keys come from. lusty's browse mode follows that
model — it is the reason a directory picker stays open and moves its root
instead of exiting on every `cd`.

No broot code was copied; the influence is conceptual.
