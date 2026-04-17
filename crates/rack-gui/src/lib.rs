//! GUI for plugin-rack — egui-based layout engine.
//!
//! Implements issue #8: Row / Column / Wrap layout toggle visible in the editor
//! window. Strip slots are empty for now; guest-plugin rendering lands in a
//! future issue.
//!
//! Note: The BillyDM nih-plug fork (rev 3e0c4ac0) does not ship nih_plug_vizia;
//! its GUI adapter is nih_plug_egui (egui 0.34). This is a pragmatic choice
//! consistent with the fork's design. Vizia migration can follow when a vizia
//! adapter compatible with the BillyDM fork is available.

// guest_view uses unsafe NSView / COM calls; the module documents each invariant.
#![allow(unsafe_op_in_unsafe_fn)]

pub mod guest_view;

use crossbeam::atomic::AtomicCell;
use egui::{Align, Layout, Panel, RichText};
use nih_plug::prelude::Editor;
use nih_plug_egui::{EguiSettings, EguiState, create_egui_editor};
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
// GuiState — egui user_state (owned by the GUI thread)
// ---------------------------------------------------------------------------

/// State owned exclusively by the egui GUI closure.
///
/// `layout_cell` is a shared `Arc<AtomicCell<LayoutMode>>` so the plugin can
/// persist the value on DAW save.
pub struct GuiState {
    pub layout_cell: Arc<AtomicCell<LayoutMode>>,
}

// ---------------------------------------------------------------------------
// Editor factory
// ---------------------------------------------------------------------------

/// Initial window size in logical pixels (before HiDPI scaling).
pub const EDITOR_WIDTH: u32 = 800;
pub const EDITOR_HEIGHT: u32 = 600;

/// Construct an [`EguiState`] with the editor's default size.
pub fn default_editor_state() -> Arc<EguiState> {
    EguiState::from_size(EDITOR_WIDTH, EDITOR_HEIGHT)
}

/// Create and return a boxed [`Editor`] for the rack plugin.
///
/// * `egui_state` — persisted window size / scale; store in `#[persist]`.
/// * `layout_cell` — shared layout mode; updated by the GUI, persisted by
///   the plugin via `#[persist = "layout_mode"]`.
pub fn create_editor(
    egui_state: Arc<EguiState>,
    layout_cell: Arc<AtomicCell<LayoutMode>>,
) -> Option<Box<dyn Editor>> {
    let gui_state = GuiState {
        layout_cell: layout_cell.clone(),
    };

    create_egui_editor(
        egui_state,
        gui_state,
        EguiSettings::default(),
        // build callback: runs once when the window opens
        |_egui_ctx, _queue, _state| {},
        // update callback: runs every frame
        move |ui, _setter, _queue, state| {
            build_rack_ui(ui, state);
        },
    )
}

// ---------------------------------------------------------------------------
// UI layout
// ---------------------------------------------------------------------------

/// Top-level UI: toolbar + strip area.
fn build_rack_ui(ui: &mut egui::Ui, state: &mut GuiState) {
    Panel::top("toolbar").show_inside(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("plugin-rack").size(18.0).strong());

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let current = state.layout_cell.load();
                ui.label(current.label());
                if ui.button("Mode").clicked() {
                    state.layout_cell.store(current.next());
                }
            });
        });
    });

    egui::CentralPanel::default().show_inside(ui, |ui| {
        #[allow(unused_variables)]
        let strip_count = 0usize; // populated in a future issue
        if strip_count == 0 {
            ui.centered_and_justified(|ui| {
                ui.label("Drop a plugin here");
            });
        }
        // Populated strip rendering lands in a future issue.
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
}
