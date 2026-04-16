//! plugin-rack: nih_plug entry point.
//!
//! v0.1: empty passthrough. Hosting, GUI, IPC land in subsequent issues.

use nih_plug::prelude::*;
use std::num::NonZeroU32;
use std::sync::Arc;

struct PluginRack {
    params: Arc<PluginRackParams>,
}

#[derive(Params, Default)]
struct PluginRackParams {}

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
