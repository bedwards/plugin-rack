//! Core DSP, scheduling, and state for plugin-rack.
//!
//! This crate is format-agnostic. It does not link to nih_plug, VST3, or CLAP.
//! Higher-level crates (rack-plugin, rack-host-*) compose these primitives.

#![forbid(unsafe_op_in_unsafe_fn)]

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

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

// ─── Guest state persistence (issue #11) ─────────────────────────────────────
//
// The rack owns a list of `StripState`s: one per nested guest plugin. Each
// carries enough information to re-load the guest bundle on DAW reopen and
// to restore its opaque state blob byte-for-byte.
//
// The chunk schema mirrors the one described in `research/vst3_spec.md` §9
// "Nested plugin state serialization". VST3 guests fill `class_id` and
// `controller_state`; CLAP guests fill `plugin_id` and leave `controller_state`
// empty.

/// Which plugin format a strip's guest uses.
///
/// Persisted via serde so rack presets survive DAW round-trips. The variants
/// match the on-disk bundle extension.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GuestFormat {
    /// CLAP (`.clap`) — state blob is the raw `save()` output from the
    /// CLAP state extension.
    Clap,
    /// VST3 (`.vst3`) — `component_state` holds `IComponent::getState` bytes,
    /// `controller_state` holds `IEditController::getState` bytes.
    Vst3,
}

/// Persistent state for a single channel-strip's guest plugin.
///
/// Serialised byte-for-byte by nih_plug's Persist hook. Guest state blobs are
/// stored as `Vec<u8>` and must round-trip unchanged (acceptance criterion in
/// issue #11).
///
/// Fields:
/// * `format` — CLAP or VST3; selects the reload code path on open.
/// * `path` — absolute path of the guest bundle at save time.
/// * `class_id` — 16-byte VST3 TUID (Steinberg class UID). `None` for CLAP.
/// * `plugin_id` — CLAP plugin identifier (e.g. `"com.foo.bar"`). `None`
///   for VST3.
/// * `macro_map` — indices of guest parameters currently bound to rack macro
///   slots. Length is bounded by [`MACRO_SLOTS`]; entries are guest-native
///   parameter indices/ids expressed as `u32`.
/// * `component_state` — opaque bytes from the guest's `get_state` call.
///   For VST3 guests this is `IComponent::getState`; for CLAP this is
///   the `state` extension's `save()` output.
/// * `controller_state` — opaque bytes from `IEditController::getState`
///   (VST3 only). Empty for CLAP.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StripState {
    pub format: GuestFormat,
    pub path: PathBuf,
    pub class_id: Option<[u8; 16]>,
    pub plugin_id: Option<String>,
    pub macro_map: Vec<u32>,
    pub component_state: Vec<u8>,
    pub controller_state: Vec<u8>,
}

impl StripState {
    /// Construct an empty CLAP strip entry. Caller fills state blobs afterwards.
    pub fn new_clap(path: PathBuf, plugin_id: String) -> Self {
        Self {
            format: GuestFormat::Clap,
            path,
            class_id: None,
            plugin_id: Some(plugin_id),
            macro_map: Vec::new(),
            component_state: Vec::new(),
            controller_state: Vec::new(),
        }
    }

    /// Construct an empty VST3 strip entry. Caller fills state blobs afterwards.
    pub fn new_vst3(path: PathBuf, class_id: [u8; 16]) -> Self {
        Self {
            format: GuestFormat::Vst3,
            path,
            class_id: Some(class_id),
            plugin_id: None,
            macro_map: Vec::new(),
            component_state: Vec::new(),
            controller_state: Vec::new(),
        }
    }
}

/// Abstraction over a hosted guest's state-blob round trip.
///
/// Both `ClapGuest` (rack-host-clap) and `Vst3Guest` (rack-host-vst3)
/// implement this as a thin shim over their existing `get_state` /
/// `set_state` inherent methods. Having the trait lets the rack drive state
/// saves/loads uniformly across formats and lets tests substitute a
/// `MockGuest`.
///
/// For VST3 guests `get_state` returns only the processor (`IComponent`)
/// state; `IEditController` state is handled by a separate call path at the
/// host layer (see `research/vst3_spec.md` §"Two states, two streams").
///
/// See `research/state_api_notes.md` for API rationale (why `get_state`
/// returns an owned `Vec<u8>` rather than filling a caller-supplied buffer).
pub trait GuestStateSource {
    fn get_state(&mut self) -> anyhow::Result<Vec<u8>>;
    fn set_state(&mut self, bytes: &[u8]) -> anyhow::Result<()>;
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

    // ── StripState / GuestStateSource round-trip tests (issue #11) ──────────

    /// Populate a `StripState` with non-trivial opaque blobs, serialize to
    /// JSON, deserialize, assert byte equality on every field.
    ///
    /// JSON is nih_plug's on-disk format (see `persist::serialize_field` /
    /// `deserialize_field` which alias `serde_json::to_string` /
    /// `serde_json::from_str`).
    #[test]
    fn strip_state_roundtrip_bytes() {
        // VST3 variant with both component and controller bytes populated.
        let original = StripState {
            format: GuestFormat::Vst3,
            path: PathBuf::from("/Library/Audio/Plug-Ins/VST3/Surge XT.vst3"),
            class_id: Some([
                0x93, 0x2A, 0x56, 0x7F, 0xAB, 0xCD, 0xEF, 0x01, 0x23, 0x45, 0x67, 0x89, 0xDE, 0xAD,
                0xBE, 0xEF,
            ]),
            plugin_id: None,
            macro_map: vec![0, 3, 7, 127],
            component_state: (0u8..=255).collect(),
            controller_state: b"opaque controller chunk \x00\x01\x02\xFF".to_vec(),
        };

        let json = serde_json::to_string(&original).expect("serialize");
        let round: StripState = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(round.format, original.format);
        assert_eq!(round.path, original.path);
        assert_eq!(round.class_id, original.class_id);
        assert_eq!(round.plugin_id, original.plugin_id);
        assert_eq!(round.macro_map, original.macro_map);
        assert_eq!(round.component_state, original.component_state);
        assert_eq!(round.controller_state, original.controller_state);

        // CLAP variant — controller_state stays empty.
        let clap = StripState::new_clap(
            PathBuf::from("/Library/Audio/Plug-Ins/CLAP/Surge XT.clap"),
            "org.surge-synth-team.surge-xt".into(),
        );
        let json2 = serde_json::to_string(&clap).expect("clap serialize");
        let round2: StripState = serde_json::from_str(&json2).expect("clap deserialize");
        assert_eq!(round2.format, GuestFormat::Clap);
        assert_eq!(round2.class_id, None);
        assert_eq!(
            round2.plugin_id.as_deref(),
            Some("org.surge-synth-team.surge-xt")
        );
        assert!(round2.controller_state.is_empty());
    }

    /// End-to-end round-trip of a full persistable-params-equivalent struct
    /// that holds a `Vec<StripState>` containing real guest bytes from a
    /// `MockGuest`.  Mirrors what nih_plug does internally when Params persist
    /// hooks fire on DAW save/load.
    #[test]
    fn mock_guest_state_cycle() {
        // Minimal mock guest: holds its own blob, echoes it back on
        // get_state/set_state. Proves the trait plumbing is sound.
        struct MockGuest {
            blob: Vec<u8>,
        }

        impl GuestStateSource for MockGuest {
            fn get_state(&mut self) -> anyhow::Result<Vec<u8>> {
                Ok(self.blob.clone())
            }
            fn set_state(&mut self, bytes: &[u8]) -> anyhow::Result<()> {
                self.blob = bytes.to_vec();
                Ok(())
            }
        }

        // Stand-in for the subset of `PluginRackParams` that is Persist-backed
        // state. `rack-core` does not link to nih_plug so we serialise this
        // analogue; the field list matches what `PluginRackParams` will hold.
        #[derive(Serialize, Deserialize, Debug, PartialEq)]
        struct RackPersistedState {
            macro_names: Vec<String>,
            strips: Vec<StripState>,
        }

        // 1. Source-side guest with a non-trivial blob.
        let mut source_guest = MockGuest {
            blob: (0u8..200).rev().collect(),
        };
        let blob = source_guest.get_state().expect("source get_state");

        let persisted = RackPersistedState {
            macro_names: (0..MACRO_SLOTS)
                .map(|i| format!("Macro {}", i + 1))
                .collect(),
            strips: vec![StripState {
                format: GuestFormat::Vst3,
                path: PathBuf::from("/tmp/mock.vst3"),
                class_id: Some([0xAB; 16]),
                plugin_id: None,
                macro_map: vec![1, 2, 3],
                component_state: blob.clone(),
                controller_state: vec![0xCC, 0xDD, 0xEE],
            }],
        };

        // 2. Serialise (as nih_plug would on DAW save) and deserialise (as on open).
        let json = serde_json::to_string(&persisted).expect("serialize persisted");
        let restored: RackPersistedState =
            serde_json::from_str(&json).expect("deserialize persisted");

        assert_eq!(restored, persisted);
        assert_eq!(restored.strips.len(), 1);

        // 3. Feed the restored component_state into a fresh MockGuest and
        //    verify the guest holds the exact same bytes.
        let mut dest_guest = MockGuest { blob: Vec::new() };
        dest_guest
            .set_state(&restored.strips[0].component_state)
            .expect("dest set_state");
        assert_eq!(dest_guest.blob, blob);
    }
}
