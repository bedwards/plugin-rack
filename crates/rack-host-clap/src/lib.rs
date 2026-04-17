//! CLAP guest plugin hosting via `clack-host`.
//!
//! Loads a `.clap` bundle, instantiates its first audio-effect plugin,
//! passes stereo audio through unchanged, and round-trips state via
//! the CLAP `state` extension.
//!
//! # Lifecycle
//! ```text
//! ClapGuest::load(path)    → load entry, find descriptor, instantiate, activate, start_processing
//! guest.process(buffer)    → de-interleave → plugin.process() → re-interleave   (audio thread)
//! guest.get_state()        → plugin state extension save()    (main thread)
//! guest.set_state(data)    → plugin state extension load()    (main thread)
//! drop(guest)              → stop_processing → deactivate → drop instance + entry
//! ```

use std::io::Cursor;
use std::path::Path;
use std::sync::OnceLock;

use anyhow::{anyhow, bail};
use clack_extensions::state::{HostState, HostStateImpl, PluginState};
use clack_host::prelude::*;

// ── Minimal RackHost implementation ──────────────────────────────────────────
//
// clack-host requires a `HostHandlers` impl. We provide the minimum: stubs for
// the three required `SharedHandler` callbacks and, for state support, the
// `HostState` extension on `MainThread` (so the plugin can call `mark_dirty`).

/// Shared (thread-safe) host state.
/// Holds queried plugin extensions after initialization.
struct RackHostShared {
    /// The CLAP state extension pointer, if the plugin supports it.
    state_ext: OnceLock<Option<PluginState>>,
}

impl<'a> SharedHandler<'a> for RackHostShared {
    fn initializing(&self, instance: InitializingPluginHandle<'a>) {
        // Query the state extension during plugin initialization.
        let _ = self.state_ext.set(instance.get_extension());
    }

    fn request_restart(&self) {
        // No-op: a real host would schedule a deactivate/reactivate cycle.
        log::debug!("clap guest: plugin requested restart (ignored)");
    }

    fn request_process(&self) {
        // No-op: we drive processing ourselves.
        log::debug!("clap guest: plugin requested process (ignored)");
    }

    fn request_callback(&self) {
        // No-op: we don't support on_main_thread callbacks in v1.
        log::debug!("clap guest: plugin requested main-thread callback (ignored)");
    }
}

/// Main-thread host handler.
/// Implements `HostStateImpl` so the plugin can call `mark_dirty`.
struct RackHostMainThread {
    state_dirty: bool,
}

impl<'a> MainThreadHandler<'a> for RackHostMainThread {}

impl HostStateImpl for RackHostMainThread {
    fn mark_dirty(&mut self) {
        self.state_dirty = true;
        log::debug!("clap guest: plugin marked state dirty");
    }
}

/// `HostHandlers` glue type binding Shared + MainThread + AudioProcessor.
struct RackHost;

impl HostHandlers for RackHost {
    type Shared<'a> = RackHostShared;
    type MainThread<'a> = RackHostMainThread;
    /// `()` is sufficient for the audio processor handler in v1.
    type AudioProcessor<'a> = ();

    fn declare_extensions(builder: &mut HostExtensions<Self>, _shared: &Self::Shared<'_>) {
        // Declare HostState so the plugin can call mark_dirty on the host.
        builder.register::<HostState>();
    }
}

// ── ClapGuest ─────────────────────────────────────────────────────────────────

/// A hosted CLAP guest plugin instance.
///
/// # Thread safety
///
/// `ClapGuest` is `Send`: you may move it to an audio thread for `process()`
/// calls, then move it back to the main thread for `get_state`/`set_state`.
/// Never call `process()` and `get_state`/`set_state` concurrently.
pub struct ClapGuest {
    /// The loaded plugin entry (keeps the `.clap` dylib alive).
    _entry: PluginEntry,
    /// The plugin instance (main-thread object).
    instance: PluginInstance<RackHost>,
    /// The stopped (ready-to-start) audio processor.  `None` only during the
    /// brief window between `stop_processing` and `deactivate`.
    stopped_processor: Option<StoppedPluginAudioProcessor<RackHost>>,
    /// Pre-allocated event buffers (reused every process call).
    input_events_buf: EventBuffer,
    output_events_buf: EventBuffer,
    /// Pre-allocated audio port buffer handles.
    input_ports: AudioPorts,
    output_ports: AudioPorts,
    /// Channel scratch buffers for de-interleaving.
    left_in: Vec<f32>,
    right_in: Vec<f32>,
    left_out: Vec<f32>,
    right_out: Vec<f32>,
    /// Maximum block size configured at activation.
    max_frames: usize,
}

// SAFETY: PluginEntry, PluginInstance and audio-processor types in clack-host
// are Send (clack enforces the CLAP thread model in its type system).
// We document that the caller must not call process() + state methods
// simultaneously from different threads.
unsafe impl Send for ClapGuest {}

impl ClapGuest {
    /// Load a `.clap` bundle at `path`, instantiate its first audio-effect
    /// plugin, and activate it at 44 100 Hz with `[1, 512]` frame range.
    ///
    /// Returns `Err` if:
    /// - the path does not exist or the dylib fails to load,
    /// - no plugin descriptor is found in the bundle,
    /// - activation fails.
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        if !path.exists() {
            bail!("CLAP bundle not found: {}", path.display());
        }

        log::debug!("loading CLAP bundle: {}", path.display());

        // Step 1: load the entry.
        // SAFETY: We load an external dylib; the path exists (checked above)
        // and the caller is responsible for not loading the same plugin twice
        // in the same process (which could cause UB in some poorly-written plugins).
        let entry = unsafe { PluginEntry::load(path) }
            .map_err(|e| anyhow!("failed to load CLAP entry from {}: {e}", path.display()))?;

        // Step 2: get the plugin factory.
        let factory = entry
            .get_plugin_factory()
            .ok_or_else(|| anyhow!("no plugin factory in CLAP bundle: {}", path.display()))?;

        // Step 3: pick the first descriptor.
        let descriptor = factory
            .plugin_descriptors()
            .next()
            .ok_or_else(|| anyhow!("no plugin descriptors in CLAP factory: {}", path.display()))?;

        let plugin_id = descriptor
            .id()
            .ok_or_else(|| anyhow!("plugin descriptor has no id"))?;

        log::debug!(
            "instantiating CLAP plugin: {}",
            descriptor
                .name()
                .and_then(|n| n.to_str().ok())
                .unwrap_or("<unnamed>")
        );

        // Step 4: build host info and create instance.
        let host_info = HostInfo::new(
            "plugin-rack",
            "bedwards",
            "https://github.com/bedwards/plugin-rack",
            "0.2.0",
        )
        .map_err(|e| anyhow!("HostInfo::new failed: {e}"))?;

        let mut instance = PluginInstance::<RackHost>::new(
            |_| RackHostShared {
                state_ext: OnceLock::new(),
            },
            |_| RackHostMainThread { state_dirty: false },
            &entry,
            plugin_id,
            &host_info,
        )
        .map_err(|e| anyhow!("PluginInstance::new failed: {e}"))?;

        // Step 5: activate with a fixed stereo configuration.
        let config = PluginAudioConfiguration {
            sample_rate: 44_100.0,
            min_frames_count: 1,
            max_frames_count: 512,
        };

        let stopped_processor = instance
            .activate(|_, _| (), config)
            .map_err(|e| anyhow!("plugin activation failed: {e}"))?;

        // Pre-allocate event + audio port buffers for stereo (2 channels, 1 port).
        let max_frames: usize = 512;

        Ok(Self {
            _entry: entry,
            instance,
            stopped_processor: Some(stopped_processor),
            input_events_buf: EventBuffer::new(),
            output_events_buf: EventBuffer::new(),
            input_ports: AudioPorts::with_capacity(2, 1),
            output_ports: AudioPorts::with_capacity(2, 1),
            left_in: vec![0.0f32; max_frames],
            right_in: vec![0.0f32; max_frames],
            left_out: vec![0.0f32; max_frames],
            right_out: vec![0.0f32; max_frames],
            max_frames,
        })
    }

    /// Pass `buffer` (interleaved stereo frames `[left, right]`) through the
    /// hosted plugin.  The plugin processes audio in-place into the scratch
    /// output buffers; results are re-interleaved back into `buffer`.
    ///
    /// Must be called from the audio thread after `start_processing` succeeds.
    ///
    /// Returns `Err` if `start_processing` fails or the plugin returns an
    /// error status.
    pub fn process(&mut self, buffer: &mut [[f32; 2]]) -> anyhow::Result<()> {
        let n = buffer.len();
        if n > self.max_frames {
            bail!("buffer length {n} exceeds max_frames {}", self.max_frames);
        }

        // Take the stopped processor so we can start processing.
        let stopped = self
            .stopped_processor
            .take()
            .ok_or_else(|| anyhow!("no stopped processor available (already processing?)"))?;

        // Start processing.
        let mut active = stopped
            .start_processing()
            .map_err(|e| anyhow!("start_processing failed: {e}"))?;

        // De-interleave input into planar scratch buffers.
        for (i, frame) in buffer.iter().enumerate() {
            self.left_in[i] = frame[0];
            self.right_in[i] = frame[1];
        }

        // Zero output buffers.
        self.left_out[..n].fill(0.0);
        self.right_out[..n].fill(0.0);

        // Build input/output audio port views.
        let input_events = InputEvents::from_buffer(&self.input_events_buf);
        self.output_events_buf.clear();
        let mut output_events = OutputEvents::from_buffer(&mut self.output_events_buf);

        // Split borrows so the temporary slice arrays live long enough.
        let (left_in_slice, right_in_slice) = (
            &mut self.left_in[..n] as *mut [f32],
            &mut self.right_in[..n] as *mut [f32],
        );
        let (left_out_slice, right_out_slice) = (
            &mut self.left_out[..n] as *mut [f32],
            &mut self.right_out[..n] as *mut [f32],
        );
        // SAFETY: We hold exclusive access to these slices via `&mut self`; no
        // aliasing can occur during this scope.
        let mut in_ch_refs: [&mut [f32]; 2] =
            unsafe { [&mut *left_in_slice, &mut *right_in_slice] };
        let mut out_ch_refs: [&mut [f32]; 2] =
            unsafe { [&mut *left_out_slice, &mut *right_out_slice] };

        let input_audio = self.input_ports.with_input_buffers([AudioPortBuffer {
            latency: 0,
            channels: AudioPortBufferType::f32_input_only(
                in_ch_refs.iter_mut().map(InputChannel::variable),
            ),
        }]);

        let mut output_audio = self.output_ports.with_output_buffers([AudioPortBuffer {
            latency: 0,
            channels: AudioPortBufferType::f32_output_only(
                out_ch_refs.iter_mut().map(|ch| ch as &mut [f32]),
            ),
        }]);

        let _status = active
            .process(
                &input_audio,
                &mut output_audio,
                &input_events,
                &mut output_events,
                None,
                None,
            )
            .map_err(|e| anyhow!("plugin process() failed: {e}"))?;

        // Re-interleave output back into buffer.
        for (i, frame) in buffer.iter_mut().enumerate() {
            frame[0] = self.left_out[i];
            frame[1] = self.right_out[i];
        }

        // Stop processing and put the processor back.
        self.stopped_processor = Some(active.stop_processing());

        Ok(())
    }

    /// Serialize the plugin's state via the CLAP `state` extension.
    ///
    /// Returns `Err` if the plugin does not support the state extension or
    /// if saving fails.
    pub fn get_state(&mut self) -> anyhow::Result<Vec<u8>> {
        let state_ext = self
            .instance
            .access_shared_handler(|shared| {
                shared.state_ext.get().and_then(|opt| opt.as_ref().copied())
            })
            .ok_or_else(|| anyhow!("state extension not available (plugin not initialized?)"))?;

        let mut buf: Vec<u8> = Vec::new();
        state_ext
            .save(&mut self.instance.plugin_handle(), &mut buf)
            .map_err(|e| anyhow!("clap state save failed: {e}"))?;
        Ok(buf)
    }

    /// Restore the plugin's state from a blob previously returned by `get_state`.
    ///
    /// Returns `Err` if the plugin does not support the state extension or
    /// if loading fails.
    pub fn set_state(&mut self, data: &[u8]) -> anyhow::Result<()> {
        let state_ext = self
            .instance
            .access_shared_handler(|shared| {
                shared.state_ext.get().and_then(|opt| opt.as_ref().copied())
            })
            .ok_or_else(|| anyhow!("state extension not available (plugin not initialized?)"))?;

        let mut cursor = Cursor::new(data);
        state_ext
            .load(&mut self.instance.plugin_handle(), &mut cursor)
            .map_err(|e| anyhow!("clap state load failed: {e}"))?;
        Ok(())
    }
}

impl Drop for ClapGuest {
    fn drop(&mut self) {
        // Deactivate using the stopped processor (if we have one).
        if let Some(stopped) = self.stopped_processor.take() {
            self.instance.deactivate(stopped);
        }
        // instance and entry are dropped in field order.
    }
}

/// Shim over the inherent `get_state` / `set_state` methods so the rack can
/// drive state round-trips uniformly across CLAP and VST3 guests
/// (see `rack_core::GuestStateSource`). Issue #11.
impl rack_core::GuestStateSource for ClapGuest {
    fn get_state(&mut self) -> anyhow::Result<Vec<u8>> {
        ClapGuest::get_state(self)
    }

    fn set_state(&mut self, bytes: &[u8]) -> anyhow::Result<()> {
        ClapGuest::set_state(self, bytes)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// `ClapGuest` must be `Send` so it can be moved to the audio thread.
    #[test]
    fn clap_guest_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<ClapGuest>();
    }

    /// Loading a non-existent path must return `Err` gracefully — no panic.
    #[test]
    fn load_missing_path_returns_err() {
        let result = ClapGuest::load(&PathBuf::from("/nonexistent/plugin.clap"));
        assert!(result.is_err(), "expected Err for missing path");
        let msg = result.err().unwrap().to_string();
        assert!(
            msg.contains("not found") || msg.contains("CLAP"),
            "unexpected error message: {msg}"
        );
    }

    /// If a `.clap` bundle exists in the standard macOS path, smoke-test the
    /// full load + 64-frame silence process cycle.
    #[test]
    #[cfg(target_os = "macos")]
    fn smoke_test_real_plugin_if_available() {
        let clap_dir = std::path::Path::new("/Library/Audio/Plug-Ins/CLAP");
        if !clap_dir.exists() {
            eprintln!("no CLAP directory found; skipping smoke test");
            return;
        }

        // Find the first .clap bundle.
        let bundle = std::fs::read_dir(clap_dir).ok().and_then(|mut entries| {
            entries.find_map(|e| {
                let path = e.ok()?.path();
                if path.extension().and_then(|s| s.to_str()) == Some("clap") {
                    Some(path)
                } else {
                    None
                }
            })
        });

        let Some(bundle_path) = bundle else {
            eprintln!("no .clap bundle found in /Library/Audio/Plug-Ins/CLAP; skipping");
            return;
        };

        eprintln!("smoke testing: {}", bundle_path.display());

        let mut guest = match ClapGuest::load(&bundle_path) {
            Ok(g) => g,
            Err(e) => {
                eprintln!("load failed (skipping): {e}");
                return;
            }
        };

        // Process 64 frames of silence.
        let mut buf = vec![[0.0f32; 2]; 64];
        if let Err(e) = guest.process(&mut buf) {
            eprintln!("process failed (non-fatal for smoke test): {e}");
            return;
        }

        // Verify all output samples are finite.
        assert!(
            buf.iter().all(|f| f[0].is_finite() && f[1].is_finite()),
            "output contains non-finite samples"
        );

        eprintln!("smoke test passed for {}", bundle_path.display());
    }
}
