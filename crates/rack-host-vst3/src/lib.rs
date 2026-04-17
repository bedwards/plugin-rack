//! VST3 guest plugin hosting for plugin-rack.
//!
//! Loads a `.vst3` bundle, drives the IComponent/IAudioProcessor lifecycle,
//! passes stereo audio through unchanged, and round-trips state via
//! `IComponent::getState` / `setState`.
//!
//! Platform support in this file: macOS. Windows / Linux stubs are present
//! but untested; the module-binary resolution and entry/exit hooks differ.

// The vst3 crate exposes unsafe COM vtable calls everywhere; we wrap them in
// safe-ish public methods and document the remaining invariants.
#![allow(unsafe_op_in_unsafe_fn)]

use std::ffi::c_void;
use std::path::Path;

use anyhow::{anyhow, bail, Context};
use vst3::ComPtr;
use vst3::Interface;
use vst3::Steinberg::Vst::{
    AudioBusBuffers, AudioBusBuffers__type0, BusDirections_, IAudioProcessor,
    IAudioProcessorTrait, IComponent, IComponentTrait, MediaTypes_, ProcessData, ProcessModes_,
    ProcessSetup, SpeakerArr, SymbolicSampleSizes_,
};
use vst3::Steinberg::{
    IBStream, IPluginBaseTrait, IPluginFactory, IPluginFactoryTrait, PClassInfo,
    kInvalidArgument, kNoInterface, kResultOk,
};

// ── IBStream implementation ──────────────────────────────────────────────────

/// Static vtable for MemStream.  Lives for the duration of the program.
static MEM_STREAM_VTBL: vst3::Steinberg::IBStreamVtbl = vst3::Steinberg::IBStreamVtbl {
    base: vst3::Steinberg::FUnknownVtbl {
        queryInterface: mem_stream_query_interface,
        addRef: mem_stream_add_ref,
        release: mem_stream_release,
    },
    read: mem_stream_read,
    write: mem_stream_write,
    seek: mem_stream_seek,
    tell: mem_stream_tell,
};

/// A simple `Vec<u8>`-backed [`IBStream`] implementation for state serialization.
///
/// Layout: the first field is the `IBStream` ABI header (a `*const IBStreamVtbl`).
/// This matches the C layout, so casting `*mut MemStream` to `*mut IBStream` is
/// valid as long as the pointer is non-null and properly aligned.
#[repr(C)]
struct MemStream {
    /// Must be the first field — VST3 COM reads `(*this).vtbl` to dispatch.
    header: IBStream,
    data: Vec<u8>,
    pos: usize,
    /// COM ref count. We own the Box so we don't actually free on release=0,
    /// but the field is required for ABI compliance.
    refs: std::sync::atomic::AtomicU32,
}

// SAFETY: MemStream is only accessed from a single thread during getState/setState calls.
unsafe impl Send for MemStream {}
unsafe impl Sync for MemStream {}

impl MemStream {
    fn new(initial: Vec<u8>) -> Box<Self> {
        Box::new(Self {
            header: IBStream {
                vtbl: &raw const MEM_STREAM_VTBL,
            },
            data: initial,
            pos: 0,
            refs: std::sync::atomic::AtomicU32::new(1),
        })
    }

    /// Return a raw `*mut IBStream` pointing to the header field.
    ///
    /// The pointer is valid as long as this Box is alive.
    fn as_ibstream_ptr(&mut self) -> *mut IBStream {
        &mut self.header as *mut IBStream
    }
}

/// Recover `*mut MemStream` from an `*mut IBStream` received in a COM callback.
///
/// Since `header: IBStream` is at offset 0 (repr(C)), the two pointers are
/// numerically identical.
unsafe fn mem_stream_from_ibstream(this: *mut IBStream) -> *mut MemStream {
    this as *mut MemStream
}

unsafe extern "system" fn mem_stream_query_interface(
    _this: *mut vst3::Steinberg::FUnknown,
    _iid: *const vst3::Steinberg::TUID,
    obj: *mut *mut c_void,
) -> vst3::Steinberg::tresult {
    // Minimal: only supports IBStream (no extra QI).
    *obj = std::ptr::null_mut();
    kNoInterface
}

unsafe extern "system" fn mem_stream_add_ref(
    this: *mut vst3::Steinberg::FUnknown,
) -> vst3::Steinberg::uint32 {
    let stream = this as *mut MemStream;
    (*stream)
        .refs
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        + 1
}

unsafe extern "system" fn mem_stream_release(
    this: *mut vst3::Steinberg::FUnknown,
) -> vst3::Steinberg::uint32 {
    let stream = this as *mut MemStream;
    let prev = (*stream)
        .refs
        .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
    // We own the MemStream via Box; we never drop from inside the plugin's
    // release call. The Box drop handles cleanup after the call returns.
    prev - 1
}

unsafe extern "system" fn mem_stream_read(
    this: *mut IBStream,
    buffer: *mut c_void,
    num_bytes: vst3::Steinberg::int32,
    num_bytes_read: *mut vst3::Steinberg::int32,
) -> vst3::Steinberg::tresult {
    let stream = mem_stream_from_ibstream(this);
    let available = (*stream).data.len().saturating_sub((*stream).pos);
    let to_read = (num_bytes as usize).min(available);
    std::ptr::copy_nonoverlapping(
        (*stream).data.as_ptr().add((*stream).pos),
        buffer as *mut u8,
        to_read,
    );
    (*stream).pos += to_read;
    if !num_bytes_read.is_null() {
        *num_bytes_read = to_read as vst3::Steinberg::int32;
    }
    kResultOk
}

unsafe extern "system" fn mem_stream_write(
    this: *mut IBStream,
    buffer: *mut c_void,
    num_bytes: vst3::Steinberg::int32,
    num_bytes_written: *mut vst3::Steinberg::int32,
) -> vst3::Steinberg::tresult {
    let stream = mem_stream_from_ibstream(this);
    let bytes = std::slice::from_raw_parts(buffer as *const u8, num_bytes as usize);
    (*stream).data.extend_from_slice(bytes);
    (*stream).pos += num_bytes as usize;
    if !num_bytes_written.is_null() {
        *num_bytes_written = num_bytes;
    }
    kResultOk
}

unsafe extern "system" fn mem_stream_seek(
    this: *mut IBStream,
    pos: vst3::Steinberg::int64,
    mode: vst3::Steinberg::int32,
    result: *mut vst3::Steinberg::int64,
) -> vst3::Steinberg::tresult {
    let stream = mem_stream_from_ibstream(this);
    // IBStream seek modes: 0=set, 1=cur, 2=end
    let new_pos: i64 = match mode {
        0 => pos,
        1 => (*stream).pos as i64 + pos,
        2 => (*stream).data.len() as i64 + pos,
        _ => return kInvalidArgument,
    };
    if new_pos < 0 {
        return kInvalidArgument;
    }
    (*stream).pos = new_pos as usize;
    if !result.is_null() {
        *result = new_pos;
    }
    kResultOk
}

unsafe extern "system" fn mem_stream_tell(
    this: *mut IBStream,
    pos: *mut vst3::Steinberg::int64,
) -> vst3::Steinberg::tresult {
    let stream = mem_stream_from_ibstream(this);
    if !pos.is_null() {
        *pos = (*stream).pos as vst3::Steinberg::int64;
    }
    kResultOk
}

// ── Platform module-binary resolution ────────────────────────────────────────

/// Resolve the platform-specific binary path inside a `.vst3` bundle directory.
fn resolve_module_binary(bundle: &Path) -> anyhow::Result<std::path::PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let name = bundle
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow!("bundle has no file stem: {}", bundle.display()))?;
        let p = bundle.join("Contents").join("MacOS").join(name);
        if p.exists() {
            return Ok(p);
        }
        bail!(
            "macOS binary not found at expected path: {}",
            p.display()
        );
    }
    #[cfg(target_os = "windows")]
    {
        let name = bundle
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow!("bundle has no file name: {}", bundle.display()))?;
        for subdir in ["x86_64-win", "arm64ec-win", "arm64-win"] {
            let p = bundle.join("Contents").join(subdir).join(name);
            if p.exists() {
                return Ok(p);
            }
        }
        bail!("Windows binary not found inside: {}", bundle.display());
    }
    #[cfg(target_os = "linux")]
    {
        let stem = bundle
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow!("bundle has no file stem: {}", bundle.display()))?;
        let so = format!("{stem}.so");
        for subdir in ["x86_64-linux", "aarch64-linux"] {
            let p = bundle.join("Contents").join(subdir).join(&so);
            if p.exists() {
                return Ok(p);
            }
        }
        bail!("Linux binary not found inside: {}", bundle.display());
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    bail!("unsupported platform for VST3 hosting");
}

// ── Module loader ─────────────────────────────────────────────────────────────

type GetPluginFactoryFn = unsafe extern "system" fn() -> *mut IPluginFactory;

#[cfg(target_os = "macos")]
type BundleEntryFn = unsafe extern "system" fn(*mut c_void) -> bool;
#[cfg(target_os = "macos")]
type BundleExitFn = unsafe extern "system" fn() -> bool;

#[cfg(target_os = "windows")]
type InitDllFn = unsafe extern "system" fn() -> bool;
#[cfg(target_os = "windows")]
type ExitDllFn = unsafe extern "system" fn() -> bool;

#[cfg(target_os = "linux")]
type ModuleEntryFn = unsafe extern "system" fn(*mut c_void) -> bool;
#[cfg(target_os = "linux")]
type ModuleExitFn = unsafe extern "system" fn() -> bool;

/// Loaded dynamic library + factory handle. Drop order matters: factory must
/// be released before the library is unloaded.
struct VstModule {
    factory: ComPtr<IPluginFactory>,
    /// The library must be kept alive for as long as `factory` is alive.
    _lib: libloading::Library,
    /// Platform exit hook — called when VstModule is dropped.
    exit: Option<ExitHook>,
}

enum ExitHook {
    #[cfg(target_os = "macos")]
    Bundle(BundleExitFn),
    #[cfg(target_os = "windows")]
    Dll(ExitDllFn),
    #[cfg(target_os = "linux")]
    Module(ModuleExitFn),
}

impl Drop for VstModule {
    fn drop(&mut self) {
        // factory Drop releases its refcount first (via field ordering),
        // then we call the exit hook, then the library is unloaded.
        if let Some(hook) = self.exit.take() {
            unsafe {
                match hook {
                    #[cfg(target_os = "macos")]
                    ExitHook::Bundle(f) => {
                        f();
                    }
                    #[cfg(target_os = "windows")]
                    ExitHook::Dll(f) => {
                        f();
                    }
                    #[cfg(target_os = "linux")]
                    ExitHook::Module(f) => {
                        f();
                    }
                }
            }
        }
    }
}

/// Load a VST3 module binary, call the platform entry hook, and return the
/// `IPluginFactory`.
///
/// # Safety
///
/// The returned `VstModule` must be kept alive for as long as any COM objects
/// obtained from the factory are in use. Dropping `VstModule` calls the exit
/// hook and unloads the library.
unsafe fn load_vst3_module(binary: &Path) -> anyhow::Result<VstModule> {
    let lib = libloading::Library::new(binary)
        .with_context(|| format!("dlopen failed: {}", binary.display()))?;

    // Platform entry hook
    #[cfg(target_os = "macos")]
    let exit = {
        let entry: libloading::Symbol<BundleEntryFn> = lib
            .get(b"bundleEntry\0")
            .context("bundleEntry symbol not found")?;
        if !entry(std::ptr::null_mut()) {
            bail!("bundleEntry() returned false");
        }
        let exit_sym: libloading::Symbol<BundleExitFn> = lib
            .get(b"bundleExit\0")
            .context("bundleExit symbol not found")?;
        Some(ExitHook::Bundle(*exit_sym))
    };

    #[cfg(target_os = "windows")]
    let exit = {
        if let Ok(init) = lib.get::<InitDllFn>(b"InitDll\0") {
            init();
        }
        lib.get::<ExitDllFn>(b"ExitDll\0")
            .ok()
            .map(|s| ExitHook::Dll(*s))
    };

    #[cfg(target_os = "linux")]
    let exit = {
        if let Ok(entry) = lib.get::<ModuleEntryFn>(b"ModuleEntry\0") {
            if !entry(std::ptr::null_mut()) {
                bail!("ModuleEntry() returned false");
            }
        }
        lib.get::<ModuleExitFn>(b"ModuleExit\0")
            .ok()
            .map(|s| ExitHook::Module(*s))
    };

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    let exit: Option<ExitHook> = None;

    let get_factory: libloading::Symbol<GetPluginFactoryFn> = lib
        .get(b"GetPluginFactory\0")
        .context("GetPluginFactory symbol not found")?;
    let raw = get_factory();
    let factory = ComPtr::from_raw(raw)
        .ok_or_else(|| anyhow!("GetPluginFactory returned null"))?;

    Ok(VstModule {
        factory,
        _lib: lib,
        exit,
    })
}

// ── Vst3Guest ─────────────────────────────────────────────────────────────────

/// A hosted VST3 guest plugin instance.
///
/// Lifecycle:
/// ```text
/// Vst3Guest::load(path)   → load + init
/// guest.process(...)      → audio passthrough (call repeatedly on audio thread)
/// drop(guest)             → setProcessing(false) → setActive(false) → terminate → unload
/// ```
///
/// Public API is intentionally minimal for v1: `load` + `process` + state round-trip.
pub struct Vst3Guest {
    component: ComPtr<IComponent>,
    processor: ComPtr<IAudioProcessor>,
    /// Pre-allocated scratch buffers for the ProcessData, held for the lifetime
    /// of the active processing session. Indexed as [bus][channel].
    ///
    /// For v1 we only deal with one stereo input bus and one stereo output bus.
    _input_ptrs: Vec<*mut f32>,
    _output_ptrs: Vec<*mut f32>,
    /// AudioBusBuffers structs passed into ProcessData each block.
    input_bus: AudioBusBuffers,
    output_bus: AudioBusBuffers,
    /// Sample rate and block size remembered for ProcessSetup (used to report
    /// configuration; not called again after activation in v1).
    #[allow(dead_code)]
    sample_rate: f64,
    max_block_size: usize,
    /// The loaded module — must outlive all COM objects.
    _module: VstModule,
}

// SAFETY: Vst3Guest is used from a single thread (audio thread for process(),
// main thread for load/state). The user of this API is responsible for not
// calling process() from multiple threads simultaneously. The COM objects
// (IComponent, IAudioProcessor) are Send + Sync per their bindings.
unsafe impl Send for Vst3Guest {}

impl Vst3Guest {
    /// Load a `.vst3` bundle at `path`, initialize the first audio-processor
    /// class found, configure stereo I/O buses, and call `setupProcessing`.
    ///
    /// Returns `Err` if the bundle doesn't exist, the binary can't be loaded,
    /// no audio-processor class is found, or any mandatory init step fails.
    pub fn load(path: &Path, sample_rate: f64, max_block_size: usize) -> anyhow::Result<Self> {
        if !path.exists() {
            bail!("VST3 bundle not found: {}", path.display());
        }

        let binary = resolve_module_binary(path)?;
        log::debug!("loading VST3 binary: {}", binary.display());

        // SAFETY: we keep _module alive for the full lifetime of Vst3Guest.
        let module = unsafe { load_vst3_module(&binary)? };

        // Find the first audio-processor class.
        let cid = unsafe { find_audio_processor_cid(&module.factory)? };
        log::debug!("using CID: {:?}", cid);

        // Create IComponent instance.
        let component = unsafe { create_component(&module.factory, &cid)? };

        // Cast to IAudioProcessor (same object, different interface).
        let processor = component
            .cast::<IAudioProcessor>()
            .ok_or_else(|| anyhow!("plugin does not implement IAudioProcessor"))?;

        // Initialize.
        let init_result = unsafe { component.initialize(std::ptr::null_mut()) };
        if init_result != kResultOk {
            bail!("IComponent::initialize failed: {init_result}");
        }

        // Activate stereo buses.
        unsafe { activate_stereo_buses(&component)? };

        // Set bus arrangements (stereo in, stereo out).
        let mut stereo = SpeakerArr::kStereo;
        let arr_result = unsafe {
            processor.setBusArrangements(
                &mut stereo,
                1,
                &mut stereo,
                1,
            )
        };
        if arr_result != kResultOk {
            log::warn!("setBusArrangements returned {arr_result} (non-fatal, continuing)");
        }

        // setupProcessing.
        let mut setup = ProcessSetup {
            processMode: ProcessModes_::kRealtime as i32,
            symbolicSampleSize: SymbolicSampleSizes_::kSample32 as i32,
            maxSamplesPerBlock: max_block_size as i32,
            sampleRate: sample_rate,
        };
        let setup_result = unsafe { processor.setupProcessing(&mut setup) };
        if setup_result != kResultOk {
            bail!("setupProcessing failed: {setup_result}");
        }

        // Activate the component.
        let active_result = unsafe { component.setActive(1) };
        if active_result != kResultOk {
            bail!("setActive(true) failed: {active_result}");
        }

        // Start processing.
        let proc_result = unsafe { processor.setProcessing(1) };
        if proc_result != kResultOk {
            bail!("setProcessing(true) failed: {proc_result}");
        }

        // Pre-allocate channel pointer arrays (2 channels each for stereo).
        // The actual float pointers are filled in per-call in process().
        let input_ptrs = vec![std::ptr::null_mut::<f32>(); 2];
        let output_ptrs = vec![std::ptr::null_mut::<f32>(); 2];

        let input_bus = AudioBusBuffers {
            numChannels: 2,
            silenceFlags: 0,
            __field0: AudioBusBuffers__type0 {
                channelBuffers32: std::ptr::null_mut(),
            },
        };
        let output_bus = AudioBusBuffers {
            numChannels: 2,
            silenceFlags: 0,
            __field0: AudioBusBuffers__type0 {
                channelBuffers32: std::ptr::null_mut(),
            },
        };

        Ok(Self {
            component,
            processor,
            _input_ptrs: input_ptrs,
            _output_ptrs: output_ptrs,
            input_bus,
            output_bus,
            sample_rate,
            max_block_size,
            _module: module,
        })
    }

    /// Pass `buffer` through the hosted plugin.
    ///
    /// `buffer` is a slice of stereo frames: each element is `[left, right]`.
    /// The plugin processes the audio in-place (or as passthrough for a
    /// transparent plugin).
    ///
    /// # Panics
    ///
    /// Panics if `buffer.len() > max_block_size` passed to `load`.
    pub fn process(&mut self, buffer: &mut [[f32; 2]]) {
        assert!(
            buffer.len() <= self.max_block_size,
            "buffer size {} exceeds max_block_size {}",
            buffer.len(),
            self.max_block_size
        );

        let num_samples = buffer.len();

        // De-interleave: build separate channel slices pointing into buffer.
        // We avoid heap allocation by using temporary stack arrays of pointers.
        // For stereo we need exactly 2 channel pointers in / 2 out.
        //
        // VST3 expects planar (non-interleaved) buffers. We de-interleave
        // into two temporary vecs, call process(), then re-interleave.
        let mut left_in: Vec<f32> = buffer.iter().map(|f| f[0]).collect();
        let mut right_in: Vec<f32> = buffer.iter().map(|f| f[1]).collect();
        let mut left_out = vec![0.0f32; num_samples];
        let mut right_out = vec![0.0f32; num_samples];

        // Channel pointer arrays on the stack (VST3 expects *mut *mut f32).
        let mut in_ptrs: [*mut f32; 2] = [left_in.as_mut_ptr(), right_in.as_mut_ptr()];
        let mut out_ptrs: [*mut f32; 2] = [left_out.as_mut_ptr(), right_out.as_mut_ptr()];

        self.input_bus.__field0.channelBuffers32 = in_ptrs.as_mut_ptr();
        self.output_bus.__field0.channelBuffers32 = out_ptrs.as_mut_ptr();

        let mut data = ProcessData {
            processMode: ProcessModes_::kRealtime as i32,
            symbolicSampleSize: SymbolicSampleSizes_::kSample32 as i32,
            numSamples: num_samples as i32,
            numInputs: 1,
            numOutputs: 1,
            inputs: &mut self.input_bus as *mut AudioBusBuffers,
            outputs: &mut self.output_bus as *mut AudioBusBuffers,
            inputParameterChanges: std::ptr::null_mut(),
            outputParameterChanges: std::ptr::null_mut(),
            inputEvents: std::ptr::null_mut(),
            outputEvents: std::ptr::null_mut(),
            processContext: std::ptr::null_mut(),
        };

        let result = unsafe { self.processor.process(&mut data) };
        if result != kResultOk {
            log::warn!("IAudioProcessor::process returned {result}");
        }

        // Re-interleave output back into buffer.
        for (i, frame) in buffer.iter_mut().enumerate() {
            frame[0] = left_out[i];
            frame[1] = right_out[i];
        }
    }

    /// Serialize the plugin's DSP state via `IComponent::getState`.
    ///
    /// Returns the raw state blob as a `Vec<u8>`. Pass back to `set_state` to
    /// restore.
    pub fn get_state(&mut self) -> anyhow::Result<Vec<u8>> {
        let mut stream = MemStream::new(Vec::new());
        let result = unsafe { self.component.getState(stream.as_ibstream_ptr()) };
        if result != kResultOk {
            bail!("IComponent::getState failed: {result}");
        }
        Ok(stream.data)
    }

    /// Restore the plugin's DSP state from a blob previously returned by
    /// `get_state`.
    pub fn set_state(&mut self, blob: &[u8]) -> anyhow::Result<()> {
        let mut stream = MemStream::new(blob.to_vec());
        let result = unsafe { self.component.setState(stream.as_ibstream_ptr()) };
        if result != kResultOk {
            bail!("IComponent::setState failed: {result}");
        }
        Ok(())
    }
}

impl Drop for Vst3Guest {
    fn drop(&mut self) {
        unsafe {
            let _ = self.processor.setProcessing(0);
            let _ = self.component.setActive(0);
            let _ = self.component.terminate();
        }
        // component, processor, and _module are dropped in field order.
    }
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Enumerate factory classes and return the TUID of the first
/// `kVstAudioEffectClass` (category "Audio Module Class").
unsafe fn find_audio_processor_cid(
    factory: &ComPtr<IPluginFactory>,
) -> anyhow::Result<vst3::Steinberg::TUID> {
    let count = factory.countClasses();
    log::debug!("factory has {count} class(es)");
    for i in 0..count {
        let mut info: PClassInfo = std::mem::zeroed();
        if factory.getClassInfo(i, &mut info) != kResultOk {
            continue;
        }
        // category is a null-terminated ASCII string: "Audio Module Class"
        let cat = std::ffi::CStr::from_ptr(info.category.as_ptr())
            .to_string_lossy();
        log::debug!("class {i}: category=\"{cat}\"");
        if cat == "Audio Module Class" {
            return Ok(info.cid);
        }
    }
    bail!("no 'Audio Module Class' found in plugin factory")
}

/// Create an `IComponent` instance from the factory using the given CID.
unsafe fn create_component(
    factory: &ComPtr<IPluginFactory>,
    cid: &vst3::Steinberg::TUID,
) -> anyhow::Result<ComPtr<IComponent>> {
    // createInstance takes FIDString (*const char8 = *const i8) for both cid and iid.
    // TUID is [i8; 16]; Guid is [u8; 16]. We need the raw byte pointer cast to *const i8.
    let cid_ptr = cid.as_ptr();
    let iid = <IComponent as Interface>::IID;
    let iid_ptr = iid.as_ptr() as *const i8;
    let mut raw: *mut IComponent = std::ptr::null_mut();
    let result = factory.createInstance(
        cid_ptr,
        iid_ptr,
        &mut raw as *mut *mut IComponent as *mut *mut c_void,
    );
    if result != kResultOk {
        bail!("IPluginFactory::createInstance failed: {result}");
    }
    ComPtr::from_raw(raw).ok_or_else(|| anyhow!("createInstance returned null for IComponent"))
}

/// Activate the main stereo audio input and output buses on an IComponent.
unsafe fn activate_stereo_buses(component: &ComPtr<IComponent>) -> anyhow::Result<()> {
    let media_audio = MediaTypes_::kAudio as i32;
    let dir_input = BusDirections_::kInput as i32;
    let dir_output = BusDirections_::kOutput as i32;

    let num_in = component.getBusCount(media_audio, dir_input);
    let num_out = component.getBusCount(media_audio, dir_output);
    log::debug!("audio buses: {num_in} input(s), {num_out} output(s)");

    // Activate bus index 0 for input and output (the main stereo bus).
    if num_in > 0 {
        let r = component.activateBus(media_audio, dir_input, 0, 1);
        if r != kResultOk {
            log::warn!("activateBus(input, 0) returned {r}");
        }
    }
    if num_out > 0 {
        let r = component.activateBus(media_audio, dir_output, 0, 1);
        if r != kResultOk {
            log::warn!("activateBus(output, 0) returned {r}");
        }
    }
    Ok(())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Vst3Guest must be Send so it can be moved to the audio thread.
    #[test]
    fn vst3_guest_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<Vst3Guest>();
    }

    /// MemStream round-trip: write bytes then read them back.
    #[test]
    fn mem_stream_write_read_roundtrip() {
        let mut stream = MemStream::new(Vec::new());
        let data = b"hello vst3";
        let ibstream = stream.as_ibstream_ptr();

        let mut written = 0i32;
        unsafe {
            let r = mem_stream_write(
                ibstream,
                data.as_ptr() as *mut c_void,
                data.len() as i32,
                &mut written,
            );
            assert_eq!(r, kResultOk);
            assert_eq!(written, data.len() as i32);
        }

        // Rewind.
        unsafe {
            let r = mem_stream_seek(ibstream, 0, 0, std::ptr::null_mut());
            assert_eq!(r, kResultOk);
        }

        let mut buf = vec![0u8; data.len()];
        let mut read = 0i32;
        unsafe {
            let r = mem_stream_read(
                ibstream,
                buf.as_mut_ptr() as *mut c_void,
                buf.len() as i32,
                &mut read,
            );
            assert_eq!(r, kResultOk);
            assert_eq!(read, data.len() as i32);
        }
        assert_eq!(&buf, data);
    }

    /// Loading a non-existent path must return Err gracefully — no panic.
    #[test]
    fn load_missing_path_returns_err() {
        let result = Vst3Guest::load(
            &PathBuf::from("/nonexistent/plugin.vst3"),
            44100.0,
            512,
        );
        assert!(result.is_err(), "expected Err for missing path");
    }

    /// If a known test plugin is available in the standard macOS VST3 location,
    /// exercise the full lifecycle. Skipped if no plugin is present.
    #[test]
    #[cfg(target_os = "macos")]
    fn load_real_plugin_if_available() {
        // Try a commonly installed free plugin. Adjust path as needed.
        let candidates = [
            "/Library/Audio/Plug-Ins/VST3/Surge XT.vst3",
            "/Library/Audio/Plug-Ins/VST3/Vital.vst3",
            "~/Library/Audio/Plug-Ins/VST3/Surge XT.vst3",
        ];

        let path = candidates
            .iter()
            .map(|&s| PathBuf::from(s))
            .find(|p| p.exists());

        let Some(path) = path else {
            eprintln!("no test plugin found; skipping real-plugin test");
            return;
        };

        let mut guest = Vst3Guest::load(&path, 44100.0, 512)
            .expect("failed to load real plugin");

        // State round-trip.
        let state = guest.get_state().expect("getState failed");
        eprintln!("state blob: {} bytes", state.len());
        guest.set_state(&state).expect("setState failed");

        // Audio passthrough — plugin should not crash.
        let mut buf = vec![[0.0f32; 2]; 512];
        guest.process(&mut buf);
        // A pure passthrough or silent plugin: output may be silence or
        // unchanged. Just verify no panic and results are finite.
        assert!(buf.iter().all(|f| f[0].is_finite() && f[1].is_finite()));
    }
}
