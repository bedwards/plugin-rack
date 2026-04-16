//! GUI for plugin-rack.
//!
//! Stub — real implementation under issue "GUI: layout engine + strip UI".
//! Primary framework: vizia (via vizia-plug or custom baseview wiring).
//! Fallback: iced (BillyDM/iced_baseview) where native wrap layout is needed.

#![forbid(unsafe_op_in_unsafe_fn)]

/// The three layout modes the rack supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LayoutMode {
    /// Single row, L→R.
    #[default]
    Row,
    /// Vertical stack, T→B.
    Column,
    /// L→R with automatic wrap to next row.
    Wrap,
}
