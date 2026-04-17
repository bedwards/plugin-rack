//! Core DSP, scheduling, and state for plugin-rack.
//!
//! This crate is format-agnostic. It does not link to nih_plug, VST3, or CLAP.
//! Higher-level crates (rack-plugin, rack-host-*) compose these primitives.

#![forbid(unsafe_op_in_unsafe_fn)]

/// Fixed macro-parameter count exposed by the rack to the host.
///
/// Static because VST3 / most hosts do not handle dynamic parameter lists
/// reliably. Nested plugin parameters are mapped into these slots by the
/// host crate.
pub const MACRO_SLOTS: usize = 128;

/// Per-slot runtime state (audio thread holds values; GUI thread may read).
#[derive(Clone, Debug)]
pub struct MacroSlot {
    /// Normalized value in 0.0..=1.0.
    pub value: f32,
    /// User-editable label; default is "Macro N" (1-indexed).
    pub name: String,
}

impl MacroSlot {
    pub fn default_for(index: usize) -> Self {
        Self {
            value: 0.0,
            name: format!("Macro {}", index + 1),
        }
    }
}

/// A placeholder for the rack's audio-thread state. Will grow as hosting
/// lands (rack-host-clap, rack-host-vst3).
#[derive(Default)]
pub struct RackState;

impl RackState {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macro_slot_count_is_128() {
        assert_eq!(MACRO_SLOTS, 128);
    }

    #[test]
    fn rack_state_constructs() {
        let _ = RackState::new();
    }

    #[test]
    fn macro_slot_default_name() {
        assert_eq!(MacroSlot::default_for(0).name, "Macro 1");
        assert_eq!(MacroSlot::default_for(127).name, "Macro 128");
    }

    #[test]
    fn macro_slot_default_value() {
        assert_eq!(MacroSlot::default_for(0).value, 0.0);
    }
}
