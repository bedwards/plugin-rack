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
}
