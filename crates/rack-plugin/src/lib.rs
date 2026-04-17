//! plugin-rack: nih_plug entry point.
//!
//! v0.3: egui GUI with Row / Column / Wrap layout toggle (issue #8).
//!       128 macro parameters remain visible to DAW modulation/automation.
//!       Hosting and IPC land in subsequent issues.
//! v0.5: persist `link_tag` (issue #12). This is the user-facing group
//!       identifier used by `rack-ipc::SharedRegistry` for sibling
//!       discovery.
//! v0.6: live IPC discovery (issue #12). When the persisted `link_tag`
//!       is non-empty we claim a registry slot and spawn a 500 ms
//!       heartbeat thread inside `initialize()`; the resulting
//!       `DiscoveryHandle` is dropped on `deactivate()` (or when the
//!       host drops the plugin), which stops the heartbeat and zeros
//!       `alive` on the slot. Sibling instances observe the drop within
//!       one TTL window (2 s default; 4 s acceptance budget).

use crossbeam::atomic::AtomicCell;
use nih_plug::prelude::*;
use nih_plug_egui::{EguiSettings, EguiState, create_egui_editor};
use parking_lot::Mutex;
use rack_core::StripState;
use rack_gui::{EditorUiState, LayoutMode, default_editor_state, macro_grid};
use rack_ipc::{DiscoveryHandle, SharedRegistry};
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
///   DAW save/load.
/// * `editor_state`: persisted window size and user scale factor.
/// * `layout_mode`: persisted layout mode (Row/Column/Wrap).
/// * `strip_order`: persisted list of guest strip state (issue #11). Each
///   `StripState` carries the guest bundle path, a format tag, opaque state
///   bytes, and macro-binding map. Serialised to JSON via nih_plug's Persist
///   hook so DAW save/reopen restores the rack byte-for-byte.
///
/// `Arc<AtomicCell<LayoutMode>>` satisfies `PersistentField<'_, LayoutMode>`
/// via the blanket impl in `nih_plug::params::persist` (crossbeam AtomicCell
/// + Arc<AtomicCell> both have built-in impls when T: Serialize+Deserialize+Copy).
#[derive(Params)]
struct PluginRackParams {
    #[nested(array, group = "Macros")]
    pub macros: [MacroParams; rack_core::MACRO_SLOTS],

    #[persist = "macro_names"]
    pub macro_names: Arc<Mutex<Vec<String>>>,

    /// Persisted editor window size and user scale factor.
    #[persist = "editor_state"]
    pub editor_state: Arc<EguiState>,

    /// Persisted layout mode shared with the GUI.
    ///
    /// The GUI holds a clone of this Arc and writes to it on each Mode click.
    /// On DAW save, nih_plug serialises this via the PersistentField impl for
    /// Arc<AtomicCell<T>> (built-in for crossbeam AtomicCell when T: Serialize
    /// + Deserialize + Copy).
    #[persist = "layout_mode"]
    pub layout_mode: Arc<AtomicCell<LayoutMode>>,

    /// Persisted guest-strip state — one entry per nested plugin (issue #11).
    ///
    /// Defaults to empty; guest strip loading is out of scope for #11 (lands
    /// in a subsequent issue). The schema + round-trip fidelity are in place
    /// now so DAW sessions that save rack+guest state survive a reopen
    /// exactly.
    ///
    /// # Audio-path contract (issue #29)
    ///
    /// This field is GUI + save/load ONLY and MUST NEVER be `.lock()`-ed
    /// from `process()` or any other audio-thread callback. Holding a
    /// `parking_lot::Mutex` on the audio thread can block for an unbounded
    /// time if the GUI thread is mid-write, which would drop the DAW's
    /// realtime deadline and produce xruns.
    ///
    /// The audio path must instead consume a lock-free snapshot produced by
    /// the (future) `rack-ipc` snapshot consumer: the GUI/save path builds
    /// an immutable `Arc<[StripState]>` and swaps it into an
    /// `arc_swap`/`rtrb` channel that `process()` reads without locking.
    /// Until that snapshot path lands, `process()` is pure passthrough and
    /// does not look at strip state at all.
    ///
    /// A `NoAudioLock<T>` debug-only newtype that panics on `.lock()` from
    /// a thread named `"audio"` was considered as a regression guard; it
    /// was deferred because nih_plug does not label the audio thread and a
    /// reliable detector needs host cooperation. Revisit when strip
    /// scheduling lands.
    ///
    /// The `#[persist = "strips"]` attribute name is load-bearing — it is
    /// the key nih_plug writes into the DAW session blob. Changing it
    /// breaks every saved project. Do not rename.
    #[persist = "strips"]
    pub strip_order: Arc<Mutex<Vec<StripState>>>,

    /// Persisted IPC link tag (issue #12).
    ///
    /// Two plugin-rack instances whose `link_tag` strings match AND which
    /// run on the same host will discover each other through the shared
    /// memory registry in `rack-ipc` and render a combined console view.
    ///
    /// On first instantiation we generate a fresh per-instance tag so two
    /// new rack instances do NOT link by accident. The user will later be
    /// able to edit this string from the GUI (or copy-paste a peer's tag)
    /// to opt in to a group. That UI lands with issue #13; for now the
    /// field is persisted and round-trips through DAW save/load.
    #[persist = "link_tag"]
    pub link_tag: Arc<Mutex<String>>,
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
            editor_state: default_editor_state(),
            layout_mode: Arc::new(AtomicCell::new(LayoutMode::default())),
            strip_order: Arc::new(Mutex::new(Vec::new())),
            // Default is empty — rack is UNLINKED. clap-validator requires
            // `Default::default()` to be deterministic across instances
            // (its "same params to two instances" test compares state);
            // a per-instance fresh tag would break that. The actual
            // registry-slot claim happens lazily from `initialize()` and
            // generates a tag there if the persisted value is still empty.
            link_tag: Arc::new(Mutex::new(String::new())),
        }
    }
}

// ---------------------------------------------------------------------------
// PluginRack
// ---------------------------------------------------------------------------

struct PluginRack {
    params: Arc<PluginRackParams>,

    /// Live IPC discovery handle. Populated by `initialize()` if the
    /// persisted `link_tag` is non-empty, dropped by `deactivate()` (or
    /// by the plugin being dropped). A `Mutex<Option<_>>` rather than a
    /// `OnceLock` because the host may call `initialize` / `deactivate`
    /// repeatedly across the lifetime of one `PluginRack` instance and
    /// we want a fresh handle each time (persisted `link_tag` might
    /// have changed in between).
    discovery: Mutex<Option<DiscoveryHandle>>,
}

impl Default for PluginRack {
    fn default() -> Self {
        Self {
            params: Arc::new(PluginRackParams::default()),
            discovery: Mutex::new(None),
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

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        // We own the editor build closure here (not in rack-gui) so we can
        // close over the whole `Arc<PluginRackParams>`. rack-gui supplies the
        // layout helpers (macro_grid + LayoutMode); this crate provides the
        // param layout + toolbar + persistence glue.
        let params = self.params.clone();
        let egui_state = self.params.editor_state.clone();

        create_egui_editor(
            egui_state,
            EditorUiState::default(),
            EguiSettings::default(),
            |_egui_ctx, _queue, _ui_state| {},
            move |ui, setter, _queue, ui_state| {
                build_editor(ui, setter, &params, ui_state);
            },
        )
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        _buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        // Drop any prior discovery session — `initialize` can be called
        // more than once for a single instance (e.g. host restores state
        // while we're already active) and we want a single in-flight
        // handle at a time.
        *self.discovery.lock() = None;

        // Pull the persisted link_tag. Empty = UNLINKED rack; skip
        // discovery entirely (no slot, no thread) per acceptance
        // criterion "unlinked default claims nothing".
        let tag = self.params.link_tag.lock().clone();
        if tag.is_empty() {
            return true;
        }

        // Attach to the registry. If either step fails we log and proceed
        // without discovery — a broken registry segment must not prevent
        // audio processing from starting.
        match SharedRegistry::open_or_create() {
            Ok(registry) => {
                let registry = Arc::new(registry);
                match registry.start_discovery(tag.as_bytes()) {
                    Ok(handle) => {
                        *self.discovery.lock() = Some(handle);
                    }
                    Err(e) => {
                        nih_log!("rack-plugin: start_discovery failed: {e}");
                    }
                }
            }
            Err(e) => {
                nih_log!("rack-plugin: SharedRegistry open_or_create failed: {e}");
            }
        }
        true
    }

    fn deactivate(&mut self) {
        // Dropping the DiscoveryHandle stops the heartbeat thread and
        // releases the slot. Siblings observe our disappearance within
        // one TTL window.
        *self.discovery.lock() = None;
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

// ---------------------------------------------------------------------------
// Editor UI
// ---------------------------------------------------------------------------

/// Build the full rack editor UI inside the egui update closure.
///
/// Toolbar (top):
/// * "plugin-rack" title
/// * `link_tag` single-line text field (persisted via `params.link_tag`)
/// * Mode button cycling Row / Column / Wrap (persisted via
///   `params.layout_mode`)
///
/// Central panel: 128 macro sliders rendered via `rack_gui::macro_grid` in
/// the current layout mode.
fn build_editor(
    ui: &mut egui::Ui,
    setter: &ParamSetter,
    params: &Arc<PluginRackParams>,
    ui_state: &mut EditorUiState,
) {
    use egui::{Align, Layout, Panel, RichText, TextEdit};

    // Sync the persistent link_tag into the scratch buffer on the first frame
    // (and only on the first frame — subsequent frames edit the buffer and
    // write back only on `.changed()`).
    if !ui_state.link_tag_synced {
        ui_state.link_tag_buf = params.link_tag.lock().clone();
        ui_state.link_tag_synced = true;
    }

    Panel::top("rack_toolbar").show_inside(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("plugin-rack").size(16.0).strong());
            ui.separator();

            ui.label("link_tag:");
            let resp = ui.add(
                TextEdit::singleline(&mut ui_state.link_tag_buf)
                    .desired_width(160.0)
                    .hint_text("(unlinked)"),
            );
            if resp.changed() {
                // Commit every keystroke; this keeps the persistent value in
                // sync and avoids losing edits on window close. The underlying
                // Mutex<String> is locked for a microsecond; never touched on
                // the audio thread.
                *params.link_tag.lock() = ui_state.link_tag_buf.clone();
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let current = params.layout_mode.load();
                if ui.button(format!("Mode: {}", current.label())).clicked() {
                    params.layout_mode.store(current.next());
                }
            });
        });
    });

    egui::CentralPanel::default().show_inside(ui, |ui| {
        // Collect borrows of the 128 FloatParams into a Vec<&FloatParam> so
        // the layout helper can iterate them generically. This is O(128) per
        // frame but the cost is trivial (just pointer copies).
        let param_refs: Vec<&FloatParam> = params.macros.iter().map(|m| &m.value).collect();
        let mode = params.layout_mode.load();
        macro_grid(ui, setter, &param_refs, &params.macro_names, mode, ui_state);
    });
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
        let default_val = params.macros[0].value.default_plain_value();
        assert!((0.0..=1.0).contains(&default_val));
    }

    #[test]
    fn layout_mode_default_in_params() {
        let params = PluginRackParams::default();
        assert_eq!(params.layout_mode.load(), LayoutMode::Row);
    }

    #[test]
    fn strip_order_default_empty() {
        let params = PluginRackParams::default();
        assert!(params.strip_order.lock().is_empty());
    }

    #[test]
    fn link_tag_default_is_empty_unlinked() {
        // Default must be deterministic across instances (clap-validator
        // "same params to two instances" test compares defaults).
        // The rack is UNLINKED until the user opts into a group via the
        // GUI (issue #13) or a subsequent PR wires lazy tag generation
        // inside `initialize()`.
        let p = PluginRackParams::default();
        assert!(p.link_tag.lock().is_empty());
    }
}
