//! plugin-rack: nih_plug entry point.
//!
//! v0.2: 128 macro parameters visible to DAW modulation and automation.
//! Hosting, GUI, and IPC land in subsequent issues.

use nih_plug::prelude::*;
use parking_lot::Mutex;
use std::num::NonZeroU32;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// MacroParams — one nested param struct per slot
// ---------------------------------------------------------------------------

/// A single macro slot's nih_plug parameter.
///
/// `#[nested(array, group = "Macros")]` appends `_{idx+1}` to the `#[id]`
/// string, producing stable IDs `value_1` .. `value_128`.
#[derive(Params)]
struct MacroParams {
    #[id = "value"]
    pub value: FloatParam,
}

impl Default for MacroParams {
    fn default() -> Self {
        Self {
            value: FloatParam::new("Macro", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(5.0))
                .with_unit(""),
        }
    }
}

// ---------------------------------------------------------------------------
// PluginRackParams — top-level params struct
// ---------------------------------------------------------------------------

/// Top-level parameter struct.
///
/// * `macros`: 128-element array of `MacroParams`. nih_plug's `#[nested(array)]`
///   produces param IDs `value_1` .. `value_128`, each in group "Macros N".
/// * `macro_names`: persistent JSON blob — user-editable labels that survive
///   DAW save/load. Stored as `Arc<Mutex<Vec<String>>>` so both the plugin and
///   a future GUI can share it.
#[derive(Params)]
struct PluginRackParams {
    #[nested(array, group = "Macros")]
    pub macros: [MacroParams; rack_core::MACRO_SLOTS],

    #[persist = "macro_names"]
    pub macro_names: Arc<Mutex<Vec<String>>>,
}

impl Default for PluginRackParams {
    fn default() -> Self {
        Self {
            macros: std::array::from_fn(|_| MacroParams::default()),
            macro_names: Arc::new(Mutex::new(
                (0..rack_core::MACRO_SLOTS)
                    .map(|i| format!("Macro {}", i + 1))
                    .collect(),
            )),
        }
    }
}

// ---------------------------------------------------------------------------
// PluginRack
// ---------------------------------------------------------------------------

struct PluginRack {
    params: Arc<PluginRackParams>,
}

impl Default for PluginRack {
    fn default() -> Self {
        Self {
            params: Arc::new(PluginRackParams::default()),
        }
    }
}

impl Plugin for PluginRack {
    const NAME: &'static str = "plugin-rack";
    const VENDOR: &'static str = "vibe";
    const URL: &'static str = "https://github.com/bedwards/plugin-rack";
    const EMAIL: &'static str = "brian.mabry.edwards@gmail.com";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),
        aux_input_ports: &[],
        aux_output_ports: &[],
        names: PortNames::const_default(),
    }];

    const MIDI_INPUT: MidiConfig = MidiConfig::None;
    const MIDI_OUTPUT: MidiConfig = MidiConfig::None;
    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn process(
        &mut self,
        _buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        ProcessStatus::Normal
    }
}

impl ClapPlugin for PluginRack {
    const CLAP_ID: &'static str = "dev.vibe.plugin-rack";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Mixing-console plugin rack");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> =
        Some("https://github.com/bedwards/plugin-rack/issues");
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Mixing,
        ClapFeature::Utility,
    ];
}

impl Vst3Plugin for PluginRack {
    const VST3_CLASS_ID: [u8; 16] = *b"vibePluginRack01";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Stereo];
}

nih_export_clap!(PluginRack);
nih_export_vst3!(PluginRack);

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macro_slot_count_is_128() {
        let params = PluginRackParams::default();
        assert_eq!(params.macros.len(), 128);
    }

    #[test]
    fn macro_names_default() {
        let params = PluginRackParams::default();
        let names = params.macro_names.lock();
        assert_eq!(names[0], "Macro 1");
        assert_eq!(names[127], "Macro 128");
        assert_eq!(names.len(), 128);
    }

    #[test]
    fn macro_value_range() {
        let params = PluginRackParams::default();
        // FloatRange::Linear { min: 0.0, max: 1.0 } — verify default is in range
        let default_val = params.macros[0].value.default_plain_value();
        assert!((0.0..=1.0).contains(&default_val));
    }
}
