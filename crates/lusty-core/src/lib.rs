//! Shared listing stack of the lusty pickers: depth-limited directory walk,
//! on-disk listing cache, query ranking, LS_COLORS palette, glob/mount helpers
//! and the preview pane.
//!
//! Front-ends (`serve` for the nvim float, the standalone TUI, later the GUI)
//! are consumers: nothing here knows about crossterm, the line protocol or CLI
//! flags, so the same engine can back another interface unchanged.

pub mod cache;
pub mod colors;
pub mod glob;
pub mod listing;
pub mod mount;
pub mod preview;
pub mod rank;
pub mod text;
