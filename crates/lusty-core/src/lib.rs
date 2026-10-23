//! Shared listing stack of the lusty pickers: depth-limited directory walk,
//! on-disk listing cache, query ranking, LS_COLORS palette, glob/mount helpers,
//! the preview pane, and the interaction layer ported from `fzf-navigator`
//! (`history`, `icon_theme`, `keys`, `paths` — see CREDITS.md).
//!
//! Front-ends (`serve` for the nvim float, the standalone TUI, later the GUI)
//! are consumers: nothing here knows about crossterm, the line protocol or CLI
//! flags, so the same engine can back another interface unchanged.

pub mod cache;
pub mod colors;
pub mod glob;
pub mod history;
pub mod icon_theme;
pub mod keys;
pub mod listing;
pub mod mount;
pub mod paths;
pub mod preview;
pub mod rank;
pub mod text;
