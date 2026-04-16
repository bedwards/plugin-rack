# Hosting VST3 and CLAP Plugins Inside a VST3 Plugin in Rust

_Research snapshot: April 2026_

This document researches how to build a "plugin rack" — a VST3 plugin that, itself, loads and hosts other VST3 and CLAP plugins in-process. The rack is written in Rust and is intended to be loaded inside a DAW (Logic, Ableton Live, Cubase, Bitwig, Reaper).

The core tension: a VST3 plugin is built with plugin SDKs; a plugin host needs host SDKs. The Rust crate ecosystem is strongly skewed toward the plugin side. Hosting is possible, but the Rust library surface is thin and several of the "best" options are either very new (2025–2026), experimental, or effectively wrappers around Steinberg's own C++ host helper classes.

---

## 1. State of Rust VST3 Host Crates

There is no "batteries-included, ship-it-today" VST3 host crate in Rust the way there is a plugin crate (`nih-plug`). The space is fragmented across four broad categories: raw FFI bindings, binding generators, frameworks that happen to export host glue, and one or two recent attempts at an opinionated host-oriented API.

### 1.1 `vst3-sys` (RustAudio/vst3-sys)

- Repo: <https://github.com/RustAudio/vst3-sys>
- Status: "Raw Bindings to the VST3 API." A pure-Rust port of the VST3 COM API — no SDK is distributed, no clean abstractions.
- Primary use-case: implementing plugins. It exposes enough of the COM ABI (FUnknown, IPluginFactory, IAudioProcessor, etc.) that you can build either side, but there are no host helper classes (no `Module`, no `PluginFactory`, no `PlugProvider`). You get the interface definitions and a macro system for declaring COM classes.
- Maintenance: active but slow. The README still says "Currently this crate is missing definitions of some of the constants found in the SDK." No formal releases are published on GitHub.
- Verdict for a host: usable as a low-level base, but you'd be rebuilding all of `public.sdk/source/vst/hosting/*` yourself. Significant effort.

### 1.2 `vst3-bindgen` / `coupler-rs/vst3-rs` / `vst3` crate

- Repo: <https://github.com/coupler-rs/vst3-rs>
- Latest release: `vst3` crate **0.3.0** on crates.io, published 2025-12-07.
- License: MIT OR Apache-2.0.
- Status: this is the best-maintained pure bindings crate. It generates bindings from Steinberg's C++ headers via `com-scrape`. As of 0.3.0, the generated bindings are vendored in the crate source, so you no longer need a build-time pass against the SDK. This became possible because Steinberg relicensed the VST3 SDK under MIT in October 2025 (SDK 3.8.0).
- API: exposes `Steinberg::*` namespaces mirrored as Rust modules, plus `ComPtr`, `ComRef`, `ComWrapper`, and `Class`/`Interface` traits. The bindings themselves are unsafe; you bring the safety layer.
- Host side: no host helper classes are included (no `Module`, no `PluginFactory` helper). But because FUnknown/IPluginFactory/IComponent are there, you can write the module-loading code yourself — it's roughly 200–400 lines per-platform.
- Verdict for a host: **this is the right low-level foundation in Rust as of April 2026.** It tracks the MIT-licensed SDK, publishes releases, has a working Cargo story, and the generated bindings are complete enough for both plugin and host work.

### 1.3 `vst3` crate (`crates.io/crates/vst3`)

- This is the crate published by the coupler-rs project. Same project as §1.2. Don't confuse with any older `vst3` crate squatting.

### 1.4 `plugin_host` crate

- Repo: referenced on lib.rs and crates.io.
- Version: **0.1.0** released 2026-02-25. License MIT. Self-described as "unstable."
- Scope: "VST3 and CLAP plugin host for DAW applications" with scanning, sandboxing (in-process or out-of-process per plugin), auto-restart for crashed plugins, parameter automation, preset management, and project serialization.
- Reality check: the crate is very new and reads as scaffolding. Its primary dependency is `libloading`. The bridge implementations are described as "fully scaffolded but awaiting connection to the underlying C APIs." Treat it as a useful reference for the _shape_ of an ergonomic host API, not a production dependency.
- Verdict: worth reading, not yet worth depending on.

### 1.5 `rack` crate

- Crate page: <https://crates.io/crates/rack>
- Version: **0.4.8**, MIT OR Apache-2.0.
- Scope: "modern Rust library for hosting audio plugins." AudioUnit on macOS/iOS is described as production-ready (Phases 1–8). VST3 is listed as supported on Windows and Linux (implying the macOS side is still in-progress or routed through AU). CLAP is "coming soon."
- API: `PluginScanner`, `PluginInstance`, `PluginInfo` and friends through a prelude. Planar buffer model.
- Verdict: most promising "host-first" Rust crate. The AU path is the most complete; VST3 coverage is thinner and CLAP is not yet there. Watch this one.

### 1.6 `cutoff-vst` (Tylium)

- Case study: <https://renauddenis.com/case-studies/rust-vst>
- Status: proprietary component of the "Cutoff" platform. Not public on crates.io as of this writing; described as wrapping "over 150 functions across dozens of interfaces" with a safe Rust facade, a plugin scanner with metadata caching, lifecycle management, automation, state persistence, and macOS UI embedding.
- Verdict: not available as an open dependency. Useful as a _design_ reference for what a good Rust VST3 host API looks like.

### 1.7 `HelgeSverre/rust-vst3-host`

- Repo: <https://github.com/HelgeSverre/rust-vst3-host>
- Status: experimental, self-described as "for learning purposes, unsuitable for production." Extensive unsafe, manual COM, `Arc<Mutex<AudioProcessingState>>` for thread hand-off.
- Verdict: best used as a worked example to read. Don't vendor.

### 1.8 `nih-plug`

- Repo: <https://github.com/robbert-vdh/nih-plug>
- Scope: plugin framework (VST3 + CLAP + standalone exporters). Not a host. Mentioned here only because it is frequently conflated with host tooling — do not use it to host other plugins.

### 1.9 `vst-rs`

- Repo: <https://github.com/RustAudio/vst-rs>
- Status: VST 2.4, officially deprecated. Ignore.

### Summary table

| Crate | Role | Latest | License | Host fitness |
|---|---|---|---|---|
| `vst3` (coupler-rs) | FFI bindings | 0.3.0 (2025-12) | MIT/Apache-2.0 | Recommended low-level base |
| `vst3-sys` | FFI bindings | unversioned | MIT/Apache-2.0 | Usable, less polished |
| `rack` | High-level host | 0.4.8 | MIT/Apache-2.0 | Promising, AU-first |
| `plugin_host` | High-level host | 0.1.0 (2026-02) | MIT | Scaffolding only |
| `cutoff-vst` | Proprietary host | n/a | closed | Design reference only |
| `rust-vst3-host` | Experimental | n/a | n/a | Read, don't depend |
| `nih-plug` | Plugin framework | n/a | ISC | Not a host |

---

## 2. CLAP Hosting in Rust: `clack`

### 2.1 Overview

- Canonical repo: <https://github.com/prokopyl/clack>
- Archived fork: <https://github.com/MeadowlarkDAW/clack> (archived 2023-05-09; don't use)
- Reference C++ host the API is modeled on: <https://github.com/prokopyl/clap-host>

`clack` is explicitly "Safe, low-level wrapper to create CLAP audio plugins and hosts in Rust." It is split into two crates: `clack-plugin` and `clack-host`. As of early 2026 the project describes itself as **feature-complete but still pre-1.0**, with API breakage still possible. No formal crates.io releases have been published from the primary branch at time of writing — most users track it as a git dependency.

### 2.2 `clack-host` capabilities

`clack-host` can, in-process:

- Scan CLAP search paths (OS defaults plus `CLAP_PATH`).
- Load a `.clap` bundle via `libloading` wrapped in `clack_host::bundle::PluginBundle`.
- Call `clap_plugin_entry_t::init(path)`, fetch the plugin factory, enumerate descriptors, and instantiate plugin instances.
- Drive `activate`, `start_processing`, `process`, `stop_processing`, `deactivate` with type-safe event lists and audio buffer adapters.
- Implement host-side extensions the plugin asks for (log, timer, audio-ports, params, state, gui, thread-check, etc.).
- Hold plugins in a state machine that mirrors the CLAP lifecycle (audio thread vs main thread separation is encoded in the type system).

### 2.3 Can it host arbitrary CLAP plugins in-process?

Yes. The reference example under `examples/` (often cited as `clack-host-cpal`) loads an arbitrary `.clap` bundle, runs it against CPAL, and streams audio. Non-trivial plugins (Surge XT, u-he Diva/Zebra CLAP builds, Vital) load in this example; the rough edges that remain are around the newer extensions (audio ports v2 changes, thread-pool/voice-info, remote-controls metadata) rather than core hosting.

### 2.4 Maturity verdict

`clack-host` is the only serious path for CLAP hosting in Rust, and is mature enough for a plugin rack. Expect some version pinning pain until a 1.0 is cut. Pin to a git SHA.

---

## 3. Hybrid: Linking Steinberg's C++ Host Helper Classes via FFI

### 3.1 What you'd be linking

Steinberg's SDK ships `public.sdk/source/vst/hosting/` — a set of C++ helper classes specifically for building hosts:

- `VST3::Hosting::Module` — cross-platform bundle loader with `::create(path, error)`.
- `VST3::Hosting::PluginFactory` — wraps `IPluginFactory{,2,3}`, enumerates class infos.
- `VST3::Hosting::ClassInfo` — decoded `PClassInfoW` metadata.
- `Steinberg::Vst::PlugProvider` — full end-to-end glue: loads module, creates `IComponent` and `IEditController`, connects them, queries bus info.
- `Steinberg::Vst::HostApplication`, `ParameterChanges`, `EventList`, `ProcessData` helpers.
- Platform shims: `module_mac.mm`, `module_win32.cpp`, `module_linux.cpp`.

These classes do the exact work a Rust host would otherwise have to reimplement: `bundleEntry`/`bundleExit`, `InitDll`/`ExitDll`, moduleinfo.json parsing, per-OS bundle path resolution, and reference-counted COM lifetime.

### 3.2 Feasibility in Rust

The path is `cxx`, `autocxx`, or hand-rolled `extern "C"` shims. The common pattern:

1. Write a thin C++ static library that exposes `extern "C"` functions calling into `VST3::Hosting::Module` and friends.
2. Build it with `cc`/`cmake-rs` from `build.rs`.
3. Link it. Call from Rust with `unsafe extern "C"`.

This is what Ardour effectively does in C++ (`libs/ardour/vst3_plugin.cc` + `vst3_scan.cc`, by Robin Gareus), and is how the closed-source `cutoff-vst` is described. It's the most pragmatic path if you want feature parity with commercial hosts quickly.

### 3.3 Cross-platform pain

- **macOS.** Plugins are CFBundle directories with a Mach-O inside `Contents/MacOS/`. Load with `CFBundleCreate` + `CFBundleLoadExecutable`. Symbols to look up: `bundleEntry` (must call before anything), `GetPluginFactory`, `bundleExit` (on unload). Beware code-signing: if the host is hardened-runtime and the plugin is unsigned, `dlopen` may fail with `EPERM`. Also: you cannot `dlopen` a CFBundle directory with `libloading` alone on macOS — you need the Core Foundation APIs (or a symlink trick pointing at the Mach-O, which many Rust hosts use as a workaround).
- **Windows.** Plugins are folders with `.vst3` extension containing `Contents\x86_64-win\Plugin.vst3` (a DLL). Load with `LoadLibraryW` on the DLL path. Symbols: optional `InitDll` before, `GetPluginFactory`, optional `ExitDll` after. COM reference counting works but VST3 uses its own `FUnknown`, not COM `IUnknown` directly — only the layout is compatible.
- **Linux.** Plugins are folders with `.vst3` extension containing `Contents/x86_64-linux/Plugin.so`. Load with `dlopen`. Symbols: `ModuleEntry`/`ModuleExit` (analogous to macOS bundleEntry/bundleExit), plus `GetPluginFactory`. Linux hosts also need to supply `Steinberg::Linux::IRunLoop` to the plugin — many Linux VST3 plugins require it for their editor event loop.

The Rust crate `libloading` handles Windows DLLs and Linux `.so` cleanly. For macOS CFBundle, either:
- call `CFBundleCreate` via `core-foundation` crate, or
- open the inner Mach-O directly (works for most plugins but skips `bundleEntry`, which breaks some).

### 3.4 When to choose the hybrid path

Choose the hybrid path (link Steinberg C++ hosting classes) if:
- You want the broadest plugin compatibility on day 1.
- You're OK with a C++ build dependency.
- You want the Linux `IRunLoop` plumbing done for you.

Choose pure-Rust (`vst3` crate + your own module loader) if:
- You want a clean Cargo build with no C++ toolchain.
- You're targeting a subset of plugins you control test for.
- You have time to re-implement `Module`/`PluginFactory`.

---

## 4. Validating VST3 Plugins From a Host

### 4.1 `pluginval` (Tracktion)

- Repo: <https://github.com/Tracktion/pluginval>
- License: GPLv3.
- Cross-platform (macOS/Windows/Linux). Tests VST/VST3/AU at strictness levels 1–10; level 5 is the usual "must-pass" bar for host compatibility. Level 1–3 are crash/call-coverage tests; 6–10 fuzz parameters, state, and threading.
- Command-line mode: `pluginval --strictness-level 5 /path/to/plugin.vst3`. Exit code 0 on pass, 1 on fail. Designed for CI.
- From Rust: shell out to the binary. You can parse stdout or consume the JSON-lines when run with `--output-dir` for per-test reports.

### 4.2 Steinberg's `validator`

- Source: `public.sdk/samples/vst-hosting/validator` in the VST3 SDK.
- Behaves as a minimal host and exercises conformity paths (bus configuration, process setup, state round-trip, parameter metadata, etc.).
- Invoked automatically by the SDK CMake build for plugins, but can be run standalone: `validator /path/to/plugin.vst3`.
- Lower-level than pluginval, runs faster, and produces simpler output. Good for a pre-commit gate.

### 4.3 `moduleinfotool`

- Source: `public.sdk/vst-utilities/moduleinfotool` in the SDK.
- Creates and validates `moduleinfo.json`. A host scanner that respects moduleinfo can skip full module load during discovery and get metadata directly from JSON, greatly speeding up first-launch scanning.

### 4.4 Using validators from a Rust host

The pragmatic Rust flow:

1. Ship `pluginval` binary alongside the rack (or detect it on PATH).
2. On first discovery of a plugin, run `pluginval --strictness-level 5 --timeout-ms 30000 $plugin`. Cache the result by `(path, mtime, size, cid)` key.
3. Plugins that crash or timeout during validation get quarantined with a user-visible reason.
4. Offer an "override" that runs the plugin anyway — plenty of real-world plugins fail pluginval strictly and still work in real DAWs.

---

## 5. Plugin Discovery

### 5.1 Default search paths

**VST3**
- macOS: `~/Library/Audio/Plug-Ins/VST3` and `/Library/Audio/Plug-Ins/VST3`.
- Windows: `%ProgramFiles%\Common Files\VST3` (system) and `%LOCALAPPDATA%\Programs\Common\VST3` (user).
- Linux: `~/.vst3` and `/usr/lib/vst3` (and `/usr/local/lib/vst3`).
- Additionally honor `VST3_PATH` if set (not all DAWs do, but many).

**CLAP**
- macOS: `~/Library/Audio/Plug-Ins/CLAP` and `/Library/Audio/Plug-Ins/CLAP`.
- Windows: `%COMMONPROGRAMFILES%\CLAP` and `%LOCALAPPDATA%\Programs\Common\CLAP`.
- Linux: `~/.clap` and `/usr/lib/clap`.
- Always also honor the `CLAP_PATH` env var (colon- or semicolon-separated).

A scanner should walk these directories recursively for `.vst3` and `.clap` bundles (not following symlinks outside the root, to avoid loops).

### 5.2 VST3 bundle layout (recap)

```
MyPlugin.vst3/
└── Contents/
    ├── Info.plist                     (macOS only)
    ├── PkgInfo                         (macOS only)
    ├── MacOS/                          (macOS)
    │   └── MyPlugin                    (Mach-O, universal)
    ├── x86_64-win/                     (Windows)
    │   └── MyPlugin.vst3               (DLL)
    ├── arm64ec-win/                    (Windows on Arm, recommended)
    │   └── MyPlugin.vst3
    ├── x86_64-linux/                   (Linux)
    │   └── MyPlugin.so
    ├── aarch64-linux/                  (optional)
    │   └── MyPlugin.so
    └── Resources/
        ├── moduleinfo.json             (VST 3.7.5+; relocated here 3.7.8+)
        └── Snapshots/                  (optional PNG previews)
```

### 5.3 `moduleinfo.json` (VST 3.7.5+, relocated 3.7.8)

A host that respects `moduleinfo.json` does not need to load the module to learn its class list. Minimal example:

```json
{
  "Name": "HelloWorld",
  "Version": "3.7.5.0",
  "Factory Info": {
    "Vendor": "Steinberg Media Technologies",
    "URL": "http://www.steinberg.net",
    "E-Mail": "mailto:support@steinberg.de",
    "Flags": { "Unicode": true, "Classes Discardable": false }
  },
  "Classes": [{
    "CID": "BD58B550F9E5634E9D2EFF39EA0927B1",
    "Category": "Audio Module Class",
    "Name": "Hello World",
    "Vendor": "Steinberg",
    "Version": "1.0.0.0",
    "SDKVersion": "VST 3.7.5",
    "Sub Categories": ["Fx", "Stereo"],
    "Class Flags": 0,
    "Cardinality": 2147483647,
    "Snapshots": []
  }],
  "Compatibility": [{
    "New": "BD58B550F9E5634E9D2EFF39EA0927B1",
    "Old": ["AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"]
  }]
}
```

File is JSON5; parse with `json5` crate. If present and well-formed, skip the full load for discovery metadata. If absent, fall back to loading the module and querying `IPluginFactory`. Cache results keyed on `(path, mtime, len)`.

### 5.4 Compatibility entries

`moduleinfo.json`'s `Compatibility[]` maps new class IDs to legacy IDs (VST2 FourCC or previous VST3 CIDs). When loading a session that references an old CID, you can upgrade it to the new one transparently.

---

## 6. Sandboxing / Crash Isolation

### 6.1 How commercial hosts approach this

- **Bitwig Studio**: three sandbox modes — "Together" (all plugins in one subprocess), "Per Plugin" (one subprocess per plugin instance), "Individually" (fully isolated). Communication is via shared memory ring buffers and POSIX sockets / Windows named pipes. A plugin crash only takes down its subprocess; Bitwig keeps the audio engine alive and the plugin's output is silenced until restarted.
- **Cubase / Nuendo**: optional VST3 plugin sandboxing ("VST Plug-in Sandboxing"), disabled by default; enables per-plugin subprocess hosting.
- **Studio One**: per-plugin subprocess hosting for VST2/VST3, optional.
- **Ardour**: explicitly runs plugins in-process and documents why — they accept the DAW-crash risk in exchange for lower latency and simpler state. See <https://ardour.org/plugins-in-process.html>.
- **Reaper**: has an optional "run plugin as separate process" mode via `reaper_host` executables.

Every sandbox adds per-buffer context switches. Bitwig reports roughly 1–2 extra buffer periods of added round-trip latency in their docs; real-world this depends on IPC primitive choice.

### 6.2 Is in-process acceptable for v1?

Yes. For a first-cut plugin rack:

- In-process is dramatically simpler. No subprocess management, no shared memory, no IPC wire format.
- A plugin crash will take down the rack, which takes down the DAW — but this is _exactly_ how Ardour operates and how most hosts behaved for decades.
- Parameter serialization, automation, and GUI embedding all "just work" because everything is in one address space.
- You can add out-of-process hosting in v2 without changing the external API if you keep an `HostedPlugin` trait boundary.

v1 recommendation: in-process. Add a watchdog (`catch_unwind` on the audio path, and per-OS structured exception handling via `os_sigaction` or `AddVectoredExceptionHandler`) that tries to bypass a misbehaving plugin, but accept that some crashes are unrecoverable. Note that Rust's `catch_unwind` does not catch C++ exceptions or SIGSEGVs — it's panic-only. For real crash protection you need OS signal handlers (fragile in a real-time audio thread) or out-of-process.

---

## 7. Threading Model and Latency

### 7.1 Audio thread: nested `process()` calls

When the DAW calls the rack's `IAudioProcessor::process(ProcessData&)`, the rack is on the audio thread. It must then, _synchronously_, call each hosted plugin's `process()` in sequence, feeding each one's output into the next one's input. This is a nested, fan-out-fan-in graph evaluation.

Key constraints:

- All plugin `process()` calls happen on the rack's audio thread. Do not spawn threads.
- Never allocate on the audio thread. Pre-allocate all `ProcessData`, `ParameterChanges`, `EventList`, and audio buffer scratch memory during `setupProcessing()` / `setProcessing(true)`.
- Honor the host-provided `symbolicSampleSize` (32 vs 64-bit float). The rack must present whichever format the DAW sent to each hosted plugin. Convert if a hosted plugin only supports one.
- Honor `processMode`: `kRealtime` vs `kPrefetch` vs `kOffline`. Pass through to hosted plugins.

### 7.2 Latency compensation

VST3 exposes latency via `IAudioProcessor::getLatencySamples()`. Each hosted plugin in a serial chain contributes its own latency; the rack's total reported latency to the DAW is the **sum** of in-series plugins plus any internal buffering the rack itself adds.

If a hosted plugin changes its latency (via `IComponentHandler::restartComponent(kLatencyChanged)`), the rack must:

1. Recompute its own total latency.
2. Propagate a `restartComponent(kLatencyChanged)` up to the DAW so the DAW re-runs its plugin-delay compensation.

For parallel chains (e.g. wet/dry splits inside a rack), the rack must insert its own delay lines to align the latencies of the branches before mixing. This is classic plugin-delay-compensation work; implement it with a sample-accurate ring buffer per branch.

### 7.3 Sample-accurate automation pass-through

The DAW sends automation to the rack as `IParameterChanges` inside `ProcessData::inputParameterChanges`. Each `IParamValueQueue` has sample-offset points.

For pass-through to a hosted plugin, the rack maintains a mapping `(rack_param_id -> (plugin_index, plugin_param_id))`. When iterating the DAW's input queues, route each point into a per-hosted-plugin `ParameterChanges` object, preserving sample offsets. Pass that to the nested `process()` call.

For the reverse direction (nested plugin reports value changes via `outputParameterChanges`), the rack aggregates them, maps back to the rack's param space, and places them in its own `outputParameterChanges` so the DAW sees them.

---

## 8. Parameter Forwarding

### 8.1 Should the rack expose nested plugin params to the DAW?

Yes, with nuance:

- **Raw exposure**: expose _every_ nested param. Pros: full automation. Cons: DAW UI clutter; potentially thousands of params; breaks when the chain changes.
- **Mapped exposure**: rack has a fixed number of "macro" slots (e.g. 32–64) that the user assigns to nested params. DAW sees only the macros. Pros: clean. Cons: user friction.
- **Hybrid**: fixed macros plus on-demand exposure of recently-touched nested params.

Commercial prior art: Ableton's Instrument/Audio Racks expose macros; Bitwig exposes device parameters directly; FL's Patcher exposes a configurable subset.

### 8.2 Dynamic parameter lists (`kParamTitlesChanged` / `kReloadComponent`)

VST3 supports dynamic parameter lists, but the host's cooperation is limited. The rack, as a plugin, uses:

- `IComponentHandler::restartComponent(kParamTitlesChanged)` — tells the DAW to re-query `getParameterInfo` for each param: title, units, step count, flags, default. IDs must be stable.
- `IComponentHandler::restartComponent(kParamValuesChanged)` — values only.
- `IComponentHandler::restartComponent(kReloadComponent)` — full rebuild. Some DAWs handle this poorly (notoriously, Logic and pre-2023 Ableton Live). See the Steinberg issue where `kParamTitlesChanged` resets parameter values in the AUv3 wrapper.

Practical implication: design the rack's param ID space to be **stable across chain edits**. Pre-allocate a large macro pool (say, 128 params) with fixed IDs. Titles and flags can change (and fire `kParamTitlesChanged`); IDs cannot be re-used. Only call `kReloadComponent` as a last resort, on explicit user action.

### 8.3 Param metadata mapping

When mapping a rack macro to a nested plugin param, forward metadata:

- `title` = nested param title (or user rename).
- `units` = nested param units.
- `stepCount` = nested param stepCount.
- `defaultNormalizedValue` = nested default.
- `flags` = merge (`kCanAutomate` generally preserved; `kIsBypass` only for a true bypass macro).

---

## 9. GUI Hosting

### 9.1 Opening the nested plugin's editor

Each hosted plugin exposes an `IEditController::createView("editor")` that returns an `IPlugView`. The rack's own GUI window owns a sub-view area and passes that area's native handle to the nested view via `IPlugView::attached(parent, type)`:

- macOS: `parent = (void*)NSView*`, `type = kPlatformTypeNSView` (`"NSView"`).
- Windows: `parent = (void*)HWND`, `type = kPlatformTypeHWND` (`"HWND"`).
- Linux: `parent = (void*)(uintptr_t)X11Window`, `type = kPlatformTypeX11EmbedWindowID`, and the host must also supply `IRunLoop` via the `FUnknown::queryInterface` path on the host-context.

Lifecycle:

```
view = controller->createView("editor")
view->setFrame(&frame)                // supply IPlugFrame
view->attached(parent, platformType)
... host window runs ...
view->removed()
view->release()
```

### 9.2 Resizing

The nested view calls `IPlugFrame::resizeView(view, newSize)`. The rack implements `IPlugFrame` and must either:
- resize its inner sub-view area and re-attach (most common), or
- reject the resize by returning `kResultFalse` (rarely appropriate).

Use `IPlugView::canResize()` and `checkSizeConstraint()` before committing to a size.

### 9.3 Multi-plugin GUI

The rack GUI will typically show a chain strip and open one nested editor at a time (tabbed), or tile them. Opening multiple views is legal but each one consumes a native sub-window and some plugins mis-behave when multiple instances of their editor exist in one process.

### 9.4 Rust-side implementation

- Use `baseview`, `raw-window-handle` + a native toolkit (Cocoa via `objc2`/`cacao`, Win32 via `windows` crate, X11 via `x11rb`) to own the top-level rack window.
- Extract a child `NSView`/`HWND`/`X11Window` for each open nested editor.
- Wire `IPlugFrame` in Rust, implementing the one method via the `vst3` crate's COM impl macros.

---

## 10. State Serialization

### 10.1 VST3 state model

Two separate state blobs:

- `IComponent::getState(IBStream*)` / `setState` — processor (DSP) state, called from UI thread (typically).
- `IEditController::getState(IBStream*)` / `setState` — controller state.

The host writes both to persistent storage. For projects, the DAW calls these on the rack when the project is saved. The rack, in turn, must call them on every nested plugin and encode the results into its own single blob.

### 10.2 Rack's state chunk

Pragmatic layout:

```
RackState v1:
  [u32 version]
  [u32 chain_len]
  for each slot:
    [u16 format]               // 0 = VST3, 1 = CLAP
    [len-prefixed string]      // plugin CID or CLAP id
    [len-prefixed string]      // plugin path (for portability)
    [len-prefixed blob]        // component getState output
    [len-prefixed blob]        // editController getState output
    [len-prefixed blob]        // CLAP plugin state output (if CLAP)
    [u32 macro_count]
    for each macro:
      [u32 rack_param_id]
      [u32 target_slot]
      [u32 target_param_id]
      [f32 range_lo, range_hi, curve]
```

Version the format from v1 and keep reading old versions forever. Use a framing format that's fast to skip and extensible (protobuf, MessagePack, or bincode with explicit versioning).

### 10.3 CLAP state

CLAP exposes `clap_plugin_state::save(stream)` / `load(stream)` with a host-provided `clap_ostream`/`clap_istream`. `clack-host` wraps this — implement a `std::io::Read`/`Write`-style adapter and feed it.

### 10.4 Threading

Both `IComponent::getState` and `clap_plugin_state.save` are **not** expected on the audio thread. Call from the main/UI thread. Do not interleave with `process()`.

---

## 11. Prior Art: Open-Source Rust Projects That Do This

Honest assessment: **there is no mature open-source Rust project that hosts VST3 or CLAP plugins inside another VST3 plugin as of April 2026.** The closest referents are:

- **`clack-host` examples** (<https://github.com/prokopyl/clack>) — a standalone CPAL-driven CLAP host, not a plugin-in-plugin. Good reference for the API.
- **`rack` crate** (0.4.8) — library for embedding plugin hosting into an application, most complete for AU on macOS.
- **`plugin_host`** crate (0.1.0, 2026-02) — scaffolding in the right shape; not functional yet.
- **`HelgeSverre/rust-vst3-host`** — experimental standalone VST3 host in Rust.
- **`nih-plug`** — plugin framework only; does not host.

The illustrative C++ prior art (read these if you're serious):

- **Ardour**: `libs/ardour/vst3_plugin.cc`, `vst3_scan.cc`, `vst3_plugin.h`. In-process, cross-platform, LGPL.
- **JUCE**'s `AudioPluginHost` example and `AudioProcessorGraph` (<https://docs.juce.com/master/classAudioProcessorGraph.html>) — the canonical in-process plugin-chain design.
- **Steinberg's `validator`** in `public.sdk/samples/vst-hosting/validator` — minimal reference host, straight from Steinberg.
- **Zrythm** — GPL DAW with native VST3/CLAP/LV2 hosting in C.
- **`prokopyl/clap-host`** — reference C++ CLAP host.

---

## 12. Licensing

### 12.1 VST3 SDK

As of **October 2025, VST3 SDK 3.8.0 is MIT-licensed**. This replaces the previous dual-license (proprietary Steinberg VST3 License or GPLv3). For a Rust project:

- You can link to the SDK under MIT. Attribution required.
- You no longer need Steinberg to sign a proprietary license agreement just to ship a closed-source product.
- Previous GPL constraints no longer apply.

### 12.2 Effect on Rust crates

- `vst3` crate (coupler-rs) ships MIT/Apache-2.0 and, since 0.3.0, vendors pre-generated bindings. No runtime SDK redistribution needed for consumers.
- `vst3-sys` is MIT/Apache-2.0.
- `vst3-bindgen` can regenerate from any SDK copy.

### 12.3 CLAP

CLAP is MIT-licensed (<https://github.com/free-audio/clap>). No licensing friction.

### 12.4 ASIO

Separately, ASIO became dual-licensed (GPLv3 or Steinberg proprietary) in October 2025. Only matters if you plan a standalone mode; inside a DAW you go through the DAW's audio driver.

### 12.5 pluginval

GPLv3. If you ship `pluginval` as a binary alongside the rack, that's fine (mere aggregation). If you link its code, you're GPL'd.

### 12.6 Verdict

No meaningful licensing barrier to a Rust VST3 + CLAP host as of April 2026, including for commercial closed-source products.

---

## 13. Loading a VST3 Bundle From Rust (Code Sketches)

Below are minimum-viable sketches using the `vst3` crate (coupler-rs 0.3.0) plus `libloading` / `core-foundation`. Error handling is elided.

### 13.1 Cross-platform module path resolution

```rust
use std::path::{Path, PathBuf};

pub fn resolve_module_binary(bundle: &Path) -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        // On macOS the bundle is Contents/MacOS/<BundleName>
        let name = bundle.file_stem()?.to_str()?;
        let p = bundle.join("Contents").join("MacOS").join(name);
        if p.exists() { return Some(p); }
    }
    #[cfg(target_os = "windows")]
    {
        let name = bundle.file_name()?.to_str()?; // e.g. "MyPlugin.vst3"
        for subdir in ["x86_64-win", "arm64ec-win", "arm64-win"] {
            let p = bundle.join("Contents").join(subdir).join(name);
            if p.exists() { return Some(p); }
        }
    }
    #[cfg(target_os = "linux")]
    {
        let stem = bundle.file_stem()?.to_str()?;
        let so = format!("{stem}.so");
        for subdir in ["x86_64-linux", "aarch64-linux"] {
            let p = bundle.join("Contents").join(subdir).join(&so);
            if p.exists() { return Some(p); }
        }
    }
    None
}
```

### 13.2 Loading the module and getting `IPluginFactory`

```rust
use libloading::{Library, Symbol};
use vst3::Steinberg::{IPluginFactory, FUnknown};
use vst3::ComPtr;
use std::ffi::{c_void, CStr};

type GetPluginFactoryFn = unsafe extern "system" fn() -> *mut IPluginFactory;
type InitDllFn          = unsafe extern "system" fn() -> bool;
type ExitDllFn          = unsafe extern "system" fn() -> bool;
type BundleEntryFn      = unsafe extern "system" fn(*mut c_void) -> bool;
type BundleExitFn       = unsafe extern "system" fn() -> bool;
type ModuleEntryFn      = unsafe extern "system" fn(*mut c_void) -> bool;
type ModuleExitFn       = unsafe extern "system" fn() -> bool;

pub struct VstModule {
    _lib: Library,               // order matters: drop factory before lib
    pub factory: ComPtr<IPluginFactory>,
    exit: Option<ExitFn>,
}

enum ExitFn { Dll(ExitDllFn), Bundle(BundleExitFn), Module(ModuleExitFn) }

pub unsafe fn load_vst3_module(binary: &std::path::Path) -> anyhow::Result<VstModule> {
    let lib = Library::new(binary)?;

    // Per-platform entry hook (must succeed or plugin may be in undefined state)
    let exit: Option<ExitFn> = {
        #[cfg(target_os = "macos")] {
            let entry: Symbol<BundleEntryFn> = lib.get(b"bundleEntry\0")?;
            if !entry(std::ptr::null_mut()) { anyhow::bail!("bundleEntry failed"); }
            let exit: Symbol<BundleExitFn> = lib.get(b"bundleExit\0")?;
            Some(ExitFn::Bundle(*exit))
        }
        #[cfg(target_os = "windows")] {
            if let Ok(init) = lib.get::<InitDllFn>(b"InitDll\0") { let _ = init(); }
            lib.get::<ExitDllFn>(b"ExitDll\0").ok().map(|s| ExitFn::Dll(*s))
        }
        #[cfg(target_os = "linux")] {
            if let Ok(entry) = lib.get::<ModuleEntryFn>(b"ModuleEntry\0") {
                if !entry(std::ptr::null_mut()) { anyhow::bail!("ModuleEntry failed"); }
            }
            lib.get::<ModuleExitFn>(b"ModuleExit\0").ok().map(|s| ExitFn::Module(*s))
        }
    };

    let get_factory: Symbol<GetPluginFactoryFn> = lib.get(b"GetPluginFactory\0")?;
    let raw = get_factory();
    if raw.is_null() { anyhow::bail!("GetPluginFactory returned null"); }
    // SAFETY: raw is a ref-counted COM pointer; wrap it without double-add-ref.
    let factory = ComPtr::from_raw(raw).ok_or_else(|| anyhow::anyhow!("null factory"))?;

    Ok(VstModule { _lib: lib, factory, exit })
}
```

Note: on macOS, the above `Library::new(binary)` opens the Mach-O directly. For strict compatibility you should instead use Core Foundation to load the CFBundle and `CFBundleGetFunctionPointerForName(bundle, "bundleEntry")`. The direct-open path works for the majority of plugins but can skip some initialization code.

### 13.3 Enumerating classes

```rust
use vst3::Steinberg::{PClassInfo, PClassInfo2, IPluginFactory2, IPluginFactory3};

unsafe fn list_classes(factory: &ComPtr<IPluginFactory>) -> Vec<PClassInfo> {
    let n = factory.countClasses() as usize;
    let mut v = Vec::with_capacity(n);
    for i in 0..n {
        let mut info = std::mem::zeroed::<PClassInfo>();
        if factory.getClassInfo(i as i32, &mut info) == vst3::Steinberg::kResultOk {
            v.push(info);
        }
    }
    v
}
```

### 13.4 Creating an `IComponent`

```rust
use vst3::Steinberg::Vst::{IComponent, IAudioProcessor};
use vst3::Steinberg::{TUID, kResultOk};

unsafe fn create_component(
    factory: &ComPtr<IPluginFactory>,
    cid: &TUID,
) -> Option<(ComPtr<IComponent>, ComPtr<IAudioProcessor>)> {
    let mut raw_component: *mut IComponent = std::ptr::null_mut();
    if factory.createInstance(
        cid.as_ptr(),
        IComponent::IID.as_ptr(),
        &mut raw_component as *mut _ as *mut *mut _,
    ) != kResultOk { return None; }
    let component = ComPtr::from_raw(raw_component)?;
    component.initialize(std::ptr::null_mut() /* host context */);

    let processor = component.cast::<IAudioProcessor>()?;
    Some((component, processor))
}
```

### 13.5 Running `process()`

Allocate `ProcessData`, `AudioBusBuffers`, `ParameterChanges`, and `EventList` once during activation; reuse per block. Call:

```rust
processor.setupProcessing(&mut setup);
component.setActive(1);       // true
processor.setProcessing(1);
// per block:
processor.process(&mut process_data);
processor.setProcessing(0);
component.setActive(0);
```

### 13.6 Loading a CLAP plugin with `clack-host`

```rust
use clack_host::prelude::*;

let bundle = PluginBundle::load("/Library/Audio/Plug-Ins/CLAP/Surge XT.clap")?;
let factory = bundle.get_plugin_factory().unwrap();
for desc in factory.plugin_descriptors() {
    println!("{}: {}", desc.id()?.to_str()?, desc.name()?.to_str()?);
}
let desc = factory.plugin_descriptors().next().unwrap();
let host_info = HostInfo::new("MyRack", "me", "example.com", "0.1")?;
let mut instance = PluginInstance::<MyHost>::new(
    |_| MyHostShared { /* ... */ },
    |_| MyHostMainThread { /* ... */ },
    &bundle,
    desc.id()?,
    &host_info,
)?;
let cfg = PluginAudioConfiguration {
    sample_rate: 48_000.0,
    min_frames_count: 1,
    max_frames_count: 1024,
};
let processor = instance.activate(|_, _, _| MyAudioProcessor, cfg)?;
// ... drive processor.process(...) on the audio thread ...
```

---

## 14. Recommended Approach for Plugin Rack

Given the April 2026 Rust ecosystem, here is the opinionated plan.

### 14.1 Foundation

- **Plugin SDK (rack-as-a-plugin)**: build the rack itself with `nih-plug` (master branch), exporting both VST3 and CLAP. This side is well-trodden.
- **VST3 hosting (inside the rack)**: depend on the **`vst3` crate (coupler-rs)** at 0.3.x. Write your own thin `Module` loader (see §13) — a few hundred lines per platform.
- **CLAP hosting (inside the rack)**: depend on **`clack-host`** at a pinned git SHA. Do not chase `main`.
- Do **not** depend on `plugin_host`, `cutoff-vst`, or `rack` yet — either not public, not mature, or not CLAP-ready.

### 14.2 Architecture

- **In-process hosting for v1.** Revisit sandboxing in v2. Ship a bypass-on-panic mechanism using `std::panic::catch_unwind` around `process()`.
- **Fixed macro-param space** of 128 slots exposed to the DAW. Map macros to nested params via a mapping table. Use `kParamTitlesChanged` for rename, avoid `kReloadComponent`.
- **Serial chain first** (slot 0 feeds slot 1 feeds slot 2). Parallel / branching in v2. Implement plugin-delay compensation between slots by summing `getLatencySamples()`.
- **One editor open at a time** in the rack GUI. A chain strip with thumbnails; double-click opens the nested view via `IPlugView::attached`.
- **Scanner runs once per launch**, caches `(path, mtime, size) -> metadata` in `~/Library/Application Support/MyRack/scan.json` (and equivalents). Respect `moduleinfo.json` when present.
- **Discovery paths**: OS defaults plus `VST3_PATH` / `CLAP_PATH` env vars.
- **Validation**: ship `pluginval` alongside or detect on PATH; validate on first discovery at level 5, cache result.
- **State**: single versioned blob written from `IComponent::getState`, containing each slot's plugin id, path, component state, controller state, macro map.

### 14.3 Risks and unknowns

- **macOS CFBundle loading**: `libloading` alone is imperfect. Plan to add a `core-foundation` code path.
- **Linux `IRunLoop`**: required for many Linux VST3 plugins' editors. Must be implemented on the rack's host-context.
- **`clack-host` churn**: pre-1.0 API breaks. Pin hard.
- **DAW quirks**: Logic's VST3 wrapper is famously strict about parameter ID stability. Never re-use IDs, even across chain edits.
- **GPL plugins**: some VST3 plugins are GPL'd. Loading them as a dylib is the classic GPL-host-hosting-GPL-plugin scenario — no problem in practice because the plugin ships as a separate binary artifact, and dlopen'ing is generally considered mere aggregation.

### 14.4 Out of scope for v1

- Out-of-process sandboxing.
- AU hosting (Mac-only anyway).
- VST2 hosting (deprecated, licensing dead end).
- LV2 hosting.
- Plugin surgery (param-range rescaling, custom curves beyond linear/log).
- Multi-bus routing. Start mono/stereo in, mono/stereo out.

### 14.5 Minimum viable deliverable

1. Build scaffold with `nih-plug` exporting a VST3 + CLAP rack.
2. VST3 bundle loader in Rust using `vst3` crate + `libloading` + (on macOS) `core-foundation`.
3. CLAP bundle loader via `clack-host`.
4. Plugin scanner + cached JSON index.
5. Serial slot chain with 8 slots, fixed-stereo, 32-bit float.
6. 128 macro params, user-assignable mapping, `kParamTitlesChanged` on rename.
7. GUI: chain strip + embedded nested editor (one at a time).
8. State round-trip: save project, close DAW, reopen — every slot restores.
9. Latency reporting: sum-of-latencies, `kLatencyChanged` on chain edit.
10. Shell-out pluginval validation with per-plugin cache.

That's a clean v1 that demonstrates every hard piece. Every feature beyond that is an extension.

---

## 15. Reference Links (Canonical)

- Steinberg VST3 SDK: <https://github.com/steinbergmedia/vst3sdk>
- Steinberg VST3 Developer Portal: <https://steinbergmedia.github.io/vst3_dev_portal/>
- VST3 Doc (API reference): <https://steinbergmedia.github.io/vst3_doc/>
- CLAP Specification: <https://github.com/free-audio/clap>
- `clack`: <https://github.com/prokopyl/clack>
- reference C++ CLAP host: <https://github.com/prokopyl/clap-host>
- `vst3` crate (coupler-rs): <https://github.com/coupler-rs/vst3-rs>, <https://crates.io/crates/vst3>
- `vst3-sys`: <https://github.com/RustAudio/vst3-sys>
- `vst3-bindgen`: <https://lib.rs/crates/vst3-bindgen>
- `rack` crate: <https://crates.io/crates/rack>
- `plugin_host` crate: <https://lib.rs/crates/plugin_host>
- `nih-plug`: <https://github.com/robbert-vdh/nih-plug>
- `pluginval`: <https://github.com/Tracktion/pluginval>
- Ardour VST3 host code: <https://github.com/Ardour/ardour/blob/master/libs/ardour/vst3_plugin.cc>, <https://github.com/Ardour/ardour/blob/master/libs/ardour/vst3_scan.cc>
- Bitwig plugin-hosting explanation: <https://www.bitwig.com/learnings/plug-in-hosting-crash-protection-in-bitwig-studio-20/>
- Ardour in-process rationale: <https://ardour.org/plugins-in-process.html>
- JUCE AudioProcessorGraph: <https://docs.juce.com/master/classAudioProcessorGraph.html>
- VST3 licensing announcement (MIT, Oct 2025): <https://www.soundonsound.com/news/steinberg-adopt-mit-license-vst3>
- moduleinfo.json spec: <https://steinbergmedia.github.io/vst3_dev_portal/pages/Technical+Documentation/VST+Module+Architecture/ModuleInfo-JSON.html>
- VST3 validator: <https://steinbergmedia.github.io/vst3_dev_portal/pages/What+is+the+VST+3+SDK/Validator.html>
- VST3 bundle format: <https://steinbergmedia.github.io/vst3_dev_portal/pages/Technical+Documentation/Locations+Format/Plugin+Format.html>
- VST3 parameters & automation: <https://steinbergmedia.github.io/vst3_dev_portal/pages/Technical+Documentation/Parameters+Automation/Index.html>
