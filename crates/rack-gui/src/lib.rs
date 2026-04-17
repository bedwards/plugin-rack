//! GUI for plugin-rack — egui-based layout helpers.
//!
//! This crate deliberately does NOT depend on `rack-plugin` (that would be a
//! circular dep). Instead it ships small, parameter-agnostic helpers that the
//! plugin crate composes into a full editor:
//!
//! * [`LayoutMode`] — Row / Column / Wrap enum, persisted by the plugin.
//! * [`default_editor_state`] — a sensible default `EguiState` size.
//! * [`macro_grid`] — renders a list of `nih_plug::params::FloatParam`s as
//!   `ParamSlider` widgets with editable labels, laid out per [`LayoutMode`].
//! * [`EditorUiState`] — per-editor-instance scratch state (which label is
//!   currently being edited, the in-progress text) owned by the plugin and
//!   handed to `macro_grid` each frame.
//!
//! The plugin crate owns `create_egui_editor(...)` itself so it can close over
//! its full `Arc<PluginRackParams>` without a dependency cycle.
//!
//! Note: The BillyDM nih-plug fork (rev 3e0c4ac0) does not ship nih_plug_vizia;
//! its GUI adapter is nih_plug_egui (egui 0.34). This is a pragmatic choice
//! consistent with the fork's design. Vizia migration can follow when a vizia
//! adapter compatible with the BillyDM fork is available.

// guest_view uses unsafe NSView / COM calls; the module documents each
// invariant and wraps every unsafe operation in an explicit `unsafe {}`
// block even inside `unsafe fn`, so the Rust 2024 default-deny lint is
// restored here (issue #26).
#![forbid(unsafe_op_in_unsafe_fn)]

pub mod guest_view;

use egui::{Grid, Key, ScrollArea, TextEdit};
use nih_plug::prelude::{FloatParam, ParamSetter};
use nih_plug_egui::{EguiState, widgets::ParamSlider};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// ---------------------------------------------------------------------------
// LayoutMode
// ---------------------------------------------------------------------------

/// The three layout modes the rack supports.
///
/// Stored in an `Arc<AtomicCell<LayoutMode>>` so both the plugin struct (for
/// persistence) and the GUI can read/write without a lock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LayoutMode {
    /// Single row, L→R.
    #[default]
    Row,
    /// Vertical stack, T→B.
    Column,
    /// L→R with automatic wrap to the next row.
    Wrap,
}

impl LayoutMode {
    /// Cycle Row → Column → Wrap → Row.
    pub fn next(self) -> Self {
        match self {
            Self::Row => Self::Column,
            Self::Column => Self::Wrap,
            Self::Wrap => Self::Row,
        }
    }

    /// Human-readable label for the current mode.
    pub fn label(self) -> &'static str {
        match self {
            Self::Row => "Row",
            Self::Column => "Column",
            Self::Wrap => "Wrap",
        }
    }
}

// ---------------------------------------------------------------------------
// Editor sizing
// ---------------------------------------------------------------------------

/// Initial window size in logical pixels (before HiDPI scaling).
///
/// 900 × 520 fits a 16-column × 8-row wrap grid of 128 macro sliders at the
/// default `MACRO_SLIDER_WIDTH` + label line without squashing, and is small
/// enough to fit comfortably inside a Bitwig device-chain slot.
pub const EDITOR_WIDTH: u32 = 900;
pub const EDITOR_HEIGHT: u32 = 520;

/// Construct an [`EguiState`] with the editor's default size.
pub fn default_editor_state() -> Arc<EguiState> {
    EguiState::from_size(EDITOR_WIDTH, EDITOR_HEIGHT)
}

// ---------------------------------------------------------------------------
// Per-editor scratch state
// ---------------------------------------------------------------------------

/// Transient per-editor-instance UI state.
///
/// Owned by the plugin (the `user_state` slot of `create_egui_editor`) and
/// passed to [`macro_grid`] each frame. Not persisted — this is purely about
/// which label text field is currently open and what the user has typed so
/// far.
#[derive(Debug, Default)]
pub struct EditorUiState {
    /// If `Some(i)`, the macro at index `i` is currently in edit mode and
    /// `label_edit_buf` holds the in-progress text.
    pub editing_label: Option<usize>,
    /// Scratch buffer for the in-progress label edit.
    pub label_edit_buf: String,
    /// Scratch buffer for the `link_tag` text field, synced from the
    /// persistent `Mutex<String>` on change.
    pub link_tag_buf: String,
    /// True once we have populated `link_tag_buf` from the persistent
    /// mutex at least once; reset to false between editor opens is fine
    /// because the struct lives for one `create_egui_editor` lifetime.
    pub link_tag_synced: bool,
}

// ---------------------------------------------------------------------------
// Macro grid
// ---------------------------------------------------------------------------

/// Default width of each macro slider in logical pixels.
const MACRO_SLIDER_WIDTH: f32 = 110.0;
/// Width of the labelled cell (slider + space for text) in wrap/grid layouts.
const MACRO_CELL_WIDTH: f32 = 130.0;
/// Columns per row in the Wrap layout. 128 / 16 = 8 rows.
const WRAP_COLUMNS: usize = 16;

/// Render a grid of macro sliders, one per `FloatParam`.
///
/// * `ui` — parent egui Ui.
/// * `setter` — the `ParamSetter` provided by nih_plug_egui's update closure.
/// * `params` — slice of `FloatParam` references, one per slot.
/// * `names` — shared `Mutex<Vec<String>>` of user-editable labels. Mutated
///   only when the user commits a rename (Enter or focus-lost); otherwise
///   only a short read-lock per frame to build the label text.
/// * `mode` — current layout mode.
/// * `ui_state` — transient scratch for the in-progress label edit.
///
/// The three layout modes map to:
/// * `Row` — horizontal ScrollArea, one row of 128 sliders, left → right.
/// * `Column` — vertical ScrollArea, stacked top → bottom.
/// * `Wrap` — vertical ScrollArea, 16-column grid, 8 rows.
pub fn macro_grid(
    ui: &mut egui::Ui,
    setter: &ParamSetter,
    params: &[&FloatParam],
    names: &Mutex<Vec<String>>,
    mode: LayoutMode,
    ui_state: &mut EditorUiState,
) {
    match mode {
        LayoutMode::Row => {
            ScrollArea::horizontal()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.horizontal_top(|ui| {
                        for (i, param) in params.iter().enumerate() {
                            macro_cell(ui, setter, i, param, names, ui_state);
                        }
                    });
                });
        }
        LayoutMode::Column => {
            ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for (i, param) in params.iter().enumerate() {
                        macro_cell(ui, setter, i, param, names, ui_state);
                        ui.separator();
                    }
                });
        }
        LayoutMode::Wrap => {
            ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    Grid::new("macro_grid")
                        .num_columns(WRAP_COLUMNS)
                        .spacing([6.0, 8.0])
                        .min_col_width(MACRO_CELL_WIDTH)
                        .show(ui, |ui| {
                            for (i, param) in params.iter().enumerate() {
                                macro_cell(ui, setter, i, param, names, ui_state);
                                if (i + 1) % WRAP_COLUMNS == 0 {
                                    ui.end_row();
                                }
                            }
                        });
                });
        }
    }
}

/// Render one macro slot: label (double-click → edit) above a `ParamSlider`.
fn macro_cell(
    ui: &mut egui::Ui,
    setter: &ParamSetter,
    idx: usize,
    param: &FloatParam,
    names: &Mutex<Vec<String>>,
    ui_state: &mut EditorUiState,
) {
    ui.vertical(|ui| {
        // Label row — editable via double-click.
        let editing_this = ui_state.editing_label == Some(idx);
        if editing_this {
            let resp = ui.add(
                TextEdit::singleline(&mut ui_state.label_edit_buf)
                    .desired_width(MACRO_SLIDER_WIDTH),
            );
            // Focus the text box when we enter edit mode; egui's
            // `memory_mut` is the standard way to request focus.
            if !resp.has_focus() && resp.gained_focus() {
                // no-op; keeping structure for clarity
            }
            resp.request_focus();

            let commit = resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            let cancel = resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Escape));
            if commit {
                // Write back into the persistent names vec.
                let new_name = ui_state.label_edit_buf.trim().to_string();
                if !new_name.is_empty() {
                    let mut guard = names.lock();
                    if idx < guard.len() {
                        guard[idx] = new_name;
                    }
                }
                ui_state.editing_label = None;
                ui_state.label_edit_buf.clear();
            } else if cancel || (resp.lost_focus() && !commit) {
                ui_state.editing_label = None;
                ui_state.label_edit_buf.clear();
            }
        } else {
            // Read a snapshot of the label under a short lock.
            let label = {
                let guard = names.lock();
                guard
                    .get(idx)
                    .cloned()
                    .unwrap_or_else(|| format!("Macro {}", idx + 1))
            };
            let resp = ui.add(
                egui::Label::new(egui::RichText::new(label).small())
                    .truncate()
                    .sense(egui::Sense::click()),
            );
            if resp.double_clicked() {
                // Enter edit mode with the current label pre-filled.
                let current = {
                    let guard = names.lock();
                    guard
                        .get(idx)
                        .cloned()
                        .unwrap_or_else(|| format!("Macro {}", idx + 1))
                };
                ui_state.label_edit_buf = current;
                ui_state.editing_label = Some(idx);
            }
        }

        // Slider bound to the underlying FloatParam. Two-way: drags go through
        // `setter`; DAW modulation reads back through the param itself.
        ui.add(ParamSlider::for_param(param, setter).with_width(MACRO_SLIDER_WIDTH));
    });
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_mode_cycle() {
        assert_eq!(LayoutMode::Row.next(), LayoutMode::Column);
        assert_eq!(LayoutMode::Column.next(), LayoutMode::Wrap);
        assert_eq!(LayoutMode::Wrap.next(), LayoutMode::Row);
    }

    #[test]
    fn layout_mode_labels() {
        assert_eq!(LayoutMode::Row.label(), "Row");
        assert_eq!(LayoutMode::Column.label(), "Column");
        assert_eq!(LayoutMode::Wrap.label(), "Wrap");
    }

    #[test]
    fn layout_mode_default() {
        assert_eq!(LayoutMode::default(), LayoutMode::Row);
    }

    #[test]
    fn editor_ui_state_default() {
        let s = EditorUiState::default();
        assert!(s.editing_label.is_none());
        assert!(s.label_edit_buf.is_empty());
        assert!(s.link_tag_buf.is_empty());
        assert!(!s.link_tag_synced);
    }

    #[test]
    fn default_editor_state_size() {
        let s = default_editor_state();
        assert_eq!(s.size(), (EDITOR_WIDTH, EDITOR_HEIGHT));
    }

    #[test]
    fn editor_size_is_reasonable() {
        // Wrap layout: 16 cols × 130 px + padding ≈ 2100 px; our default is
        // 900 px which intentionally forces the horizontal ScrollArea in
        // Wrap mode. Height sized so ~4 rows fit without scrolling.
        const _: () = assert!(EDITOR_WIDTH >= 600);
        const _: () = assert!(EDITOR_HEIGHT >= 400);
    }
}
