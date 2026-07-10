//! Specialized TUI Widgets
//!
//! Port of `libvaxis` `src/widgets.zig` — the widgets aggregator. Chunk 4b lands the
//! first widget, `TextInput` (`widgets/TextInput.zig`); the remaining widgets
//! (alignment/Scrollbar/Table/ScrollView/LineNumbers/TextView/CodeView/Terminal/View)
//! are later chunks.

pub mod text_input;

// so `vaxis::widgets::TextInput` names the struct, matching Zig `widgets.TextInput`
pub use text_input::TextInput;
