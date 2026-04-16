# VST3 SDK Specification — Research for Plugin Rack / Mixing Console Plugin

Research compiled April 2026. Targets VST3 SDK v3.8 (current shipping version as of early 2026). All citations link to official Steinberg developer portal (`steinbergmedia.github.io/vst3_dev_portal`, `steinbergmedia.github.io/vst3_doc`), the Steinberg GitHub repositories (`steinbergmedia/vst3sdk`, `steinbergmedia/vst3_pluginterfaces`, `steinbergmedia/vst3_public_sdk`), Steinberg's own developer help, and community sources (KVR, Steinberg forums, JUCE forum, Bitwig docs, Cockos forum).

This document is organized around the eleven specific research questions for a Plugin Rack / Mixing Console plugin. Each question has a section with an SDK-level technical breakdown and a **Plugin Rack Implications** subsection that calls out what this means for our design.

---

## 1. Multiple Independent Stereo Input/Output Buses

### Can a VST3 plugin declare MULTIPLE independent stereo input and output buses?

**Yes — this is explicitly supported and has been since VST 3.0.0.** The "Multiple Dynamic I/O Support" page in the VST 3 Developer Portal is authoritative here:

> "A VST 3 plug-in can declare any desired number of busses. […] The `kMain` busses have to be placed before any other `kAux` busses in the exported busses list."
> — [Multiple Dynamic I/O Support](https://steinbergmedia.github.io/vst3_dev_portal/pages/Technical+Documentation/Change+History/3.0.0/Multiple+Dynamic+IO.html)

### Bus model — `IComponent` + `IAudioProcessor`

Buses are declared in the processor (usually in `AudioEffect::initialize()` via `addAudioInput()` / `addAudioOutput()` from the public SDK helper class). Each bus has:

- `MediaType` — `kAudio` or `kEvent`  ([ivstcomponent.h](https://github.com/steinbergmedia/vst3_pluginterfaces/blob/master/vst/ivstcomponent.h))
- `BusDirection` — `kInput` or `kOutput`
- `BusType` — `kMain` (0) or `kAux` (1)
- A `SpeakerArrangement` (64-bit bitset of speakers)
- `BusInfo::flags` — currently `kDefaultActive` (1) and `kIsControlVoltage` (2)

The canonical declaration example from Steinberg:

```cpp
addAudioInput  (STR16 ("Stereo In"), SpeakerArr::kStereo, kMain, BusInfo::kDefaultActive);
addAudioInput  (STR16 ("Mod In"),    SpeakerArr::kMono,   kAux,  0); // not default active
addAudioOutput (STR16 ("Stereo Out"),SpeakerArr::kStereo, kMain, BusInfo::kDefaultActive);
addAudioOutput (STR16 ("Aux Out"),   SpeakerArr::kStereo, kAux,  0);
addEventOutput (STR16 ("Arpeggiator"), 1, kAux, 0);
```

### `IAudioProcessor::setBusArrangements`

Signature (from [IAudioProcessor class reference](https://steinbergmedia.github.io/vst3_doc/vstinterfaces/classSteinberg_1_1Vst_1_1IAudioProcessor.html)):

```cpp
virtual tresult setBusArrangements (SpeakerArrangement* inputs,  int32 numIns,
                                    SpeakerArrangement* outputs, int32 numOuts);
```

- Called on the **UI thread** while the plug-in is in `Initialized | Connected | Setup Done` state — not during processing.
- `numIns` / `numOuts` are the **total** bus counts including aux buses, and the arrays are ordered [`kMain` buses first, `kAux` buses after].
- The plugin may:
  1. Accept completely → return `kResultTrue` after adapting its buses.
  2. Partially adapt → mutate buses to best match, return `kResultFalse`.
  3. Reject → leave arrangement unchanged, return `kResultFalse`.
- Steinberg's guidance: "requested arrangements for `kMain` buses are handled with higher priority than `kAux` buses."

The host queries the result afterward via `IAudioProcessor::getBusArrangement(BusDirection, int32 busIndex, SpeakerArrangement& out)`.

### `SpeakerArrangement` constants

Defined in `pluginterfaces/vst/vstspeaker.h` (extracted from `vsttypes.h` in SDK 3.6.9+). Relevant constants: `kEmpty` (0), `kMono`, `kStereo`, `kStereoSurround`, `kStereoCenter`, `kStereoSide`, `kStereoCLfe`, `k30Cine`, `k30Music`, `k40Cine`, `k40Music`, `k50`, `k51`, `k60Cine`, `k60Music`, `k61Cine`, `k61Music`, `k70Cine`, `k70Music`, `k71Cine`, `k71Music`, `k71Proximity`, `k80Cine`, plus ambisonic (`kAmbi1stOrderACN` through `kAmbi7thOrderACN`) and Atmos-style 7.1.2 / 7.1.4 configurations ([Speaker Arrangements group](https://steinbergmedia.github.io/vst3_doc/vstinterfaces/group__speakerArrangements.html)).

For a rack with two independent stereo signals, the natural declaration is two `kMain` stereo input buses plus two `kMain` stereo output buses — or one `kMain` + one `kAux`. Host support for multiple `kMain` inputs varies (see §2).

### State-machine constraints

The audio processor call sequence ([Audio Processor Call Sequence](https://steinbergmedia.github.io/vst3_dev_portal/pages/Technical+Documentation/Workflow+Diagrams/Audio+Processor+Call+Sequence.html)):

```
Created → Initialized → [Connected] → Setup Done → Activated → Processing
                          ^-- setBusArrangements, activateBus, setupProcessing
                                                      ^-- setActive(true)
                                                            ^-- setProcessing(true), process()
```

- `setBusArrangements` must happen **before** `setupProcessing`, always off the audio thread.
- `IComponent::activateBus(MediaType, BusDirection, int32 index, TBool state)` turns individual buses on/off; the host calls it during the Setup Done state. All buses start **inactive** — the host explicitly activates whichever it will feed. (Important corollary: the host still must provide a buffer even for inactive buses, just not meaningful data.)

### Plugin Rack Implications

- A rack plugin is absolutely free to expose multiple independent stereo input buses. The VST3 spec permits this.
- **But spec support ≠ host support.** Test matrix must cover Bitwig, Reaper, Cubase, Studio One, Live, Pro Tools AAX wrapper, and Logic-via-AU separately — behavior diverges wildly (§2).
- For symmetry with CLAP and good host behavior, declare the "primary" pair as `kMain` + `kMain` and any further stereo inputs as `kAux`. Put all `kMain` buses first in the declaration order; that's a hard spec requirement.
- Mark only the channels actually needed as `kDefaultActive`. Allowing the host to leave "Track B" input deactivated until the user wires it up reduces CPU and works around hosts that barf on unused inputs.
- Expose at least one `bypass` parameter (`ParameterInfo::kIsBypass`) regardless — Steinberg strongly recommends this for effects.

---

## 2. How DAWs Expose Auxiliary Buses on VST3 Plugins

There is **no standard host-level UX** for "route Track B's audio into Track A's plugin." The VST3 SDK only defines the plug-in side of the contract. Host wiring is DAW-specific, and the *actual* set of buses a host will show differs dramatically.

### Cubase / Nuendo (Steinberg — the reference host)

- Full support for `kMain` + `kAux` buses. Side-chain from any channel via sends.
- Cubase Nuendo added *multiple* sidechain inputs in v11. For VST3 plug-ins: "Side-chains are also very easily realizable, and this includes the possibility to deactivate unused busses after loading and even reactivate those when needed." ([Steinberg Forum — Signal Sidechains in VST3i](https://forums.steinberg.net/t/signal-sidechains-in-vst3i-not-all-hosts-support-it/736622))
- Known quirk: "Cubase seems to always use the default channel count for sidechains no matter what the input/output configuration is." ([JUCE Forum — Issues with sidechain channel configuration in VST3 / Cubase](https://forum.juce.com/t/issues-with-sidechain-channel-configuration-in-vst3-cubase/38134))
- Multi-out groups: to feed a plugin a quad signal you historically had to make a Quadro group track; with VST3 kAux that workaround disappears.

### Bitwig (probably our primary target)

- Supports VST3 sidechain on both effects and instruments.
- **But limited to 2 stereo pairs today**: "audio routing into VSTs is currently limited to 2 stereo pairs: stereo pair 1/2 (default) and stereo pair 3/4, which can be accessed using the VST's Sidechain Input option." ([KVR — Routing more than 4 channels of audio to plugins](https://bitwish.top/t/routing-more-than-4-channels-of-audio-to-plugins/614), [KVR — VST Audio Inputs](https://www.kvraudio.com/forum/viewtopic.php?t=515542))
- Modulators and Audio Sidechain modulator are the Bitwig-native alternative to multi-input DSP. ([Bitwig — Sidechaining Tutorial](https://www.bitwig.com/learnings/sidechaining-tutorial-49/))
- Bitwig does support per-note and sample-accurate automation per the VST3 spec. ([Bitwig Studio 2 announcement](https://www.bitwig.com/stories/bitwig-studio-2-221/))

### Reaper

- Very flexible — Reaper exposes the VST3 bus pin connector as a matrix. Users can route anything to any bus channel via the *Plug-in pin connector*. Tracks have up to 64 channels and Reaper will wire them through plugin buses at the host's discretion.
- "Ports/Buses beyond #1 won't work in REAPER unless it's VST3." MIDI buses on VST3 are mappable one-to-one. ([admiralbumblebee — Reaper routing](https://www.admiralbumblebee.com/music/2017/04/18/Reapers-Amazing,-but-Awful,-Almost-Anything-to-Anywhere-Routing.html))
- Reaper is the *least* opinionated about "where Track B audio goes" — users wire it up manually. Good for our power-user audience.

### Ableton Live

- Live 12 improved VST3 sidechain handling: "smoother sidechain integration with third-party plugins." ([Icon Collective — Sidechaining in Ableton 12](https://www.iconcollective.edu/sidechaining-in-ableton-12-a-comprehensive-guide))
- Only one sidechain input is typically exposed (the first `kAux`). Live does not expose multiple `kMain` inputs beyond the first.
- Live supports Drum Rack / Instrument Rack chains but those are Live-native containers — no VST3 reflection.

### Studio One (PreSonus)

- Shows sidechain inputs for **audio-effect VST3s only, not instrument VST3s**. ([Steinberg Forum — Signal Sidechains in VST3i](https://forums.steinberg.net/t/signal-sidechains-in-vst3i-not-all-hosts-support-it/736622))
- Plug-in category matters: `"Fx"` vs `"Fx|Instrument"` affects whether aux inputs appear.

### Logic Pro (via AU — VST3 is not native)

- Logic has no VST3 runtime. Users use AU-VST3 wrappers (e.g., [AU-VST3-Wrapper](https://github.com/ivicamil/AU-VST3-Wrapper)) or commercial tools (Plogue Bidule, DDMF Metaplugin).
- Logic's AU model: "Logic usually uses Input bus 0 as the AU input and the next input bus as a side chain input." ([Universal Audio — External Sidechain FAQ](https://help.uaudio.com/hc/en-us/articles/18479403068820-FAQ-How-do-I-use-external-sidechain-in-my-DAW))
- Via a good AU wrapper, `kAux` → Logic side-chain drop-down. Multi-input beyond one sidechain is flaky.

### Pro Tools (AAX wrapper)

- Not a VST3 host; irrelevant unless we ship AAX separately. Sidechain works via `kAux`-style auxiliary input when translated by the AAX wrapper in the VST3 SDK.

### The actual "standard way"

There is no standard. Every host chose its own convention:

| Host | Aux input exposure | Multi-`kMain` | Notes |
|---|---|---|---|
| Cubase/Nuendo | Side-chain menu per-bus | Yes (v9+) | Reference host |
| Bitwig | "Sidechain Input" dropdown | Limited (pair 1/2 + 3/4) | 2 stereo pairs max in 2026 |
| Reaper | Pin connector matrix | Yes, unlimited | Most flexible |
| Live 12 | First `kAux` as sidechain | No | Improved in 12 but capped |
| Studio One | Sidechain only on FX type | No | Instrument plugins can't receive sidechain |
| Logic (AU) | AU sidechain menu | No | Via wrapper |

### Plugin Rack Implications

- Design for **Bitwig-first** (since that's the likely primary use case) and test fallbacks on the four big hosts.
- Beyond two stereo pairs, don't rely on host routing — expose *internal* routing inside our own UI. The rack's own UI must be able to route anything anywhere regardless of what the host exposes.
- Ship a stereo-pair-aware "expose N inputs" mode that the user selects at load, writing that into the plug-in state. On hosts with broken multi-bus support we can fall back to 1 stereo in / 1 stereo out plus internal routing with no functional loss.
- Warn users in the UI if the host is a known weak multi-bus citizen (Studio One, Live pre-12 for instrument sidechain, etc.) — detect via `IHostApplication::getName` (see §6).
- For "route Track B into our plugin" as a first-class feature, our best bet is:
  1. Expose 2 `kMain` stereo inputs, fall back to `kMain`+`kAux` if host rejects.
  2. For a third+ input, ship a companion "Rack Bridge" plugin the user inserts on Track B that shares memory (discoLink-style IPC — see §6). This is the only portable way to exceed host multibus limits.

---

## 3. Sidechain and Multi-Sidechain

### Single sidechain

Canonical pattern: one `kMain` audio input + one `kAux` audio input marked non-default-active. The `kAux` bus is what every host recognizes as "the sidechain." Hosts universally interpret the first `kAux` audio input as the side-chain signal.

### Multi-sidechain

**There is no VST3-defined standard for "multi-sidechain."** The spec lets you declare N `kAux` input buses; the host decides whether to surface each as a routable source.

- Cubase (v11+) surfaces each `kAux` input individually in the side-chain routing dialog.
- Reaper surfaces them all via the pin connector; the user wires them manually.
- Bitwig only wires pair 1/2 + pair 3/4 as of Bitwig 5.x in 2026.
- Live, Studio One, Logic-via-AU: single sidechain, additional `kAux` buses typically unreachable.

There's a community feature-request ([KVR — Multiple sidechain input](https://www.kvraudio.com/forum/viewtopic.php?t=552244)) but no SDK change. The workaround sometimes used: declare one wide `kAux` (e.g. 7.1.4 = 12 channels) and interpret internally. This lets hosts that support multi-channel sidechain (Reaper, Cubase with pin routing) deliver multiple "virtual" sidechains through a single bus, at the cost of losing per-source naming and host UI clarity.

### Plugin Rack Implications

- Expose *at least* one named `kAux` stereo input for traditional sidechain compatibility (every host that supports VST3 sidechain at all will see this).
- If we need more sidechain sources internally, either:
  - Declare multiple `kAux` buses and accept that only Cubase/Reaper will expose them, **or**
  - Declare one wide `kAux` bus (e.g., stereo x 4 = 8-ch, or use `k71Music`) and document that "sidechain 1 = ch 1-2, sidechain 2 = ch 3-4, …" in the manual.
- Second option is often pragmatically better: every sidechain-capable host surfaces a single multi-channel aux.
- Make all aux buses `kDefaultActive = 0`. They should light up only when the host connects a source.
- Provide an internal routing matrix in the rack UI so the user can assign "aux input N" to any nested plugin's sidechain input.

---

## 4. Parameter Count and Dynamic Parameter Lists

### Maximum parameter count

Up to **2^31 parameters** (2,147,483,647) — parameter IDs are `int32` in the range [0, 2^31 − 1]. The parameter ID space is reserved for the plugin; certain upper ranges are reserved for host use (specifically for MIDI-CC parameters via `IMidiMapping`). ([Parameters & Automation](https://steinbergmedia.github.io/vst3_dev_portal/pages/Technical+Documentation/Parameters+Automation/Index.html))

> "Up to 2^31 parameters can be exported with ID range [0, 2.147.483.647]."

In practice:
- Real-world hosts handle hundreds to low thousands fine.
- Some hosts have performance pathologies above ~10,000 parameters (parameter browser UI, automation lane indexing).
- JUCE's `AudioProcessorValueTreeState` has been tested with thousands; the bottleneck becomes host-side, not plugin-side.

### `ParameterInfo` flags

```cpp
enum ParameterFlags {
  kCanAutomate            = 1 << 0,  // enable host automation
  kIsReadOnly             = 1 << 1,  // plugin-driven only; excludes kCanAutomate
  kIsWrapAround           = 1 << 2,  // wraps instead of clamps
  kIsList                 = 1 << 3,  // discrete list in generic editor
  kIsHidden               = 1 << 4,  // hidden from generic editor (3.7+)
  kIsProgramChange        = 1 << 15, // program selector
  kIsBypass               = 1 << 16  // canonical bypass parameter
};
```

Values are always normalized to `[0.0, 1.0]` (double). Host automation records/plays the normalized value.

### Dynamic parameter lists — `restartComponent` flags

`IComponentHandler::restartComponent(int32 flags)` is the mechanism the plugin uses to tell the host "things changed." Flags (from [`ivsteditcontroller.h`](https://github.com/steinbergmedia/vst3_pluginterfaces/blob/master/vst/ivsteditcontroller.h)):

| Flag | Bit | Meaning |
|---|---|---|
| `kReloadComponent` | 1<<0 | Whole component must be regenerated — fullest reset |
| `kIoChanged` | 1<<1 | Bus count / configuration changed |
| `kParamValuesChanged` | 1<<2 | Multiple parameter values shifted (rescan values) |
| `kLatencyChanged` | 1<<3 | `getLatencySamples()` result changed |
| `kParamTitlesChanged` | 1<<4 | Titles / default / flags / step count changed (rescan `ParameterInfo`) |
| `kMidiCCAssignmentChanged` | 1<<5 | MIDI mapping changed |
| `kNoteExpressionChanged` | 1<<6 | Note-expression info changed |
| `kIoTitlesChanged` | 1<<7 | Bus *names* changed |
| `kPrefetchableSupportChanged` | 1<<8 | Prefetch capability changed |
| `kRoutingInfoChanged` | 1<<9 | Internal audio/event routing changed |
| `kKeyswitchChanged` | 1<<10 | Keyswitch info/count changed |
| `kParamIDMappingChanged` | 1<<11 | Parameter-ID mapping (3.7+) |

**Adding/removing parameters at runtime:** use `kReloadComponent`. This is the heaviest option — in several hosts the audio engine will stall briefly; Cubase/Bitwig re-scan the entire parameter list; automation lanes may reset. Real-world plugin devs mostly avoid this except at load or when the user explicitly reconfigures.

**Renaming existing parameters:** `kParamTitlesChanged`. Known buggy in several hosts: Steinberg's AUv3 wrapper "resets all parameter values when `restartComponent(kParamTitlesChanged)` is called" ([vst3_public_sdk issue #45](https://github.com/steinbergmedia/vst3_public_sdk/issues/45)). Reaper's VST3 implementation has historical issues ([Cockos Forum — restartComponent(kParamTitlesChanged) doesn't work correctly](https://forum.cockos.com/showthread.php?t=265989)). JUCE VST3 wrapper for a long time never updated the parameter table after title changes ([JUCE Forum — VST3 parameter name changes don't work](https://forum.juce.com/t/vst3-parameter-name-changes-dont-work/28313)).

All `restartComponent` calls **must be on the UI thread** (not the audio thread). Typical pattern: detect change in `process()`, queue a flag, call `restartComponent` from the edit-controller's UI timer.

### Parameter changes within a process block

Parameters are automated via `IParameterChanges` in `ProcessData`:
- `ProcessData::inputParameterChanges` — host → plugin (sample-accurate automation points)
- `ProcessData::outputParameterChanges` — plugin → host (for tied automation, e.g., knob recorded to automation lane)

Each `IParamValueQueue` has N points (sampleOffset, normalizedValue) within the block. This is how Bitwig delivers sample-accurate automation.

### Plugin Rack Implications

- A rack holding arbitrary nested plugins needs to **expose nested plugins' parameters to the outer host**. Strategies:
  1. **Pre-allocate a large pool** (e.g., 2048 generic parameters) and dynamically bind them to nested-plugin parameters. Use `kParamTitlesChanged` to rename when binding. Upside: no `kReloadComponent`. Downside: buggy in JUCE-wrapper-era Reaper/AUv3; test carefully. *This is the standard pattern* — Blue Cat PatchWork, Kushview Element, Plogue Bidule all do this.
  2. **Use `kReloadComponent` on every rack change** — cleaner model but disruptive. Nobody loves this.
  3. **Only expose a fixed set of "macro" parameters** (16 or 32) that the user assigns to nested params. Cleanest host behavior, limited for power users.
- The answer is almost certainly (1) + (3): 8–16 "macro" parameters that are always visible + named, plus a pool of 512-2048 generic slots that rebind via `kParamTitlesChanged`.
- Never call `restartComponent` from `process()` — queue it and fire from the UI timer.
- Do not exceed ~4096 parameters in the pool if we care about Studio One and some older hosts' parameter-browser UIs.

---

## 5. Modulation and Host Modulation (Bitwig-style)

### VST3 has no native "modulation" concept distinct from automation

VST3 exposes only *parameters*. Every knob-like thing the host can drive is a parameter with values in [0, 1]. The VST3 spec has **no per-parameter modulation depth** like CLAP's `clap_event_param_mod`. ([Parameters & Automation](https://steinbergmedia.github.io/vst3_dev_portal/pages/Technical+Documentation/Parameters+Automation/Index.html))

### How Bitwig's modulation works over VST3

Bitwig's Unified Modulation System ([Bitwig User Guide — Unified Modulation System](https://www.bitwig.com/userguide/latest/the_unified_modulation_system/)) treats VST parameter modulation as *automation writes*. When a modulator drives a VST parameter:

- Bitwig computes the instantaneous value (base + modulator delta), clamped to [0,1]
- Bitwig writes it to the VST3 parameter via an `IParamValueQueue` automation point
- Bitwig throttles VST parameter updates to **approximately one per 64 samples** ([CLAP Issue #60 — audio-rate automation flag](https://github.com/free-audio/clap/issues/60)):
  > "Bitwig currently sends only one parameter update every 64 samples when using modulators, presumably because very few plugins actually use audio-rate automation."

So from the VST3 plugin's perspective it's indistinguishable from automation — and it's *not sample-accurate modulation*; it's sub-audio-rate (48 kHz / 64 = 750 Hz).

Bitwig *does* support sample-accurate VST3 automation for recorded automation lanes — both via multiple `IParamValueQueue` points per block and via per-note expression ([Bitwig Studio 2](https://www.bitwig.com/stories/bitwig-studio-2-221/)). The 64-sample cap applies specifically to modulators because modulators are polled, not event-driven.

### CLAP contrast (for context)

CLAP adds `CLAP_EVENT_PARAM_MOD` as a first-class modulation offset event distinct from `CLAP_EVENT_PARAM_VALUE` — non-destructive, returns to base after the event, supports per-voice modulation. This is the feature VST3 lacks that drives Bitwig's interest in CLAP. For a plugin-rack plugin specifically, CLAP modulation would let per-voice modulation flow through nested instruments — that's simply not possible via pure VST3.

### Plugin Rack Implications

- Inside the rack, we can implement our own modulation engine freely — the outer host (Bitwig) only sees parameter automation.
- For exposing rack-level "modulation source X → nested param Y" wiring to the host, there's no way to express "host modulator → rack-internal modulator slot" as anything other than ordinary parameter automation. That's fine — our rack's LFOs/envelopes/MIDI are internal DSP.
- If we want to offer sample-accurate internal modulation between our rack macros and nested plugin params, we bypass the VST3 automation path — compute modulations locally every sample and write values directly to the nested plugin's parameter values (not via its own VST3 `IParamValueQueue` unless we respect its update rules). This sidesteps the 64-sample Bitwig throttle.
- A CLAP build of the rack plugin (using the same DSP core) can expose per-voice modulation properly. VST3 build cannot. Budget for both formats.

---

## 6. Multi-Instance Linking Between Plugin Instances

### VST3 has no standardized inter-instance communication

- `IConnectionPoint` ([class ref](https://steinbergmedia.github.io/vst3_doc/vstinterfaces/classSteinberg_1_1Vst_1_1IConnectionPoint.html)) is defined only for "separate components" — specifically the `IComponent` (processor) and `IEditController` (UI) of the **same** plug-in instance. The host may place a proxy between them. It's not intended for cross-instance messaging and most hosts won't wire it that way.
- `IMessage` + `IAttributeList` ([class ref](https://steinbergmedia.github.io/vst3_doc/vstinterfaces/classSteinberg_1_1Vst_1_1IAttributeList.html)) carry key/value payloads over connection points but again, same-plug-in only.
- `ContextMenu` / `IContextMenu` is UI-only, not a data channel.
- There is **no** standardized IPC API in VST3.

The Steinberg FAQ on [Communication](https://steinbergmedia.github.io/vst3_dev_portal/pages/FAQ/Communication.html) confirms: "messages from the processor to the controller must not be sent during the process call." Nothing in the spec addresses processor-to-processor (different instance) messaging.

### Real-world techniques plugin developers use

1. **Shared memory / file-mapped IPC.** The standard approach. [discoLink](https://github.com/reales/discolink) is an open-source example: lock-free ring buffers, per-channel up to 16,384 samples, cache-line aligned atomics, discovery via a 16-slot shared "bulletin board." It's a real audio-rate plugin bridge.
2. **Static state / singletons** inside the plugin binary. Works because the host usually loads one copy of the `.vst3` bundle and instantiates multiple `IComponent` objects from the same module. Caveat: some hosts sandbox each instance in a separate subprocess (Live's plug-in sandboxing, Bitwig's crash protection). Singletons break across subprocesses.
3. **Host-level routing tricks.** E.g., a "send" plugin on one track writes to shared memory; a "receive" plugin on another track reads. This is how ReaStream, Voxengo VST Sound Receive, and similar tools work.
4. **Named pipes / domain sockets** for non-audio-rate messages (preset sync, UI state).
5. **mDNS / loopback TCP** for cross-process (some "DAW link" plugins).

### Bitwig crash protection and sandboxing

Bitwig runs VST2/VST3 in separate processes by default ([Bitwig — Plug-in Hosting & Crash Protection](https://www.bitwig.com/learnings/plug-in-hosting-crash-protection-in-bitwig-studio-20/)). This **breaks static-singleton inter-instance linking.** Shared memory via POSIX shm / Windows named file-mapping still works across Bitwig's subprocesses.

### Plugin Rack Implications

- For a true "plugin rack" where one rack instance holds all the nested plugins, inter-instance linking is mostly moot: all the nested plugins live in one rack process.
- **If we want multiple rack instances to communicate** (share a modulation bus, gang parameter values, provide "send/receive" between tracks), we need shared-memory IPC. Reference implementation: discoLink. Expect ~1–5 ms extra latency in the receiver chain.
- Detect Bitwig sandboxing and ensure our shm segments live in OS-global namespace (not process-private). On macOS use `/tmp/` file-backed mmap or POSIX `shm_open`. On Windows use `CreateFileMapping` with a global name.
- Identify the host via `IHostApplication::getName()` (obtained from `IPluginBase::initialize`'s context query for `FUnknown::queryInterface(IHostApplication::iid)`). Switch feature paths per host.
- For the rack, IPC can be kept *simple*: cross-instance features are a bonus, not a core requirement. The core is the nested-plugin scheduler inside one process.

---

## 7. Editor (GUI) Sizing, HiDPI, and Resize

### `IPlugView` (in `pluginterfaces/gui/iplugview.h`)

Key methods ([IPlugView class reference](https://steinbergmedia.github.io/vst3_doc/base/classSteinberg_1_1IPlugView.html)):

```cpp
tresult isPlatformTypeSupported (FIDString type);
tresult attached (void* parent, FIDString type);
tresult removed ();
tresult onWheel (float distance);
tresult onKeyDown (char16 key, int16 keyCode, int16 modifiers);
tresult onKeyUp   (char16 key, int16 keyCode, int16 modifiers);
tresult getSize (ViewRect* size);
tresult onSize (ViewRect* newSize);
tresult onFocus (TBool state);
tresult setFrame (IPlugFrame* frame);
tresult canResize ();
tresult checkSizeConstraint (ViewRect* rect);
```

### Platform type constants

- Windows — `kPlatformTypeHWND`
- macOS — `kPlatformTypeNSView` (Cocoa) and legacy `kPlatformTypeHIView` (Carbon — deprecated; don't use in 2026)
- Linux — `kPlatformTypeX11EmbedWindowID`
- iOS — `kPlatformTypeUIView`

### `IPlugFrame` (host-provided)

```cpp
tresult resizeView (IPlugView* view, ViewRect* newSize);
```

### Resize call sequences

([Resize View Call Sequence](https://steinbergmedia.github.io/vst3_dev_portal/pages/Technical+Documentation/Workflow+Diagrams/Resize+View+Call+Sequence.html))

**Plug-in-initiated resize (user clicks "Large" button):**
1. `IPlugView → IPlugFrame::resizeView(newSize)`
2. Host calls `IPlugView::getSize()` (returns *old* size)
3. Host resizes its frame
4. Host calls `IPlugView::onSize(newSize)`
5. Plug-in repaints at new size
6. Host may re-query `getSize()` (now returns *new* size)

**Host-initiated resize (user drags window edge):**
1. Host calls `IPlugView::checkSizeConstraint(proposedRect)`
2. Plug-in may mutate the rect (snap to aspect ratio / minimum size)
3. Host resizes its frame to the corrected rect
4. Host calls `IPlugView::onSize(finalRect)`
5. Plug-in repaints

### `IPlugViewContentScaleSupport` (HiDPI)

Added in VST3 3.6.6 ([PlugView Content Scaling](https://steinbergmedia.github.io/vst3_dev_portal/pages/Technical+Documentation/Change+History/3.6.6/IPlugViewContentScaleSupport.html)):

```cpp
tresult setContentScaleFactor (ScaleFactor factor);
```

- Called directly before/after `attached`, and on scale change (e.g., drag window between screens).
- When the plug-in handles it (returns `kResultTrue`), the plug-in must scale its logical width/height by the factor and notify via `IPlugFrame::resizeView(scaledRect)`.
- Designed primarily for Windows, where the window system doesn't tell you the screen DPI; macOS and Linux can discover it via platform APIs.

### macOS / Retina specifics

On macOS, "most plugins assume `getSize`, `onSize`, `checkSizeConstraint` etc. operate in logical pixels" — differs from Windows/Linux which operate in physical pixels. The `backingScaleFactor` of the containing `NSWindow` (usually 2.0 on Retina) handles the HiDPI mapping automatically when the plug-in uses layer-backed views. `viewDidChangeBackingProperties:` fires on cross-screen moves. ([Apple — APIs for Supporting High Resolution](https://developer.apple.com/library/archive/documentation/GraphicsAnimation/Conceptual/HighResolutionOSX/APIs/APIs.html))

In practice: if you're using VSTGUI, or JUCE's `Component`, HiDPI on macOS "just works" because AppKit backs it. On Windows you must honor `setContentScaleFactor`. On Linux (X11 via xembed), HiDPI is often inferred from `XRESOURCES`/`GDK_SCALE` — implementation-defined.

### Fluid resize and infinite-loop pitfalls

There's a well-known host/plugin resize feedback loop documented in [Ardour PR #599](https://github.com/Ardour/ardour/pull/599): if a plug-in calls `resizeView` inside its own `onSize` handler, the host may call `onSize` again, ad infinitum. Mitigation: guard with a re-entrancy flag; accept the host's `onSize` rect rather than re-requesting.

### Plugin Rack Implications

- A mixing-console/rack UI is going to be large. Design for **fluid resize** with aspect-ratio constraints — `canResize() → kResultTrue`; `checkSizeConstraint()` enforces a sensible min (maybe 800×500) and aspect ratio if we have one.
- HiDPI: if we use VSTGUI or JUCE, implement `IPlugViewContentScaleSupport` and handle Windows scale factors. On Retina, our coordinates are logical and AppKit handles the backing store.
- Save the user's last window size in state (`IEditController::getState`) and restore in `attached`; the first `onSize` the host expects might not arrive, so seed size via `getSize` in `canResize`.
- Nested plugin views: when a user opens a nested plugin's custom UI inside our rack, host-side resize doesn't compose well. Two options:
  1. Open the nested plugin's view in a **separate host window** (VST3 generic editor style). Simplest.
  2. Reparent the nested view's `NSView`/`HWND`/X11 window inside our own view. More DAW-like but HiDPI, scaling, and macOS view hierarchy get messy. Precedent: Blue Cat PatchWork, Bidule, Kushview Element all do this — it's tractable.
- Test on Bitwig, Reaper, Cubase, Live, and Studio One for resize quirks. Studio One has historical bugs around `checkSizeConstraint`; Live has a floating-plugin-window model that sometimes mis-reports the parent size.
- Watch the re-entrant resize loop — add a re-entrancy guard.

---

## 8. Buffer Size / Block Size Negotiation

### `ProcessSetup` struct

Delivered via `IAudioProcessor::setupProcessing(ProcessSetup&)`:

```cpp
struct ProcessSetup {
  int32  processMode;           // kRealtime | kPrefetch | kOffline
  int32  symbolicSampleSize;    // kSample32 | kSample64
  int32  maxSamplesPerBlock;    // ceiling for any single process() call
  double sampleRate;            // e.g., 48000.0
};
```

### Rules

([Processing FAQ](https://steinbergmedia.github.io/vst3_dev_portal/pages/FAQ/Processing.html))

- `setupProcessing` is called while the plug-in is **inactive** — safe to allocate here.
- `maxSamplesPerBlock` is a *maximum*. Each `process()` call passes its own `ProcessData::numSamples` which can be any value ∈ [1, maxSamplesPerBlock].
- The host must call this sequence to change block size or sample rate while the plugin is running:
  ```
  setProcessing(false) → setActive(false) → setupProcessing(newSetup) → setActive(true) → setProcessing(true)
  ```
  This means the plug-in gets a clean disable/re-setup — no mid-flight buffer resizing.
- **The plugin does not negotiate block size with the host.** Block size is host-imposed. The plugin can refuse the sample size (via `canProcessSampleSize(kSample32 | kSample64)`), but not the block count.
- `ProcessModes`:
  - `kRealtime` — standard (audio thread)
  - `kPrefetch` — host may call faster than realtime (bounce-in-place)
  - `kOffline` — offline render (audio-export); can be slower-than-realtime for quality

### Known quirks

- Some hosts call `setupProcessing` with an optimistic `maxSamplesPerBlock` then actually feed larger blocks in rare edge cases. Defensive plugin code uses `std::max(maxSamplesPerBlock, numSamples)` at allocation time and re-allocates if necessary — but allocation in `process()` is forbidden. Safer: allocate a generous margin (e.g., 2× `maxSamplesPerBlock`).
- Wavelab has been reported to pass inconsistent `maxSamplesPerBlock` values ([Steinberg Forum — Inconsistent maxSamplesPerBlock for WAVELAB 10](https://forums.steinberg.net/t/inconsistent-maxsamplesperblock-for-wavelab-10/202050)).
- AUv3 wrapper imposes additional block-size constraints via AudioUnit render block.

### Plugin Rack Implications

- All nested plugins inside our rack must be driven with the *same* block size as the outer host gives us (otherwise latency compounds). Simplest: call each nested plugin's `setupProcessing` with the same `maxSamplesPerBlock` and `sampleRate` we received.
- If we want **sub-block control** (e.g., to run internal modulation at a higher rate), we can internally sub-divide our block before dispatching to nested plugins — but hosts, including Bitwig, sometimes give us already-small blocks (64 samples at 48 kHz). Over-subdividing costs CPU.
- Pre-allocate nested-plugin I/O buffers at `setupProcessing` time. Our rack must honor the real-time-safe contract in `process()` — zero allocations, zero locks.
- If a nested plugin's `getLatencySamples()` changes between blocks, that's a `kLatencyChanged` restart. See §11.
- Save `maxSamplesPerBlock` and `sampleRate` in state *only* for sanity-checking; never use them to drive DSP behavior — always trust the latest `setupProcessing`.

---

## 9. Preset / State Management

### Two states, two streams

VST3 separates processor and controller state ([Presets & Program Lists](https://steinbergmedia.github.io/vst3_dev_portal/pages/Technical+Documentation/Presets+Program+Lists/Index.html)):

- `IComponent::getState(IBStream*)` / `IComponent::setState(IBStream*)` — the **processor** state (DSP params, internal state)
- `IEditController::getState(IBStream*)` / `IEditController::setState(IBStream*)` — the **controller** state (UI-only settings like window size)
- `IEditController::setComponentState(IBStream*)` — host calls this after loading a preset to sync the controller's parameter mirror to the processor state

### `.vstpreset` file format

([Preset Format](https://steinbergmedia.github.io/vst3_dev_portal/pages/Technical+Documentation/Locations+Format/Preset+Format.html))

```
Header:
  4 bytes   'VST3' magic
  4 bytes   version (int32, little-endian)
  32 bytes  class ID (ASCII, plugin's UID)
  8 bytes   offset to chunk list (int64)
Data area:
  <component state bytes><controller state bytes><metadata>
Chunk list (at tail):
  count, then (id, offset, size) triples for:
    'Comp' — component state
    'Cont' — controller state
    'Info' — XML metadata (author, category, …) (optional)
    'List' — chunk list itself
```

### `IBStream` interface

Sequential read/write stream with `read`, `write`, `seek`, `tell`. Plug-ins write arbitrary binary; most implementations use a little-endian versioned format:

```
[uint32 version][uint32 paramCount][for each: uint32 id, double normalizedValue][variable extra data...]
```

### State for program-list-style plug-ins

Optional interfaces:
- `IUnitInfo` — exposes plug-in's internal unit tree
- `IProgramListData` — lets the host read/write individual programs from the plug-in's pool

Most effects/mixers don't need this. It's for multi-timbral instruments with large preset banks.

### Nested plugin state serialization (our core question)

VST3 does not describe nested plugins. We must invent a format. Standard approach for plugin-rack plugins (as used by Blue Cat PatchWork, Kushview Element, Bidule):

```
Rack state chunk:
  uint32  rack-format-version
  uint32  nested-plugin-count
  for each nested plugin:
    uint32   class-id-length
    bytes    class-id (VST3 UID as string / TUID)
    uint32   host-family (VST3 / AU / CLAP / LV2 / …)
    uint32   state-length
    bytes    plugin-component-state (opaque, from nested's getState)
    uint32   controller-state-length
    bytes    plugin-controller-state
    [routing info, enabled flag, custom metadata…]
  [rack-level connections, modulation, macros, etc.]
```

For each nested plugin, call its own `IComponent::getState` / `IEditController::getState` and embed the bytes opaquely. On load, instantiate the plugin via its class ID (looked up through a cached `IPluginFactory` from the host's module), then call `setState` / `setComponentState`.

### Plugin Rack Implications

- Bump a top-level `rack-format-version` on every schema change; old rack presets must still load (at least with best-effort).
- Nested plug-in class IDs must be stored exactly (they're 16-byte TUIDs). Record the **plug-in name and vendor** too as a fallback / user-facing readout when the plug-in is missing.
- If a nested plug-in is missing on load, don't silently drop — show an error and preserve the opaque state bytes so re-installing the plug-in restores everything.
- For cross-format racks (saving a VST3 rack, opening the same rack in our CLAP or AU build), serialize the plug-in family and identifier format-neutrally. This is a *we-define* problem.
- `IEditController::getState` should only hold UI-only data (window size, which nested plugin tab is open). Anything DSP-related goes in `IComponent::getState` so it survives the round-trip without needing `setComponentState`.
- VST3 presets use the `.vstpreset` wrapper — we get this "free" if we serialize through `IComponent::getState`. No extra format work.
- Test: opening a rack preset across multiple host DAWs. Bitwig, Cubase, Reaper store `getState` output differently (inline vs external `.vstpreset`), so make sure the bytes are stable regardless of wrapper.

---

## 10. Process Context — Tempo, Time Signature, Transport Sync

### `ProcessContext` struct

Delivered in `ProcessData::processContext` every `process()` call ([ProcessContext struct ref](https://steinbergmedia.github.io/vst3_doc/vstinterfaces/structSteinberg_1_1Vst_1_1ProcessContext.html)).

```cpp
struct ProcessContext {
  uint32     state;                    // bitfield of validity + transport flags
  double     sampleRate;               // always valid
  TSamples   projectTimeSamples;       // always valid (loop-aware)
  int64      systemTime;               // nanoseconds since epoch (if kSystemTimeValid)
  TSamples   continousTimeSamples;     // non-loop-aware sample position (if kContTimeValid)
  TQuarterNotes projectTimeMusic;      // quarter-note position (if kProjectTimeMusicValid)
  TQuarterNotes barPositionMusic;      // downbeat position (if kBarPositionValid)
  TQuarterNotes cycleStartMusic;       // loop start
  TQuarterNotes cycleEndMusic;         // loop end
  double     tempo;                    // BPM (if kTempoValid)
  int32      timeSigNumerator;         // (if kTimeSigValid)
  int32      timeSigDenominator;
  Chord      chord;                    // host-provided chord info (if kChordValid)
  int32      smpteOffsetSubframes;     // (if kSmpteValid)
  FrameRate  frameRate;                // SMPTE rate
  int32      samplesToNextClock;       // MIDI clock distance (if kClockValid)
};
```

### State flags

`ProcessContext::state` is a bitfield combining **transport flags** and **validity flags**:

Transport:
- `kPlaying` (1<<1) — transport is running
- `kCycleActive` (1<<2) — loop active
- `kRecording` (1<<3) — recording

Validity (tells you which optional fields are filled):
- `kSystemTimeValid`, `kContTimeValid`, `kProjectTimeMusicValid`, `kBarPositionValid`, `kCycleValid`, `kTempoValid`, `kTimeSigValid`, `kChordValid`, `kSmpteValid`, `kClockValid`

Always check the validity bit before reading the corresponding field.

### Host behaviors

- Cubase/Nuendo sets everything reliably.
- Reaper sets most flags; `kChordValid` rarely.
- Bitwig sets transport + tempo + time sig + bar position reliably; `kSmpteValid` is off except when synced externally.
- Live sets transport + tempo + time sig; `kContTimeValid` is sometimes off.
- Logic AU-wrapped: same as AU `HostCallback`.

### Plugin Rack Implications

- Rack-level features that need tempo: LFO sync, tempo-synced delays, beat-locked automation. Drive from `ProcessContext::tempo` + `projectTimeMusic`. Snapshot at block start.
- Pass `ProcessContext` through to each nested plugin's own `ProcessData::processContext` — **don't synthesize** new context per-nested-plugin. Nested plugins expect the host's authoritative timeline.
- If our rack does look-ahead (e.g., pre-roll for a compressor's side-chain), we delay the nested plugin's perceived time by our lookahead amount. That changes `projectTimeSamples` — handle by reporting the look-ahead as latency (`getLatencySamples`, §11) so the host compensates project time automatically. Don't mutate `projectTimeSamples` passed to the nested plugin.
- Guard all reads with validity flags; never assume `tempo` is non-zero.
- For transport-aware features (freeze on stop, reset on play), watch `kPlaying` edges block-to-block. Store last-state in the processor.

---

## 11. Latency Reporting and Nested Plugin Latency

### Declaration

`IAudioProcessor::getLatencySamples()` returns the plug-in's inherent processing delay in samples. Host uses this for Plug-in Delay Compensation (PDC) — it aligns tracks so that a latent plug-in on Track A still renders in phase with a non-latent Track B.

### Changing latency at runtime

When latency changes (e.g., we add a nested plug-in that has 128 samples of inherent lookahead):

1. Plug-in calls `IComponentHandler::restartComponent(kLatencyChanged)` from the UI thread.
2. Host stops the audio engine briefly, re-queries `getLatencySamples()`.
3. Host recomputes delay compensation for all tracks.
4. Audio resumes.

> "If the plug-in reports a latency change using `IComponentHandler::restartComponent(kLatencyChanged)`, this could lead to audio playback interruption because the host has to recompute its internal mixer delay compensation."
> — [Reporting latency change](https://forums.steinberg.net/t/reporting-latency-change/201601)

This is audible — a glitch, a brief silence, or a stutter depending on host.

### Nested-plugin latency accumulation

If our rack holds:
- Plug-in A with 32-sample latency (e.g., a linear-phase EQ with small buffer)
- Plug-in B with 256-sample latency (e.g., a look-ahead limiter)
- Plug-in C with 0-sample latency (e.g., saturation)

In **serial**: total latency = 32 + 256 + 0 = **288 samples** plus any rack-internal buffering.

In **parallel** (A and B in parallel sub-chains that merge back): the total is the max(A, B) = 256, *but* all parallel paths must be delayed to the longest path's latency to stay in phase. So we add a 224-sample delay to the A path to align with B's 256. The rack reports 256.

More complex topologies (sidechain feeding a parallel chain) require a per-bus latency graph. This is the core DSP bookkeeping problem for any plug-in rack.

### Querying nested plug-in latency

Each time a nested plug-in instance is configured (post-`setupProcessing`, pre-`setActive(true)`), call its `getLatencySamples()`. Store. Recompute the rack's effective latency. If the rack's latency changed vs. the last reported value, call `restartComponent(kLatencyChanged)` from our UI thread.

### Catches

- Nested plug-in latency can change mid-session (user switches a plug-in's "mode" knob → linear-phase vs. minimum-phase). Plug-in calls *its own* `restartComponent(kLatencyChanged)`. Since we're its host, we receive that call via the `IComponentHandler` we gave it — we recompute and propagate up.
- Causing `kLatencyChanged` too frequently will wreck the user experience. Rate-limit; if a plug-in toggles latency every parameter tweak, consider freezing the rack's reported latency at a maximum until the user explicitly "commits" the chain.
- If we use lookahead internally for modulation or sidechain timing, include it.

### Plugin Rack Implications

- Implement a **latency graph compiler** that walks the rack topology and computes per-path latency, inserts compensation delays on shorter paths, and returns the total latency to report to the outer host.
- Recompile the graph whenever:
  - A nested plug-in's `getLatencySamples()` changes (we get the `kLatencyChanged` notification).
  - The user adds/removes a plug-in.
  - The user changes routing (serial↔parallel, sidechain connections).
  - The buffer size or sample rate changes (`setupProcessing` re-called).
- De-bounce calls to `restartComponent(kLatencyChanged)` — coalesce to at most once per ~100 ms. The user will not notice a 100 ms delay in latency reporting, but hosts *will* notice back-to-back restart events.
- Inform the user visually that latency changed (our UI can show "Rack latency: 1024 samples / 21.3 ms @ 48 kHz"). Surfaces a core plug-in-rack UX expectation.
- Expose a "latency report" mode where we lie to the host and report 0 — gives instant UI feedback but misaligns tracks. Kushview Element and some others have this as a power-user override.

---

## Cross-Cutting Design Notes

### Host detection (useful everywhere)

Query the context passed to `IPluginBase::initialize(FUnknown* context)` for `IHostApplication`:

```cpp
FUnknownPtr<IHostApplication> host(context);
String128 name{};
host->getName(name);
```

Known strings: `"Cubase"`, `"Nuendo"`, `"Bitwig Studio"`, `"REAPER"`, `"Ableton Live"`, `"Studio One"`, `"Wavelab"`. Use this to switch host-specific workarounds.

### Threading discipline

- **Audio thread** only: `IAudioProcessor::process`, `setProcessing` transitions.
- **UI thread** for everything else — `setBusArrangements`, `setupProcessing`, `setActive`, `IComponentHandler::beginEdit/performEdit/endEdit/restartComponent`, view attach/remove.
- Never allocate, lock, block, or call OS IO in `process()`.

### Bitness and API versions

VST3 is x86_64 and arm64 on macOS (universal binary), x86_64 and arm64 on Windows 11, x86_64 and arm64 on Linux. The 3.8 SDK (current in early 2026) retains backward compat with 3.6+ plug-ins. There is no "VST3 legacy" split — it's one ABI.

### CLAP parallel strategy

Given Bitwig's continued push of CLAP, and the rack use case being exactly the kind of thing CLAP's polyphonic modulation helps, plan for a CLAP build from day one. VST3 and CLAP share enough conceptual surface (parameters, buses, state) that the DSP core and UI code can be format-agnostic; wrappers are thin.

---

## Source Index

Primary (Steinberg docs, authoritative):

- [VST 3 Developer Portal](https://steinbergmedia.github.io/vst3_dev_portal/)
- [VST 3 API Documentation — Index](https://steinbergmedia.github.io/vst3_dev_portal/pages/Technical+Documentation/API+Documentation/Index.html)
- [IAudioProcessor class reference](https://steinbergmedia.github.io/vst3_doc/vstinterfaces/classSteinberg_1_1Vst_1_1IAudioProcessor.html)
- [IComponent class reference / VST busses group](https://steinbergmedia.github.io/vst3_doc/vstinterfaces/group__vstBus.html)
- [Multiple Dynamic I/O Support (3.0.0 change notes)](https://steinbergmedia.github.io/vst3_dev_portal/pages/Technical+Documentation/Change+History/3.0.0/Multiple+Dynamic+IO.html)
- [Parameters & Automation](https://steinbergmedia.github.io/vst3_dev_portal/pages/Technical+Documentation/Parameters+Automation/Index.html)
- [Audio Processor Call Sequence](https://steinbergmedia.github.io/vst3_dev_portal/pages/Technical+Documentation/Workflow+Diagrams/Audio+Processor+Call+Sequence.html)
- [Resize View Call Sequence](https://steinbergmedia.github.io/vst3_dev_portal/pages/Technical+Documentation/Workflow+Diagrams/Resize+View+Call+Sequence.html)
- [IPlugViewContentScaleSupport (3.6.6 change notes)](https://steinbergmedia.github.io/vst3_dev_portal/pages/Technical+Documentation/Change+History/3.6.6/IPlugViewContentScaleSupport.html)
- [Presets & Program Lists](https://steinbergmedia.github.io/vst3_dev_portal/pages/Technical+Documentation/Presets+Program+Lists/Index.html)
- [Preset Format](https://steinbergmedia.github.io/vst3_dev_portal/pages/Technical+Documentation/Locations+Format/Preset+Format.html)
- [Complex Plug-in Structures (multi-timbral)](https://steinbergmedia.github.io/vst3_dev_portal/pages/Technical+Documentation/Complex+Structures/Index.html)
- [Processing FAQ](https://steinbergmedia.github.io/vst3_dev_portal/pages/FAQ/Processing.html)
- [Communication FAQ](https://steinbergmedia.github.io/vst3_dev_portal/pages/FAQ/Communication.html)
- [ProcessContext struct reference](https://steinbergmedia.github.io/vst3_doc/vstinterfaces/structSteinberg_1_1Vst_1_1ProcessContext.html)
- [Speaker Arrangements group](https://steinbergmedia.github.io/vst3_doc/vstinterfaces/group__speakerArrangements.html)
- [IPlugView class reference](https://steinbergmedia.github.io/vst3_doc/base/classSteinberg_1_1IPlugView.html)
- [IPlugFrame class reference](https://steinbergmedia.github.io/vst3_doc/base/classSteinberg_1_1IPlugFrame.html)
- [IComponentHandler class reference](https://steinbergmedia.github.io/vst3_doc/vstinterfaces/classSteinberg_1_1Vst_1_1IComponentHandler.html)
- [IConnectionPoint class reference](https://steinbergmedia.github.io/vst3_doc/vstinterfaces/classSteinberg_1_1Vst_1_1IConnectionPoint.html)
- [IAttributeList class reference](https://steinbergmedia.github.io/vst3_doc/vstinterfaces/classSteinberg_1_1Vst_1_1IAttributeList.html)
- [ivsteditcontroller.h on GitHub (RestartFlags)](https://github.com/steinbergmedia/vst3_pluginterfaces/blob/master/vst/ivsteditcontroller.h)
- [ivstcomponent.h on GitHub](https://github.com/steinbergmedia/vst3_pluginterfaces/blob/master/vst/ivstcomponent.h)
- [ivstaudioprocessor.h on GitHub](https://github.com/steinbergmedia/vst3_pluginterfaces/blob/master/vst/ivstaudioprocessor.h)
- [vst3_public_sdk repo](https://github.com/steinbergmedia/vst3_public_sdk)

Secondary / Community:

- [Steinberg Forum — Signal Sidechains in VST3i, not all hosts support it](https://forums.steinberg.net/t/signal-sidechains-in-vst3i-not-all-hosts-support-it/736622)
- [Steinberg Forum — Reporting latency change](https://forums.steinberg.net/t/reporting-latency-change/201601)
- [Steinberg Forum — How to use restartComponent](https://forums.steinberg.net/t/how-to-use-restartcomponent-and-which-flags-are-the-right-one-when-changing-all-characteristics-parameters-except-size/202031)
- [Steinberg Forum — Inconsistent maxSamplesPerBlock for WAVELAB 10](https://forums.steinberg.net/t/inconsistent-maxsamplesperblock-for-wavelab-10/202050)
- [Steinberg Forum — IPlugView ContentScaleSupport and macOS](https://forums.steinberg.net/t/iplugview-contentscalesupport-and-macos/930318)
- [Steinberg Forum — How to launch several instances in same process](https://forums.steinberg.net/t/how-to-launch-several-instances-of-same-plugin-in-same-process/909152)
- [KVR Forum — Bitwig multiple sidechain input](https://www.kvraudio.com/forum/viewtopic.php?t=552244)
- [KVR Forum — Bitwig VST3 missing sidechain](https://www.kvraudio.com/forum/viewtopic.php?t=603810)
- [KVR Forum — Bitwig VST Audio Inputs](https://www.kvraudio.com/forum/viewtopic.php?t=515542)
- [KVR Forum — Inter-plugin communication sending info between instances](https://www.kvraudio.com/forum/viewtopic.php?t=595782)
- [KVR Forum — Audio rate modulation of VST parameters (Bitwig)](https://www.kvraudio.com/forum/viewtopic.php?t=540750)
- [KVR Forum — VST3: assuming constant buffer size or not?](https://www.kvraudio.com/forum/viewtopic.php?t=596481)
- [Bitwig User Guide — Unified Modulation System](https://www.bitwig.com/userguide/latest/the_unified_modulation_system/)
- [Bitwig Learnings — Sidechaining Tutorial](https://www.bitwig.com/learnings/sidechaining-tutorial-49/)
- [Bitwig Learnings — Plug-in Hosting & Crash Protection](https://www.bitwig.com/learnings/plug-in-hosting-crash-protection-in-bitwig-studio-20/)
- [Bitwig Studio 2 announcement (sample-accurate automation)](https://www.bitwig.com/stories/bitwig-studio-2-221/)
- [Bitwish — Routing more than 4 channels of audio to plugins](https://bitwish.top/t/routing-more-than-4-channels-of-audio-to-plugins/614)
- [CLAP Issue #60 — audio-rate automation/modulation flag](https://github.com/free-audio/clap/issues/60)
- [JUCE Tutorial — Configuring the right bus layouts for your plugins](https://juce.com/tutorials/tutorial_audio_bus_layouts/)
- [JUCE Forum — Getting multiple buses working for AU and VST3](https://forum.juce.com/t/getting-multiple-buses-working-for-au-and-vst3/60078)
- [JUCE Forum — Different Multichannel Formats with Reaper VST3](https://forum.juce.com/t/different-multichannel-formats-with-reaper-vst3/56831)
- [JUCE Forum — Issues with sidechain channel configuration in VST3 / Cubase](https://forum.juce.com/t/issues-with-sidechain-channel-configuration-in-vst3-cubase/38134)
- [JUCE Forum — VST3 parameter name changes don't work](https://forum.juce.com/t/vst3-parameter-name-changes-dont-work/28313)
- [JUCE Forum — VST3 parameter updates: automation vs host refresh](https://forum.juce.com/t/vst3-parameter-updates-automation-vs-host-refresh/67373)
- [JUCE Forum — Multibus and Speaker Arrangement in VST3](https://forum.juce.com/t/multibus-and-speaker-arrangement-in-vst3-something-needs-to-be-done/28491)
- [Cockos Forum — VST3 restartComponent(kParamTitlesChanged) doesn't work correctly](https://forum.cockos.com/showthread.php?t=265989)
- [Ardour PR #599 — VST3 prevent resize loops](https://github.com/Ardour/ardour/pull/599)
- [admiralbumblebee — Reaper's Routing](https://www.admiralbumblebee.com/music/2017/04/18/Reapers-Amazing,-but-Awful,-Almost-Anything-to-Anywhere-Routing.html)
- [discoLink GitHub — cross-plugin shared memory IPC](https://github.com/reales/discolink)
- [AU-VST3-Wrapper GitHub](https://github.com/ivicamil/AU-VST3-Wrapper)
- [Apple — APIs for Supporting High Resolution on OS X](https://developer.apple.com/library/archive/documentation/GraphicsAnimation/Conceptual/HighResolutionOSX/APIs/APIs.html)
- [Universal Audio — External sidechain FAQ (Logic/Cubase/etc.)](https://help.uaudio.com/hc/en-us/articles/18479403068820-FAQ-How-do-I-use-external-sidechain-in-my-DAW)
- [Blue Cat Audio Blog — VST3 Plug-ins Released: So What?](https://www.bluecataudio.com/Blog/new-releases/vst3-plug-ins-released-what-for/)
- [Ableton Live — Sidechaining a third-party plug-in](https://help.ableton.com/hc/en-us/articles/209775325-Sidechaining-a-third-party-plug-in)
- [Icon Collective — Sidechaining in Ableton 12](https://www.iconcollective.edu/sidechaining-in-ableton-12-a-comprehensive-guide)
- [Sound on Sound — Multi-channel analyser in Cubase](https://www.soundonsound.com/sound-advice/q-how-do-set-multi-channel-analyser-cubase)
