# nih_plug and Rust VST3/CLAP Ecosystem — State of the Art (April 2026)

Research date: 2026-04-16. Focus: evaluating `nih_plug` and the surrounding
Rust audio-plugin ecosystem for building a plugin rack (a plugin that hosts
other plugins inside itself).

---

## TL;DR

- The original [`robbert-vdh/nih-plug`](https://github.com/robbert-vdh/nih-plug)
  is effectively **no longer maintained**. The accepted continuation is
  [`BillyDM/nih-plug`](https://github.com/BillyDM/nih-plug), announced as a
  hard fork on 2026-03-29 via
  [issue #265](https://github.com/robbert-vdh/nih-plug/issues/265).
- `nih_plug` is a **plugin framework only, not a host**. A rack must pair it
  with an external hosting library.
- The mature CLAP hosting library in Rust is
  [`prokopyl/clack`](https://github.com/prokopyl/clack) (`clack-host` crate).
  VST3 hosting from Rust is still rough — the most promising path is the new
  permissively-licensed
  [`coupler-rs/vst3-rs`](https://github.com/coupler-rs/vst3-rs) (`vst3` crate
  v0.3.0) plus hand-rolled glue, or the early-stage
  [`sinkingsugar/rack`](https://github.com/sinkingsugar/rack) crate.
- Dynamic parameter addition/removal (needed if nested plugins expose their
  own params through the rack) is **not supported** by `nih_plug`'s
  `Params` model, which is fundamentally static-derive-based.
- GUI: **`nih_plug_iced`** and **`nih_plug_vizia`** are the recommended
  options; `nih_plug_egui` is explicitly deprecated in its own README.
- AU (Audio Unit) is **not supported** natively by `nih_plug`. Ship AU via
  [`clap-wrapper-rs`](https://github.com/blepfx/clap-wrapper-rs) wrapping a
  CLAP build.

---

## 1. Framework Status, Forks, Version

### Original repository

- URL: https://github.com/robbert-vdh/nih-plug
- Stars: ~2.8k, Forks: ~276, Open issues: ~90, Open PRs: ~26 (April 2026).
- Commits on `master`: ~2,249.
- Published crate version: **0.0.0** on the docs site
  ([nih-plug.robbertvanderhelm.nl](https://nih-plug.robbertvanderhelm.nl/nih_plug/)).
  The project has never been cut as a semver release — it is consumed via
  git-dependency only.
- Latest visible commits touch buffer-code autoref cleanup and "BYO GUI"
  example mentions (commits `ecfd632`, `d64b2ab`). Automated builds ran as
  recently as 2026-01-16
  ([actions run 21066550536](https://github.com/robbert-vdh/nih-plug/actions/runs/21066550536)).
- `CHANGELOG.md` has a single 2025 entry (2025-02-23) and ends with
  "Who knows what happened at this point!" — signalling the changelog
  is abandoned
  ([CHANGELOG.md](https://github.com/robbert-vdh/nih-plug/blob/master/CHANGELOG.md)).

### The maintained fork — BillyDM/nih-plug

- URL: https://github.com/BillyDM/nih-plug (mirror:
  https://codeberg.org/BillyDM/nih-plug)
- Hard-forked 2026-03-29, announced publicly in
  [issue #265](https://github.com/robbert-vdh/nih-plug/issues/265).
- BillyDM is a well-known figure in Rust audio: creator of
  [Meadowlark DAW](https://github.com/MeadowlarkDAW),
  [egui-baseview](https://github.com/BillyDM/egui-baseview),
  [iced_baseview](https://github.com/BillyDM/iced_baseview), and the
  [awesome-audio-dsp](https://github.com/BillyDM/awesome-audio-dsp) list.
- README statement: *"This is a hard fork of
  https://github.com/robbert-vdh/nih-plug, since the original author is no
  longer maintaining it. This fork does NOT contain the original collection
  of plugins."*
- Scope: framework only; Crisp, Diopser, Spectral Compressor, etc. are gone.
  Filed plugin-specific issues should go to the upstream repo.
- ~2,279 commits on `main`. No tagged releases. License structure (from
  the fork README):
  - Framework + examples: **ISC**
  - Baseview adapters: **MIT OR Apache-2.0**
  - VST3 bindings (via `vst3-sys`): **GPLv3** (see §2 for migration path
    to the permissive `vst3` crate)

### Recommendation

For a new plugin-rack project in 2026, **track `BillyDM/nih-plug`**. Pin to
a commit hash (the project has no releases) and plan to re-pin periodically.
Watch upstream `robbert-vdh/nih-plug` only for plugin bug reports and test
fixtures.

Key outstanding PRs worth porting / watching:

- [PR #263 — Finish switching from `vst3-sys` to `vst3`](https://github.com/robbert-vdh/nih-plug/pull/263)
  (March 2026). Drops GPLv3 licensing burden; migrates to the new
  permissively-licensed `vst3` crate. Linux/Windows fixes + nightly SIMD
  breakage fixes included.
- [PR #260 — Track context info (`TrackInfo` API)](https://github.com/robbert-vdh/nih-plug/pull/260)
  (March 2026). Adds `context.track_info()` for name/color/channels/type
  (Regular/Return/Bus/Master). CLAP via `track-info` extension,
  VST3 via `IInfoListener`. Useful for a rack to show the host track name.
- [PR #225 — Fix reference cycles in VST3/CLAP wrappers](https://github.com/robbert-vdh/nih-plug/pull/225)
  (July 2025). Memory-leak fix.
- [PR #170 — Update Iced to 0.13](https://github.com/robbert-vdh/nih-plug/pull/170).

---

## 2. Plugin Formats Supported

`nih_plug` exports:

| Format | Status | Notes |
|---|---|---|
| **VST3** | Supported | Wrapper implements `IComponent`, `IEditController`, `IAudioProcessor`, `IMidiMapping`, `INoteExpressionController`, `IProcessContextRequirements`, `IUnitInfo` ([wrapper.rs](https://github.com/robbert-vdh/nih-plug/blob/master/src/wrapper/vst3/wrapper.rs)). |
| **CLAP** | Supported, maturity class-leading | Sample-accurate automation, poly modulation, note expression, remote-control pages, all via `clap-sys` v1.2.2. |
| **Standalone** | Supported | `nih_export_standalone()` with CPAL/JACK audio + `midir` MIDI. |
| **AU (AUv2)** | **Not supported** | Must wrap CLAP build via `clap-wrapper-rs` (see §4). |
| **AUv3 / AAX / LV2** | Not supported | No known plans. |

### VST3 SDK and licensing transition

Historically, `nih_plug`'s VST3 path went through
[`RustAudio/vst3-sys`](https://github.com/RustAudio/vst3-sys), which is
GPLv3 — meaning any VST3 plugin you built inherited GPLv3. In October 2024,
Steinberg released VST3 SDK 3.8.0 under the **MIT license**
([background blog post](https://micahrj.github.io/posts/vst3/)). This
unblocked a new, permissive bindings crate:

- **Crate**: [`vst3`](https://lib.rs/crates/vst3) on crates.io
- **Repo**: [coupler-rs/vst3-rs](https://github.com/coupler-rs/vst3-rs)
- **Latest version**: `0.3.0` (2025-12-07)
- **License**: MIT OR Apache-2.0
- **Approach**: `com-scrape` tool parses VST3 C++ headers and generates
  pre-built Rust bindings. Binding output is committed to the crate so
  consumers no longer need libclang or a local SDK checkout. Build-time is
  dramatically faster than `vst3-sys`.
- **Abstractions**: `ComPtr`/`ComRef` smart pointers, `Class` trait +
  `ComWrapper` for implementing COM interfaces from Rust.
- **SDK version**: The generated bindings correspond to VST3 3.8.x (MIT);
  the exact version isn't published in the crate metadata but the
  `vst3sdk/` submodule in the repo pins it.

PR #263 on `nih-plug` swaps the wrapper from `vst3-sys` → `vst3` — once
merged (or cherry-picked into `BillyDM/nih-plug`), your VST3 builds become
MIT/Apache rather than GPL-contaminated.

### CLAP support maturity

CLAP in `nih_plug` is arguably better supported than VST3 — it's the
"preferred" target per README. Specifically:

- Sample-accurate automation (`SAMPLE_ACCURATE_AUTOMATION = true` splits
  the audio cycle on param changes).
- Polyphonic parameter modulation (mono or poly mod).
- Note expression events, polyphonic MIDI CCs, pitch bend, channel pressure.
- MIDI SysEx (plugin defines `SysExMessage` associated type).
- `remote_controls()` — declarative controller pages that DAWs bind to
  hardware. Switched from draft to stable extension in 2025.
- CLAP version: 1.2.2 via `clap-sys` (micahrj fork, pin
  `rev = "25d7f53"`).

### AU support

Audio Unit is not possible via `nih_plug` itself. The sanctioned path is:

1. Build your plugin as CLAP (standard `nih_plug` export).
2. Use [`blepfx/clap-wrapper-rs`](https://github.com/blepfx/clap-wrapper-rs)
   v0.3.0 (March 2026) which bundles `free-audio/clap-wrapper` v0.14.0.
3. `export_auv2!()` macro reexports your existing `clap_entry` symbol.
4. Its bundler packages a `.component` AU bundle.

Limitations:

- AUv2 only; no AUv3.
- One plugin per binary (if `clap_entry` exposes multiple, only the first
  becomes an AU).
- No standalone target through the wrapper yet.
- The wrapper compiles the embedded VST3/AUv2 SDKs via the `cc` crate
  (no cmake), tested on Ubuntu 22.04, macOS 13.7, Windows 10.

Upstream is https://github.com/free-audio/clap-wrapper (C++); the Rust
wrapper bridges it for `nih_plug`-style projects.

---

## 3. GUI Frameworks

`nih_plug` provides three first-party GUI adapters. The README itself
states *"because everything is better when you do it yourself"* — and the
"BYO GUI" examples (`byo_gui_gl`, `byo_gui_softbuffer`, `byo_gui_wgpu`)
show that any `baseview` window can integrate.

### nih_plug_egui — DEPRECATED (soft)

- Path: `nih_plug_egui/`
- Backing: [`BillyDM/egui-baseview`](https://github.com/BillyDM/egui-baseview)
  v? (no releases published, ~101 commits, actively maintained by BillyDM).
- README explicitly says: *"consider using nih_plug_iced or nih_plug_vizia
  instead."*
- Upstream `nih-plug` updated egui 0.26.1 → 0.27.2 → later versions, but
  the adapter is clearly lower priority.
- Known issues: undo loops on sliders, slider snap behavior bugs
  ([issue #254](https://github.com/robbert-vdh/nih-plug/issues/254)),
  macOS window resize broken
  ([issue #253](https://github.com/robbert-vdh/nih-plug/issues/253)).
- Still perfectly fine for internal/prototype GUIs where accessibility and
  pixel-perfect design aren't required.

### nih_plug_iced — ACTIVE

- Path: `nih_plug_iced/`
- Backing: [`BillyDM/iced_baseview`](https://github.com/BillyDM/iced_baseview).
- Rendering: OpenGL by default. `wgpu` optional but flagged as "can segfault
  on certain systems" in the README — prefer OpenGL unless you have specific
  needs.
- PR #170 (Update Iced to 0.13) still open as of March 2026 — current
  `master` is on older Iced. The BillyDM fork may advance this.
- Most stable option for declarative, Elm-style UI with strong type safety.

### nih_plug_vizia — ACTIVE BUT WATCH

- Path: `nih_plug_vizia/`
- Backing: [`vizia/vizia`](https://github.com/vizia/vizia) v0.3.0
  (2025-04-16), Skia-based.
- Accessibility via [AccessKit](https://github.com/AccessKit/accesskit).
- A parallel project [`vizia/vizia-plug`](https://github.com/vizia/vizia-plug)
  exists as an "updated replacement for nih-plug-vizia" keeping pace with
  latest Vizia (16 commits, 4 open issues, 18 stars — small but current).
  This is where to look if the in-tree adapter lags.
- Known issues:
  - [#174 Accessibility not working](https://github.com/robbert-vdh/nih-plug/issues/174)
  - Historical font-name breakage when Vizia swapped text engines (called
    out in nih-plug CHANGELOG).
  - Plugin editor crashes on macOS/Windows when opening multiple instances
    were patched in 2024/2025.
- **Vizia 2026 state**: Active (38 open issues, 14 PRs on main repo), Discord
  community engaged. Production-ready for plugin UIs assuming you can
  tolerate pre-1.0 API drift (v0.3.x).

### Comparison for a plugin rack

| Criterion | egui | iced | vizia |
|---|---|---|---|
| Momentum | Fading | Steady | Most active for audio |
| Nested/dynamic content | OK | OK | Best (data binding + CSS) |
| Styling a "rack slot" UI | Manual | Stateful widgets | CSS + hot-reload — best fit |
| Accessibility | None | Limited | AccessKit |
| Rendering backend | glow/wgpu | OpenGL/wgpu | Skia |
| Maturity | Lowest friction | Stable | Most features, youngest code |

For a rack UI with draggable slots, per-slot chrome, and potentially
embedded plugin editors, **Vizia** is the strongest fit. Fall back to
**Iced** if CSS-styled reactivity is unnecessary.

### Alternative: webview GUIs

- [`httnn/nih-plug-webview`](https://github.com/httnn/nih-plug-webview):
  `wry`-based. macOS + Windows (Linux pending). Marked "work in progress,
  not production-ready yet." Known macOS bug: Escape key crash in Ableton
  Live. Useful if you want to write the UI in a web stack (React, Svelte,
  etc.) and communicate via JSON over a bridge.

---

## 4. Hosting Other Plugins Inside a nih_plug Plugin

### Short answer

`nih_plug` has **no built-in host functionality**. You must embed a separate
hosting library inside your plugin's `process()` callback. All practical
options in Rust in April 2026 are below.

### Option A: clack-host (recommended for CLAP-inside-CLAP/VST3)

- Crate: [`clack-host`](https://crates.io/crates/clack-host) from
  [`prokopyl/clack`](https://github.com/prokopyl/clack).
- Status: README states *"feature-complete, but APIs can still have
  breaking changes"*. 202 stars, 344 commits, no tagged releases.
- Authors state: *"there is (to the author's knowledge) no higher-level
  alternative available and functional yet"* — i.e., this **is** the Rust
  CLAP host solution.
- Also provides `clack-plugin` and `clack-extensions` (standard + custom
  extensions, including `track-info`, `gui`, `params`, etc.).
- Zero-cost abstractions, safe across audio/main-thread boundaries.

Minimal pattern (adapted from the README):

```rust
use clack_host::prelude::*;

// Load bundle (.clap file)
let bundle = PluginBundle::load("/path/to/Plugin.clap")?;
let host_info = HostInfo::new("MyRack", "Me", "https://example.com", "1.0.0")?;

// Instantiate
let factory = bundle.get_plugin_factory().unwrap();
let descriptor = factory.plugin_descriptors().next().unwrap();
let mut instance = PluginInstance::<MyHost>::new(
    |_| MyHostShared::new(),
    |_| MyHostMainThread::new(),
    &bundle,
    descriptor.id().unwrap(),
    &host_info,
)?;

// Activate
let audio_config = PluginAudioConfiguration {
    sample_rate: 48000.0,
    min_frames_count: 32,
    max_frames_count: 512,
};
let mut processor = instance
    .activate(|_, _, _| MyHostAudio::new(), audio_config)?
    .start_processing()?;

// Per block, in your nih_plug process():
processor.process(
    &input_audio_buffers,
    &mut output_audio_buffers,
    &input_events,
    &mut output_events,
    /* steady_time */ None,
    /* transport */ None,
)?;
```

**Thread model caveat**: `clack-host` enforces CLAP's main-thread vs
audio-thread separation via types. Your `nih_plug` plugin's `process()`
runs on the audio thread, so you pass the `AudioProcessor` there; parameter
GUI interaction goes through the main thread. You have to bridge these
(crossbeam SPSC channels for events).

### Option B: vst3 crate (for VST3-inside-nih_plug)

- Crate: [`vst3`](https://lib.rs/crates/vst3) v0.3.0.
- **No host-side helpers**. You get raw COM bindings (`IComponent`,
  `IAudioProcessor`, `IEditController`, etc.) plus smart pointers.
- You must:
  1. `libloading` the `.vst3` bundle (platform-specific layout:
     `Contents/x86_64-linux/Plugin.so` on Linux,
     `Contents/MacOS/Plugin` + Info.plist on macOS, `Contents/x86_64-win/Plugin.vst3` on Windows).
  2. Call the factory entry point (`GetPluginFactory`).
  3. Iterate `IPluginFactory::getClassInfo` and `createInstance` to build
     a component.
  4. Manually sequence `IComponent::setIoMode`, `setActive`, `setupProcessing`,
     and drive `IAudioProcessor::process` with a `ProcessData` struct you
     build each block.
- Expect to write ~500–1500 lines of unsafe-wrapper code.
- Reference: the `vst3` crate docs and the Steinberg
  [plug-in developer portal](https://steinbergmedia.github.io/vst3_dev_portal/).

### Option C: rack crate (experimental)

- Crate: [`rack`](https://crates.io/crates/rack) from
  [`sinkingsugar/rack`](https://github.com/sinkingsugar/rack).
- Claims: cross-platform discovery, loading, and processing for VST3,
  AudioUnit, CLAP.
- Actual status (README): **AudioUnit production-ready on macOS**,
  **VST3 working on macOS only (untested on Windows/Linux, no CI)**,
  **CLAP planned but not implemented**.
- Only 20 stars. Attractive API shape but not mature enough to bet a
  product on without contributing fixes upstream.

### Option D: plugin_host crate (scaffold only)

- Crate: [`plugin_host`](https://lib.rs/crates/plugin_host) v0.1.0
  (2026-02-25).
- Described honestly: *"bridges contain all required method stubs and
  inline comments mapping each method to its exact C API call, but users
  must connect clap-sys or vst3-sys to activate full functionality."*
- Useful as a **scaffold** — directory/scanner/sandboxing design is done
  — but you will finish implementing every bridge method yourself.
- Features it sketches: out-of-process sandboxing with auto-restart,
  preset management, project serialization.

### Option E: FFI to C++ clap-host / JUCE / iPlug2

- [`free-audio/clap-host`](https://github.com/free-audio/clap-host) is the
  Qt-based reference implementation in C++. Not embeddable easily.
- Wrapping JUCE's `AudioProcessorGraph` or iPlug2's `IGraphics` inside a
  Rust plugin means statically linking a C++ project and managing a
  FFI bridge — doable but painful. You lose most of the Rust ergonomics.

### Option F: The cutoff-vst (proprietary)

- Described in [Renaud Denis's case study](https://renauddenis.com/case-studies/rust-vst)
  as a "professional-grade" VST3 host wrapper in safe Rust covering 150+
  VST3 interface functions. Licensing not publicly stated, appears to be
  the private Tylium platform. Not a community option.

### Recommendation for the plugin rack

**Target CLAP-first hosting** via `clack-host`, and ship your rack as both
a CLAP and VST3 plugin (the rack itself uses `nih_plug` for the outer
shell). For users who want to load VST3 guests, require CLAP wrappers like
[`free-audio/clap-wrapper`](https://github.com/free-audio/clap-wrapper)
(it lets a CLAP host see VST3 plugins). Writing native VST3 hosting in
Rust from scratch is a significant undertaking and duplicates work that's
moving in `rack`, `plugin_host`, and private projects.

---

## 5. Parameter Handling

### Static model, not dynamic

`nih_plug` parameters use a compile-time derive macro:

```rust
#[derive(Params)]
struct MyParams {
    #[id = "gain"]
    gain: FloatParam,
    #[id = "freq"]
    freq: FloatParam,
    #[persist = "preset_name"]
    preset_name: Arc<RwLock<String>>,
    #[nested(group = "Filter")]
    filter: FilterParams,
    #[nested(id_prefix = "band", array)]
    bands: [BandParams; 8],
}
```

The `Params` trait (documented at
[params::Params](https://nih-plug.robbertvanderhelm.nl/nih_plug/params/trait.Params.html))
produces a fixed `param_map() -> Vec<(String, ParamPtr, String)>` at runtime,
but the **set of parameters is baked in at compile time**. You cannot add
or remove parameters after instantiation.

Supported features:

- Four param types: `FloatParam`, `IntParam`, `BoolParam`, `EnumParam`.
- `#[nested(group = "...")]` builds a slash-delimited group path for hosts
  that show a tree.
- `#[nested(id_prefix = "...")]` prefixes child IDs to avoid collisions.
- `#[nested(array)]` iterates arrays of nested `Params` structs.
- `#[persist = "key"]` for non-param persistent data (serde-serialized).
- Smoothing via the `Smoother<f32>` API, block-wise via
  `Smoother::next_block`.
- Callbacks on value changes.

### Dynamic parameters for a rack — the hard problem

A plugin rack hosting N arbitrary guest plugins wants to expose each
guest's parameters to the outer DAW for automation. The static-derive
design blocks the straightforward approach. Your options:

**1. Large fixed param pool (recommended, pragmatic).**
Declare e.g. 256 generic `FloatParam`s (`slot0_param0` ..
`slot7_param31`). At runtime, map guest params to free slots and forward
values. Advantages: works inside `nih_plug`; VST3 and CLAP both support
this. Disadvantages: capped, parameter names are static (display names
have to be generic or updated via host refresh — see below), and all
unused slots still appear in the DAW.

**2. VST3: `restartComponent(kParamTitlesChanged)` trick.**
VST3 has this flag (
[ivsteditcontroller.h](https://github.com/steinbergmedia/vst3_pluginterfaces/blob/master/vst/ivsteditcontroller.h))
but it is **unreliable in practice**:
- Per community reports, `kParamTitlesChanged` implies names changed but
  "not insertion or removal" of parameters.
- AUv3Wrapper has a known bug where receiving this flag resets all param
  values to defaults
  ([steinbergmedia/vst3_public_sdk#45](https://github.com/steinbergmedia/vst3_public_sdk/issues/45)).
- JUCE VST3 wrapper historically ignored name updates
  ([JUCE forum](https://forum.juce.com/t/vst3-parameter-name-changes-dont-work/28313)).
- Reaper and FL Studio behaviour is inconsistent.

**3. CLAP: `params.rescan(CLAP_PARAM_RESCAN_ALL)`.**
CLAP explicitly supports a plugin telling the host *"my params changed,
please rescan everything."* From
[clap/ext/params.h](https://github.com/free-audio/clap/blob/main/include/clap/ext/params.h):
you call this only when deactivated, the host discards its cache, and
asks for the new param set. This is **the closest thing to dynamic
parameters that actually works**, and is CLAP-only.

`nih_plug`'s current CLAP wrapper does **not** expose a way to trigger
`params.rescan` from plugin code (params are read once from `param_map`
at init). Implementing this requires a patch to the wrapper —
specifically, to cache params per-activation and call
`host_params->rescan` in response to a plugin-level event. This is the
lowest-risk path if you fork `BillyDM/nih-plug`.

**4. Own the params layer outside `nih_plug`.**
Accept that `nih_plug` is the wrong abstraction for dynamic param forwarding
and host your rack at the `clack-plugin` / raw `vst3` level. This is the
cleanest technical answer but doubles scope.

### VST3 param exposure

`nih_plug`'s VST3 wrapper maps each `ParamPtr` to a VST3 `ParamID` via
hashing the string ID. Param info is exposed via `IEditController::getParameterInfo`,
values via `getParamNormalized`/`setParamNormalized`. Units/groups map to
`IUnitInfo`. The wrapper auto-generates `UnitInfo` entries from nested
groups.

### CLAP param exposure

`ClapPlugin` side of the wrapper exposes params through
`clap_plugin_params` extension, routes value changes as
`CLAP_EVENT_PARAM_VALUE` / `CLAP_EVENT_PARAM_MOD` events in the input
stream. Sample-accurate automation works because `nih_plug` optionally
splits the audio cycle at each event timestamp when
`SAMPLE_ACCURATE_AUTOMATION = true`.

---

## 6. Audio Bus Configuration

### AudioIOLayout

```rust
pub struct AudioIOLayout {
    pub main_input_channels: Option<NonZeroU32>,
    pub main_output_channels: Option<NonZeroU32>,
    pub aux_input_ports: &'static [NonZeroU32],
    pub aux_output_ports: &'static [NonZeroU32],
    pub names: PortNames,
}
```

Declared as a constant on your `Plugin` impl:

```rust
const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
    AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),
        aux_input_ports: &[NonZeroU32::new(2).unwrap()],  // sidechain
        aux_output_ports: &[],
        names: PortNames::const_default(),
    },
    // Mono variant, 5.1, etc.
];
```

### What works

- **Sidechain input**: supported as an aux input port. Example naming
  auto-generated ("Sidechain Input 1", "Sidechain Input 2").
- **Multiple aux output buses**: supported as `aux_output_ports`. Useful
  for stem-out plugins.
- **Optional main I/O**: `main_input_channels: None` is legal (instruments).
- **Per-layout variations**: the host picks from the layout list; first is
  default.

### What does not work

- **Flexible IO negotiation**: explicit README/source comment:
  *"NIH-plug no longer supports flexible IO layouts. Instead we'll try
  to find an audio IO layout that matches the host's requested layout"*
  ([wrapper/vst3](https://github.com/robbert-vdh/nih-plug/blob/master/src/wrapper/vst3/wrapper.rs)).
  You must enumerate every layout up front.
- **VST3 `activateBus` is a stub**: returns `kResultOk` to satisfy the
  validator but does not actually track activation state. Plugins that
  need to respond to a host temporarily disabling a bus cannot do so
  through `nih_plug`. This is a real limitation for a rack that wants
  to save CPU on deactivated chains.
- **MIDI buses**: fixed at 1 in / 1 out (configurable on/off via
  `MIDI_INPUT`/`MIDI_OUTPUT: MidiConfig`). Cannot expose multiple MIDI
  ports per instance.

### AuxiliaryBuffers

The `process()` signature:

```rust
fn process(
    &mut self,
    buffer: &mut Buffer,
    aux: &mut AuxiliaryBuffers,
    context: &mut impl ProcessContext<Self>,
) -> ProcessStatus
```

`AuxiliaryBuffers::inputs` and `AuxiliaryBuffers::outputs` are
`&mut [Buffer]`, one per aux port declared in the chosen `AudioIOLayout`.
Useful idiom for a rack:

```rust
// Guest plugin 0 reads main input, writes to aux_output[0].
// Guest plugin 1 reads aux_output[0] (manually copied), writes aux_output[1].
// Final mix writes main output.
```

But note: you cannot expose an *unbounded* number of aux ports in a rack
without recompiling, since `AUDIO_IO_LAYOUTS` is a `&'static` slice.

---

## 7. Buffer Size and Sample Rate

### Negotiation

- `BufferConfig` is passed to `initialize()`:
  ```rust
  pub struct BufferConfig {
      pub sample_rate: f32,
      pub min_buffer_size: Option<u32>,
      pub max_buffer_size: u32,
      pub process_mode: ProcessMode,
  }
  ```
- `initialize()` is where you allocate buffers sized to
  `max_buffer_size`. It runs off the audio thread.
- `reset()` clears state; called after `initialize` and sometimes during
  processing (e.g. transport jumps). Must be allocation-free.
- `process()` receives a `Buffer` whose `samples()` may be anywhere in
  `[1, max_buffer_size]`. You cannot assume a fixed block size.

### `Buffer::iter_blocks(max_block_size)`

For algorithms that need sub-blocking (sample-accurate automation splits
the buffer at event boundaries; you may want further sub-blocking for
internal SIMD):

```rust
for (_block_start, mut block) in buffer.iter_blocks(64) {
    // block.samples() in [1, 64]
    for channel in block.iter_mut() {
        for sample in channel {
            *sample *= 0.5;
        }
    }
}
```

Recommended max block size from docs: **64 or 128 samples**.

### Latency

```rust
context.set_latency_samples(128);
```

- `set_latency_samples` can be called from `process()` or `initialize()`.
- **Known bug (unfixed as of April 2026):**
  [issue #177](https://github.com/robbert-vdh/nih-plug/issues/177) — calling
  it repeatedly inside `process()` on CLAP crashes with
  `"already mutably borrowed"` in `buffer_manager`. Workaround: change
  latency in `initialize()` only, or call at most once per process cycle
  guarded by a state check.
- VST3 path: wrapper calls `IComponentHandler::restartComponent(kLatencyChanged)`.
- CLAP path: wrapper emits `clap_host_latency->changed`.

### ProcessMode

`ProcessMode::Realtime` is the common case.
`ProcessMode::Offline`/`ProcessMode::Bounce` hint the host is rendering;
plugins may increase precision / disable denormals-are-zero at the cost
of CPU.

### Standalone buffer-size quirks

Two 2026-dated open issues:

- [#266 — CoreAudio delivers more samples than configured buffer](https://github.com/robbert-vdh/nih-plug/issues/266)
- [#264 — `transport.pos_samples` advances by configured period size
  instead of actual callback sample count](https://github.com/robbert-vdh/nih-plug/issues/264)

Both affect the CPAL-backed standalone mode on macOS. Don't rely on
standalone builds for correctness testing of timing-sensitive code.

---

## 8. Performance and Real-Time Safety

### SIMD

- `Buffer::iter_samples()` supports per-channel SIMD via adapters. Example
  in the Diopser plugin converts per-sample channel data to
  `std::simd::f32x2` via `to_simd_unchecked()` and back.
- Enable via the `simd` cargo feature on `nih_plug` (requires Rust
  **nightly** — `portable_simd` is not stable in April 2026).
- Alternatives if you want to stay on stable:
  - [`pulp`](https://crates.io/crates/pulp) — runtime SIMD dispatch.
  - [`multiversion`](https://crates.io/crates/multiversion) — function
    multi-versioning macros.
  - [`SIMDeez`](https://crates.io/crates/simdeez).
  - [`Fearless SIMD`](https://github.com/linebender/fearless_simd).

### Realtime safety

The framework leans heavily on the `assert_no_alloc` crate
([repo](https://github.com/Windfisch/rust-assert-no-alloc),
[nih-plug issue #30](https://github.com/robbert-vdh/nih-plug/issues/30)):

- Feature flag `assert_process_allocs` wraps the audio thread with a
  custom allocator that panics/aborts on allocation.
- Pair with `nih-log` (BillyDM's
  [nih-log](https://github.com/robbert-vdh/nih-log))
  for allocation-free logging with backtraces.
- Turn it on in debug builds, leave it off in release.

Pattern is pervasive in the ecosystem. Typical stack for a production
audio-thread path:

| Concern | Crate |
|---|---|
| Detect alloc in RT | `assert_no_alloc` |
| Disable denormals | `no_denormals` |
| Deferred destruction | `basedrop` (RT-safe "garbage collector") |
| SPSC queue (RT → UI) | `rtrb` or `ringbuf` |
| MPMC queue | `crossbeam-queue::ArrayQueue` |
| RT → non-RT state | `simple-left-right`, `triple_buffer`, `rt-write-lock` |
| Non-RT → RT state | `simple-left-right` |
| Atomic float params | `atomic_float` |

`nih_plug` itself uses `parking_lot` and `crossbeam` internally
(see `Cargo.toml`). For a plugin rack with N guest instances, you will
need at minimum: an SPSC queue per instance for param changes, and a
triple-buffer / left-right for the "active graph config" that the UI can
mutate.

### Background tasks

`Plugin::BackgroundTask` and `task_executor()` let you send work off the
audio thread from inside `process()`. The executor runs tasks on a worker
thread; the result gets shipped back via an internal channel. Use for
e.g. lazy FFT plan creation, file I/O on preset load.

---

## 9. Testing and Validation

### pluginval

- [Tracktion/pluginval](https://github.com/Tracktion/pluginval) is the
  standard cross-platform, cross-format validator (VST3 + AU).
- Headless mode returns exit 0/1 — CI friendly.
- Strictness levels 1–10. Level 5+ is generally required for host
  compatibility.
- Level 10 includes: parameter fuzzing, state save/restore cycles,
  process-then-deactivate-then-reactivate cycles, multiple instance
  interactions.
- Recommended CI job: build the VST3 bundle, run
  `pluginval --strictness-level 8 --validate-in-process --timeout-ms 300000
  ./target/bundled/MyPlugin.vst3` on each platform.

### clap-validator

- [free-audio/clap-validator](https://github.com/free-audio/clap-validator)
  — the CLAP equivalent. Written in Rust.
- Last tagged release 0.3.2 (2023-03-25) but 381 commits on `master` and
  still the canonical tool.
- Usage:
  ```
  clap-validator validate /path/to/plugin.clap
  clap-validator list tests
  clap-validator validate --only-failed ...
  ```
- Crash isolation: tests run in separate processes by default.
- Integrates in CI the same way as pluginval.

### `nih_plug` self-tests

- No in-tree unit test suite for the framework.
- Example plugins (`gain`, `sine`, `stft`) serve as smoke tests; the
  `.github/workflows/build.yml` builds them on Linux, macOS-universal,
  Windows. No test runs — just artifact upload.
- Cargo tests on plugin crates are your responsibility.

### Testing strategies for a rack

1. **Mock host harness.** Because `nih_plug` abstracts host features via
   traits (`ProcessContext`, `InitContext`, `GuiContext`), you can write
   an in-process harness that feeds known buffers and asserts outputs
   without running a real DAW.
2. **`clack-host` as a test driver.** Load your own plugin CLAP via
   `clack-host`, drive it with deterministic events, snapshot the output.
3. **Golden-file audio tests.** Render short test signals, compare
   bit-exactly (or via PSNR) against checked-in reference WAVs.
4. **pluginval + clap-validator** in CI matrix:
   `{ ubuntu-latest, macos-14, windows-latest } × { vst3, clap }`.

---

## 10. Cross-Platform Build

### Supported targets (from `nih_plug_xtask`)

| OS | Architectures |
|---|---|
| Linux | x86, x86_64, AArch64, RISC-V 64 |
| macOS | x86, x86_64, AArch64, **universal (lipo'd x86_64 + aarch64)** |
| Windows | x86, x86_64, AArch64 |

### xtask commands

```
cargo xtask bundle <package> --release
cargo xtask bundle-universal <package> --release     # macOS universal
cargo xtask known-packages                           # list bundlable packages
```

Driven by a workspace-root `bundler.toml`:

```toml
[my_rack]
name = "My Rack"
vst3 = true
clap = true
# standalone = false
```

Output goes to `target/bundled/` with correct `.vst3`, `.clap`,
`.component` bundle layouts per OS.

### macOS specifics

- `bundle-universal` cross-compiles x86_64 → aarch64 on an x86_64 runner
  (CI runs from `macos-latest` x86_64 runner in upstream).
- `MACOSX_DEPLOYMENT_TARGET=10.13` recommended for broad compatibility.
- Self-signing: `xtask` handles ad-hoc `codesign` automatically if you
  supply an identity; gatekeeper still blocks unnotarized builds for
  end-user distribution. For shipping, wire `notarytool` into a post-bundle
  step.
- Missing `Info.plist` for AU was historically a footgun — with
  `clap-wrapper-rs` this is handled.

### Windows specifics

- Nightly Rust required for SIMD feature. Use `rustup default nightly`.
- No cross-arch CI in upstream; AArch64 Windows is buildable but untested
  by the project.
- Bundler produces `.vst3` with the Windows layout:
  `MyPlugin.vst3/Contents/x86_64-win/MyPlugin.vst3` (a DLL with a
  different extension).

### Linux specifics

- Requires audio/graphics dev packages: `libasound2-dev libx11-dev
  libxcb1-dev libxcb-icccm4-dev libxkbcommon-dev libxcb-dri2-0-dev
  libxcb-xfixes0-dev libgl-dev`.
- For plugin UI: install `libxcb-shape0-dev libxcb-xrm-dev`.
- VST3 install path: `~/.vst3/`. CLAP: `~/.clap/`.

### CI pattern (observed in upstream)

```yaml
matrix:
  include:
    - { name: ubuntu-22.04,     runner: ubuntu-22.04,  cross: null }
    - { name: macos-universal,  runner: macos-13,      cross: lipo }
    - { name: windows,          runner: windows-latest, cross: null }
steps:
  - uses: actions/checkout@v4
  - run: rustup default nightly
  - run: cargo xtask bundle <package> --release    # or bundle-universal
  - uses: actions/upload-artifact@v4
```

No automated pluginval/clap-validator runs upstream. Add them yourself.

---

## 11. License Audit for a Plugin Rack

If you ship commercially:

| Dependency | License | Impact |
|---|---|---|
| `nih_plug` framework | ISC | Permissive, no copyleft |
| `nih_plug_egui`, `_iced`, `_vizia` | MIT OR Apache-2.0 | Permissive |
| `baseview` | MIT OR Apache-2.0 | Permissive |
| `vst3-sys` (OLD path) | **GPLv3** | Your VST3 build inherits GPLv3 |
| `vst3` crate v0.3.0 (NEW path, PR #263) | MIT OR Apache-2.0 | Clean |
| `clap-sys` | MIT OR Apache-2.0 | Clean |
| `clack-host` | MIT OR Apache-2.0 | Clean |
| `vizia` | MIT | Clean |
| `iced` | MIT | Clean |
| `egui` | MIT OR Apache-2.0 | Clean |

**Action item**: if you go VST3, adopt PR #263 (or equivalent in
`BillyDM/nih-plug`) before shipping — otherwise your product is GPLv3.

---

## 12. Recommendations for the Plugin Rack

### Stack

- **Framework**: fork [`BillyDM/nih-plug`](https://github.com/BillyDM/nih-plug),
  pin to a specific commit, track updates manually. Do **not** use
  `robbert-vdh/nih-plug` upstream directly; it is unmaintained.
- **VST3 bindings**: apply / cherry-pick
  [PR #263](https://github.com/robbert-vdh/nih-plug/pull/263) to get
  on the permissive `vst3` crate. Non-negotiable for commercial ship.
- **Formats**: export **CLAP + VST3** from `nih_plug`. Produce **AU** by
  running [`blepfx/clap-wrapper-rs`](https://github.com/blepfx/clap-wrapper-rs)
  on your CLAP build.
- **Guest hosting inside the rack**:
  - **CLAP guests**: [`clack-host`](https://github.com/prokopyl/clack).
    This is the only real option.
  - **VST3 guests**: either (a) require users to install
    [`free-audio/clap-wrapper`](https://github.com/free-audio/clap-wrapper)
    so that VST3s appear as CLAPs, or (b) write a VST3 host layer on top
    of the `vst3` crate. (a) is drastically less work.
  - **AU guests**: only on macOS, and only if you accept native AU host
    code (Objective-C / `AudioToolbox` FFI) — `rack` crate exists but is
    immature. Probably out of scope for v1.
- **GUI**: **`nih_plug_vizia`** via upstream or `vizia/vizia-plug`.
  Strongest fit for a slotted-rack UI. Fall back to `nih_plug_iced` if
  Vizia's pre-1.0 churn is intolerable.
- **Params**: accept the static-derive constraint. Use a **large fixed
  slot pool** (e.g. 16 slots × 64 params = 1024 generic floats) with
  runtime routing. Additionally, patch the CLAP wrapper to emit
  `params.rescan(CLAP_PARAM_RESCAN_ALL)` on slot reconfiguration — CLAP
  users get nice per-guest param names, VST3 users get generic ones.
- **Latency**: set once in `initialize()`. Do not touch
  `set_latency_samples` in `process()` until issue
  [#177](https://github.com/robbert-vdh/nih-plug/issues/177) is fixed in
  your fork.
- **RT safety**: enable `assert_process_allocs` in debug builds. Use
  `rtrb` for SPSC queues between audio and UI threads,
  `simple-left-right` for guest-graph-config updates,
  `basedrop` for deferred destruction of guest plugin instances (when
  the user removes a slot, you cannot drop the `PluginInstance` on the
  audio thread).
- **Testing**: pluginval level 8 + clap-validator in CI on all three
  OSes. Golden-file audio regression tests for the rack's pass-through
  and routing logic.

### Things to avoid

- **Don't** try to dynamically mutate `#[derive(Params)]` structures.
  The framework cannot support it without a fork.
- **Don't** rely on VST3 `kParamTitlesChanged` for dynamic param exposure
  — host support is inconsistent.
- **Don't** use `nih_plug_egui` for a new project.
- **Don't** use `wgpu` renderer for `nih_plug_iced` unless you have a
  specific need and are prepared for segfaults on some systems (README
  warning).
- **Don't** call `set_latency_samples` mid-`process()` on CLAP until
  [#177](https://github.com/robbert-vdh/nih-plug/issues/177) is fixed.
- **Don't** assume `activateBus` does anything in VST3 — the wrapper
  stubs it out.
- **Don't** bet on the `rack` or `plugin_host` crates for production VST3
  hosting in April 2026. Both are scaffolds.
- **Don't** ship a VST3 linked through the old `vst3-sys` path without
  understanding the GPLv3 implications.

### Open problems you will hit

1. **Editor hosting.** Displaying a guest's plugin editor window inside
   your rack window is non-trivial. `clack-extensions::gui` gives you
   plugin-side hooks (`gui.show(parent_window)`). You must integrate that
   with your own `baseview` / `vizia` host window — no turnkey support.
2. **Process model mismatch.** VST3 uses pre-allocated buses; CLAP uses
   event streams. Your rack's internal event bus must abstract both.
   Look at `clack-host`'s `InputEvents`/`OutputEvents` types as a template.
3. **Per-guest latency compensation.** Rack must sum guest latencies and
   report the total. Changing a slot changes total latency; see bug #177.
4. **State persistence.** Serializing nested guest plugin state requires
   each guest's `getState`/`setState` (VST3) or `state.save`/`state.load`
   (CLAP) round-trip. Format your own top-level container format
   (bincode or msgpack of `Vec<(PluginID, Vec<u8>)>`).

---

## 13. URLs Reference

### Core framework

- nih-plug upstream: https://github.com/robbert-vdh/nih-plug
- nih-plug fork (maintained): https://github.com/BillyDM/nih-plug
- Fork announcement: https://github.com/robbert-vdh/nih-plug/issues/265
- Docs site (stale): https://nih-plug.robbertvanderhelm.nl/nih_plug/
- CHANGELOG: https://github.com/robbert-vdh/nih-plug/blob/master/CHANGELOG.md

### Notable PRs and issues

- PR #263 vst3-sys → vst3 migration: https://github.com/robbert-vdh/nih-plug/pull/263
- PR #260 TrackInfo API: https://github.com/robbert-vdh/nih-plug/pull/260
- PR #225 wrapper ref-cycles: https://github.com/robbert-vdh/nih-plug/pull/225
- PR #170 Iced 0.13: https://github.com/robbert-vdh/nih-plug/pull/170
- Issue #177 CLAP latency crash: https://github.com/robbert-vdh/nih-plug/issues/177
- Issue #174 Vizia accessibility: https://github.com/robbert-vdh/nih-plug/issues/174
- Issue #266 macOS CoreAudio buffer: https://github.com/robbert-vdh/nih-plug/issues/266
- Issue #264 transport.pos_samples: https://github.com/robbert-vdh/nih-plug/issues/264
- Issue #30 assert_no_alloc integration: https://github.com/robbert-vdh/nih-plug/issues/30

### VST3 Rust

- vst3 crate (permissive): https://lib.rs/crates/vst3
- vst3-rs generator: https://github.com/coupler-rs/vst3-rs
- vst3-sys (GPL legacy): https://github.com/RustAudio/vst3-sys
- Blog: simplifying the build process: https://micahrj.github.io/posts/vst3/
- VST3 SDK 3.8.0 MIT announcement (Oct 2024): Steinberg forums

### CLAP Rust

- clap spec: https://github.com/free-audio/clap
- clap-host reference (C++/Qt): https://github.com/free-audio/clap-host
- clap-sys (Rust raw): https://github.com/glowcoil/clap-sys (latest at
  micahrj/clap-sys fork)
- clack (prokopyl): https://github.com/prokopyl/clack
- clack-host: https://crates.io/crates/clack-host
- clack archived mirror (Meadowlark): https://github.com/MeadowlarkDAW/clack
- clap-validator: https://github.com/free-audio/clap-validator
- clap-wrapper (C++ → VST3/AUv2): https://github.com/free-audio/clap-wrapper
- clap-wrapper-rs: https://github.com/blepfx/clap-wrapper-rs

### GUI frameworks and adapters

- egui-baseview: https://github.com/BillyDM/egui-baseview
- iced_baseview: https://github.com/BillyDM/iced_baseview
- vizia: https://github.com/vizia/vizia
- vizia-plug (updated adapter): https://github.com/vizia/vizia-plug
- nih-plug-webview: https://github.com/httnn/nih-plug-webview

### Hosting libraries (plugins-in-Rust)

- sinkingsugar/rack: https://github.com/sinkingsugar/rack
- plugin_host crate: https://lib.rs/crates/plugin_host
- cutoff-vst case study (proprietary): https://renauddenis.com/case-studies/rust-vst
- rusty-daw-plugin-host: https://crates.io/crates/rusty-daw-plugin-host

### Real-time safety and support crates

- assert_no_alloc: https://github.com/Windfisch/rust-assert-no-alloc
- nih-log: https://github.com/robbert-vdh/nih-log
- rtrb: https://github.com/mgeier/rtrb
- ringbuf: https://github.com/agerasev/ringbuf
- basedrop: (on crates.io; used in Meadowlark)
- awesome-audio-dsp reference: https://github.com/BillyDM/awesome-audio-dsp
- Plugin Development Frameworks section: https://github.com/BillyDM/awesome-audio-dsp/blob/main/sections/PLUGIN_DEVELOPMENT_FRAMEWORKS.md

### Alternative frameworks

- coupler: https://github.com/coupler-rs/coupler (early, not production-ready)

### Validation

- pluginval: https://github.com/Tracktion/pluginval

### Tutorials

- Kwarf: Writing a CLAP synthesizer in Rust (Parts 1–3):
  https://kwarf.com/2024/07/writing-a-clap-synthesizer-in-rust-part-1/
  /part-2/, https://kwarf.com/2025/03/writing-a-clap-synthesizer-in-rust-part-3/
- Nathan Phennel on VST dev in Rust: https://enphnt.github.io/blog/vst-plugins-rust/
- BillyDM blog (audio buffer API): https://billydm.github.io/blog/audio-buffer-api/
- BillyDM blog (porting a reverb): https://billydm.github.io/blog/porting-a-reverb/
