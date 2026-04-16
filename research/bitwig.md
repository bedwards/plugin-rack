# Bitwig Studio VST3 Hosting — Research Notes

Target release window: Bitwig Studio 5.x (5.1–5.3 stable, Jan 2025) and 6.0
(beta Oct–Nov 2025, GA planned around March 2026). All findings below refer to
this generation unless noted otherwise.

This document answers the eleven research questions posed by the plugin-rack
project, plus a "Hard constraints" section and a "Workaround space" section at
the end.

---

## 1. Can ONE VST3 plugin instance in Bitwig own TWO independent tracks' audio as separate strips?

**Short answer: no, not in any officially supported way.**

Bitwig's architecture is track-centric: every plug-in instance lives inside
exactly one "device chain" which is attached to exactly one track (an instrument
track, an audio track, an FX track, a group/bus track, or a nested chain inside
a container device). There is no concept of a plug-in instance that
simultaneously appears on multiple tracks or that "owns" multiple track strips.

The closest Bitwig gets to multi-track plug-in ownership is the MULTI-OUTPUT
model, which works in the OPPOSITE direction (one instance driving many
destinations) and is read-only from the perspective of those destinations:

### 1.1 Multi-output VST3 in Bitwig

- Load a multi-out VST (e.g. Kontakt, Omnisphere, VCV Rack, BFD) on an
  instrument track. Click the double-arrow icon in the top of the plug-in panel
  to reveal multi-out chain buttons. Each output becomes an "internal chain"
  inside the plug-in's host device, with its own mixer strip (Multi-out chain
  mixer).
  https://www.bitwig.com/support/technical_support/how-do-i-use-multi-out-vst-plug-ins-27/
- Once chains exist, they can be addressed from other tracks: on any
  destination track's audio input chooser — or from an Audio Receiver device's
  SOURCE menu — pick the host track, then the `Chains` submenu, then the
  desired output.
  https://www.bitwig.com/support/technical_support/how-do-i-use-multi-out-vst-plug-ins-27/
- Crucially this is ONE instance broadcasting to many strips; the destination
  tracks consume the audio but do not route anything back into the plug-in.
  They cannot present their own "strip" of parameters, their own sidechain
  feed, their own note input, etc. They are passive audio sinks.
- Multi-out works for VST2, VST3, CLAP, and AU. VST3 plug-ins declare their
  output bus layout and Bitwig surfaces each bus as a chain.

### 1.2 Multi-input VST is severely constrained

- Bitwig caps hosted plug-in audio INPUTS at **2 stereo pairs** (4 channels
  total): main stereo input (pair 1/2) and one stereo sidechain (pair 3/4).
- Anything above 4 input channels declared by the plug-in is silently unused —
  this is a well-known limit for VCV Rack Pro, Plogue Bidule, etc.
- Users have filed wishlist entries asking for multichannel tracks and for the
  ability to individually address VST audio inputs; no ETA.
  https://bitwish.top/t/routing-more-than-4-channels-of-audio-to-plugins/614
  https://bitwish.top/t/multichannel-tracks/2588
  https://www.kvraudio.com/forum/viewtopic.php?t=515542
- Even Bitwig's sidechain UI only supports ONE sidechain source per plug-in;
  "multiple sidechain inputs" is a standing request.
  https://www.kvraudio.com/forum/viewtopic.php?t=552244

### 1.3 Sidechain routing

- The VST sidechain input is accessed via a small sidechain icon at the top of
  the plug-in header. Only plug-ins that declare a sidechain bus show this.
  The dropdown lists all project tracks (pre- or post-fader).
  https://www.bitwig.com/learnings/sidechaining-tutorial-49/
- Audio Sidechain MODULATOR is separate and uses the track's amplitude as a
  modulation source; it does NOT deliver audio samples to the plug-in — it
  produces a scalar modulation signal.
- Known bug: prior to fixes, Bitwig would change the plug-in's sidechain bus
  configuration WHILE the plug-in was still activated, violating the VST3 spec
  that buses may only be enabled/disabled while the component is deactivated.
  This caused JUCE plug-ins to miss prepareToPlay/setupProcessing after
  sidechain toggling; use `processorLayoutsChanged()` instead of
  `prepareToPlay()` to detect sidechain bus count changes.
  https://forum.juce.com/t/bitwig-vst3-preparetoplay-not-called-after-sidechain-activation/50883

### 1.4 Track groups / buses

- A Group track in Bitwig IS a bus. Cmd/Ctrl+Shift+G creates one; dragging
  tracks in adds them.
  https://www.kvraudio.com/forum/viewtopic.php?t=522328
- The group's OWN device chain processes the summed bus audio. A plug-in on a
  group track processes the combined stereo bus — not multiple independent
  strips. The child tracks are summed before they reach the group's plug-ins.
- Therefore a group does NOT give you "one plug-in on two tracks as separate
  strips"; it gives you "one plug-in on one summed stereo bus."

### 1.5 Container devices (Chain, FX Layer, Instrument Layer, FX Selector, Instrument Selector, Replacer, Drum Machine)

All container devices live INSIDE a single track's device chain — they cannot
span tracks. They do offer internal parallelism:

- **Chain**: serial wrap around a sub-chain with a dry/wet Mix. Useful as a
  preset package but is still one stereo signal path.
  https://www.bitwig.com/userguide/latest/container/
- **FX Layer**: N parallel audio chains, each with its own mixer strip. The
  SAME input audio is copied to each parallel chain. You cannot drive chain 1
  from track A and chain 2 from track B in a single instance of FX Layer; both
  chains see whatever audio arrives at the FX Layer's input (though each
  internal chain could host its own Audio Receiver to pull from elsewhere).
- **Instrument Layer**: N parallel instrument chains, each with its own mixer.
  Incoming notes trigger every layer.
- **FX Selector / Instrument Selector / Note FX Selector**: exactly ONE child
  chain active at a time, crossfade on switch. Drives a single audio/note
  output.
- **Replacer**: analyzes incoming audio level, generates MIDI notes to trigger
  a nested instrument chain (used for drum replacement). Single-track.
  https://www.bitwig.com/userguide/latest/container/
- **Drum Machine**: up to 128 instrument chains triggered by different MIDI
  notes. Single-track; still a polyphonic instrument, not a multi-track owner.
- **Instrument + Audio FX chaining** inside an instrument device: every
  instrument has a "Note FX" pre-chain and an "FX" post-chain slot. Again,
  these live on the single host track.
  https://www.bitwig.com/userguide/latest/advanced_device_concepts/

### 1.6 Could a VST3 plug-in declare multiple MAIN input buses and get multi-track audio?

No. VST3 permits multiple input buses, but Bitwig only feeds the first stereo
MAIN bus and the first stereo AUX (sidechain) bus. Extra declared buses are
ignored. There is no UI to map "my track 17" → "plug-in's 3rd input bus."

The wishlist for individually addressable VST audio inputs is unanswered.
https://bitwish.top/t/routing-more-than-4-channels-of-audio-to-plugins/614

### 1.7 The only workaround families

(Expanded in §"Workaround space" below.) Summary:

1. Run ONE plug-in instance on track A, use Audio Receivers on track B to
   re-ingest the plug-in's audio (or a specific multi-out chain of it) — but
   track B cannot influence the plug-in state; it is a read-only sink.
2. Use container `Chain`/`FX Layer` INSIDE one track with Audio Receivers
   pulling audio from other tracks into the parallel sub-chains — still one
   track from the project's POV.
3. Use "By plug-in" hosting mode so multiple INSTANCES share a sub-process and
   can use the plug-in's own inter-instance IPC (Komplete Kontrol, iZotope
   Relay, etc.). Still multiple instances, not one.

**Conclusion:** A single VST3 plug-in instance owning two project-level track
strips is not representable in Bitwig. The audio engine model is strictly
track-scoped. This is a hard architectural constraint for any "rack" design
that wants a single shared plug-in host to expose multiple independent strips.

---

## 2. Bitwig modulators + VST3 params: mapping, targeting, dynamic params

### 2.1 How modulators target VST3 parameters

Bitwig's modulation system is fully unified across native devices and VST2/VST3/
CLAP plug-ins. Workflow:

1. Open the device view for the plug-in.
2. Click the small arrow at the bottom of the device to open the modulator
   panel.
3. Add a modulator (LFO, ADSR, Audio Sidechain, Envelope Follower, Macro,
   Random, Steps, ParSeq-8, Expressions, Vector-4, XY, etc.).
4. Enter "modulation routing" mode on the modulator (click the `+` / plug
   icon). Now click and drag on ANY exposed parameter knob in the plug-in's
   parameter pane — the drag value sets the maximum modulation depth (bipolar,
   ±, or curve-scaled).
5. Exit routing mode. The modulator is now wired; the parameter value still
   responds to direct edits (Bitwig preserves manual parameter control
   alongside modulation).
   https://www.bitwig.com/userguide/latest/the_unified_modulation_system/

This works on any VST3 parameter the plug-in exposes via `IEditController`
enumeration. The plug-in must have declared the parameter through
`getParameterInfo` and it must not be marked as a meta-parameter whose role
forbids modulation (kIsBypass, kIsProgramChange, kIsReadOnly have special
handling).

### 2.2 What Bitwig exposes per plug-in

- A searchable, filterable list of every exposed parameter (can be filtered
  to "automated", "modulated", or "all").
- A "joker knob" that always maps to the last-touched parameter.
- Per-preset-instance Remote Control pages (8 knobs per page). VSTs don't
  ship with preset remote pages — users must build & save them.
  https://www.bitwig.com/userguide/latest/vst_plug-ins/
  https://www.kvraudio.com/forum/viewtopic.php?t=537009
- Automation lanes for any parameter, displaying native units in VST3/CLAP
  (Bitwig 6.0). VST2 still displays 0–100% because VST2 has no formatted-value
  callback.
  https://downloads.bitwig.com/6.0/Release-Notes-6.0.html

### 2.3 Dynamic parameters / renaming

- Bitwig supports VST3's `IComponentHandler::restartComponent` with
  `kParamValuesChanged` and `kParamTitlesChanged` and `kReloadComponent` flags.
  Parameter titles, info, and value lists can be updated at runtime; Bitwig
  will re-read them.
- However: a long-standing gotcha is that a plug-in calling
  `IComponentHandler::performEdit` with a parameter ID that the host has not
  yet registered will generate an "invalid parameter ID" warning in
  `engine.log`. Typical cause: plug-ins that edit parameters during
  `setState()` before Bitwig has scanned the post-setState parameter list.
  Fix on the plug-in side: do not call performEdit while applying state; the
  host re-scans after setState anyway.
  https://forum.juce.com/t/bitwig-invalid-parameter-id-message-vst3/50253
- Bitwig will RE-SCAN the parameter list when the plug-in requests it, but the
  UI bindings (modulators, automation lanes, Remote Control mappings) are
  keyed by parameter ID. If a parameter ID disappears from the list, any
  modulator/automation bound to it becomes orphaned (visually greyed). If the
  ID reappears, bindings reconnect.
- VCV Rack VST3 had a well-documented bug for years where modules loaded with
  parameters reset to 0 in Bitwig because of dynamic-parameter timing issues
  during state restore; fixed on VCV's side (Dec 2022). Similar pattern for
  other "shell" plug-ins that generate their parameter list on the fly.
  https://community.vcvrack.com/t/vst3-and-bitwig-modules-loading-with-parameters-set-to-zero/18830
- Practical advice: if a plug-in renames a parameter at runtime (e.g. "Macro
  1" → "Cutoff") Bitwig will pick up the new name on the next `restartComponent
  (kParamTitlesChanged)` call. Existing modulator routings survive; only the
  display label changes.

### 2.4 Per-voice / polyphonic modulation to VST3

- Bitwig modulators are color-coded: **blue = mono**, **green = poly**. Per-
  voice can be toggled via right-click → Per-Voice.
- Polyphonic modulators only produce truly per-voice signals INSIDE a
  polyphonic native context (Poly Grid, Instrument Layer with per-voice
  voicing, voice-stack contexts).
- When a polyphonic modulator targets a non-polyphonic device — this includes
  every VST3 and CLAP plug-in that does not support CLAP's
  `clap_note_ports + per-voice parameter modulation` — Bitwig SUMS the per-
  voice modulation values into a single monophonic signal. VST3 has no
  per-voice parameter channel, so all VST3 params get the monophonic sum.
  https://www.bitwig.com/userguide/latest/the_unified_modulation_system/
- CLAP is the only format that gets true per-voice parameter modulation from
  Bitwig (via `clap_event_param_mod` with `note_id`/`key`/`channel`).
  https://www.bitwig.com/stories/clap-the-new-audio-plug-in-standard-201/
- MPE (per-note pitch bend, pressure, timbre/CC74) IS passed through to VST3
  plug-ins that declare per-note controllers. Right-click plug-in header →
  "Force MPE Mode" to enable in stubborn cases.
  https://www.bitwig.com/userguide/latest/vst_plug-in_handling_and_options/

### 2.5 Limits / caveats

- Modulator routings store target parameter by plug-in-internal ID, not by
  name. Renaming is fine; re-ordering the parameter list is fine. Removing a
  parameter orphans the routing.
- Bitwig cannot modulate parameters that the plug-in doesn't expose to the
  host (e.g. internal macros that aren't in the `IEditController` list). If
  you need to modulate something hidden, you must ask the plug-in vendor to
  expose it.
- A single modulator can drive many parameters; a single parameter can be
  targeted by many modulators. They sum (with per-routing depth, polarity,
  and curve).

---

## 3. Buffer size exposure to VST3 plug-ins

### 3.1 setupProcessing / max samples

- Bitwig calls VST3 `IAudioProcessor::setupProcessing()` with a ProcessSetup
  struct containing `sampleRate`, `maxSamplesPerBlock`, `processMode`
  (kRealtime / kPrefetch / kOffline), and `symbolicSampleSize`. The plug-in
  learns the UPPER BOUND of block size here.
- Bitwig's user-configurable audio buffer size is the transport buffer (32,
  64, 128, 256, 512, 1024, 2048 samples). This value is communicated as
  `maxSamplesPerBlock`.
- Bitwig internally splits processing at sample-accurate automation and
  modulation points, so the ACTUAL per-`process()` block can be SMALLER than
  `maxSamplesPerBlock` (see §4 on variable block sizes).
- Bitwig's internal devices (and Grid) have a **minimum block size of 32
  samples** — below that, non-linear processing is disabled. This is Bitwig
  internal; VST3 plug-ins are still called with whatever the host-determined
  block size is.

### 3.2 Can a VST3 plug-in QUERY current buffer size?

- VST3 provides `getProcessContext()` each `process()` call which carries
  transport info (tempo, position, SMPTE, bar info) — but NOT current block
  size. The current block size is simply `data.numSamples` on the
  `ProcessData` argument to `process()`.
- There is no VST3 API for a plug-in to poll the host's configured buffer
  size. The plug-in learns the max from `setupProcessing()` and the current
  from `ProcessData::numSamples`.
- Bitwig also does NOT expose the raw audio-device buffer size through any
  VST3 extension. Plug-ins must derive it from observed block sizes.

### 3.3 Can a plug-in SUGGEST a preferred buffer?

- VST3 has no standard "please set buffer size to N" call from plug-in to
  host. `IAudioProcessor::canProcessSampleSize()` only negotiates 32-bit vs
  64-bit float.
- Neither VST3 nor Bitwig supports plug-in-initiated buffer-size changes. A
  plug-in can INTERNALLY buffer and process at its preferred size by
  introducing latency (reported via `getLatencySamples`) — this is the
  standard approach.
- Changing Bitwig's audio-engine buffer size RE-INITIALIZES the audio
  engine and reloads every plug-in (well-known user complaint).
  https://www.kvraudio.com/forum/viewtopic.php?t=596390

### 3.4 ProcessContextRequirements

- VST3 lets a plug-in declare which ProcessContext fields it actually needs
  via `IProcessContextRequirements`. Bitwig honors this correctly in 5.x+.
- Since 5.1, Bitwig also honors a plug-in's realtime-requirement declaration
  (i.e. "I must run in realtime, don't call me from a non-RT bounce worker").
  https://downloads.bitwig.com/5.1.1/Release-Notes-5.1.1.html

---

## 4. Sample rate, variable block sizes, freewheeling (render)

### 4.1 Sample rate

- Bitwig's sample rate is set in the Dashboard's Audio settings (44.1/48/88.2/
  96/176.4/192 kHz; driver-dependent).
- Sample rate changes cause the audio engine to reinit (plug-ins reload).
- Bitwig passes sampleRate to VST3 plug-ins via `setupProcessing`. Standard.
- Audio samples in the project are auto-resampled to project rate on playback.

### 4.2 Variable block sizes

- Bitwig uses **sub-block processing** to honor sample-accurate automation
  and sample-accurate modulation. A single audio callback of, say, 512
  samples may be split into multiple `process()` calls whenever a parameter
  change or modulator waypoint lands mid-block.
- This means VST3 plug-ins see block sizes ranging from 1 sample up to
  `maxSamplesPerBlock`. Plug-ins MUST be tolerant of any block size ≤ max.
- Bitwig's internal DSP minimum of 32 samples does not apply to hosted
  plug-ins; they can get smaller blocks.
- Implication for latency-compensated DSP: use `IAudioProcessor::getLatency`
  and let the host manage PDC — don't try to batch inside a block.

### 4.3 Freewheel / offline render

- Bitwig's "Export Audio" and "Bounce in Place" use offline (freewheeling)
  rendering. VST3 `processMode` is set to `kOffline`.
- Known issue: plug-ins that have an "offline rendering oversampling" mode
  (e.g. some analog-modeled plug-ins) can mis-report latency during offline
  switching, causing timing/phasing errors in Bitwig exports. Workaround: don't
  change the plug-in's oversampling mode between realtime and offline.
  https://www.kvraudio.com/forum/viewtopic.php?t=524441
- Bitwig 5.1 fixed PDC updates when activating a plug-in for offline rendering
  results in a new latency.
  https://downloads.bitwig.com/5.1.1/Release-Notes-5.1.1.html
- Some users report ~50% of renders are silent unless rendered realtime, but
  this traces back to antivirus / sandboxing / plug-in isolation race
  conditions rather than Bitwig itself.
- Bitwig does NOT support plug-ins that switch their internal processing
  shape between realtime and freewheel modes without also reporting latency
  accurately.

### 4.4 Bounce / render scope

- Bitwig 5.1+ bounces only the track's prerequisites (sidechain sources,
  send sources) rather than the entire project, speeding up freewheel renders
  and reducing side-effects.

---

## 5. Plug-in GUI scaling (HiDPI, zoom)

### 5.1 OS-level HiDPI

- macOS: plug-in windows are handled by the native NSView/CAOpenGLLayer.
  Retina scaling is automatic if the plug-in is built with HiDPI awareness.
  Bitwig does not override OS scaling on macOS.
- Windows: Bitwig is DPI-aware. VST3 plug-ins that implement
  `IPlugViewContentScaleSupport` receive the correct scale factor from the
  host.
  https://steinbergmedia.github.io/vst3_dev_portal/pages/Technical+Documentation/Change+History/3.6.6/IPlugViewContentScaleSupport.html
- Linux: HiDPI is the weakest on this platform; plug-in windows (especially
  JUCE-based VST3) may not resize correctly without closing/reopening the
  editor.
  https://forum.juce.com/t/linux-vst3-resize-host-window-does-not-change-the-size/44212

### 5.2 Bitwig's "Stretch Plug-in Window to match DPI"

- In Preferences → Plug-ins, Bitwig exposes a global "Stretch Plug-in Window
  to match DPI" toggle. It can also be set PER-PLUGIN: right-click the plug-in
  in the browser → "Stretch Plug-in Window to match DPI".
- When enabled, Bitwig bitmap-stretches the plug-in's native window to match
  the user's OS DPI scaling. This is a dumb bilinear stretch — great for tiny
  legacy plug-ins on a 4K monitor, ugly for well-written HiDPI plug-ins.
- Default is ON for fresh plug-in installs. Users should DISABLE for
  well-maintained u-he/Arturia/FabFilter/etc. plug-ins that handle HiDPI
  themselves, otherwise text gets blurry.
  https://www.kvraudio.com/forum/viewtopic.php?t=608500
  https://www.kvraudio.com/forum/viewtopic.php?t=582715

### 5.3 Any zoom factor beyond OS HiDPI?

- No — Bitwig does NOT provide a per-plug-in zoom/scale slider like some DAWs
  do (e.g. Reaper's plug-in UI zoom). The only knobs are the global OS DPI
  setting and the per-plug-in stretch toggle.
- Plug-ins that implement their own internal scale factor (u-he, Serum,
  FabFilter) continue to work through their own menus; Bitwig does not
  interfere.

### 5.4 Plug-in-initiated resize

- VST3 `IPlugFrame::resizeView` is honored. The plug-in can ask the host for
  any new size; Bitwig will resize the outer window.
- Some Linux-specific resize bugs have been reported — closing and reopening
  the editor restores correct size.

---

## 6. Native containers as prior art: The Grid, FX Chain, Replacer, Chain

Bitwig's own containers are the best reference for what a "rack" looks like
inside Bitwig.

### 6.1 Chain (simple serial container)

- A single serial sub-chain with a global dry/wet Mix knob and a 25-second
  "Wet Gain Learn" helper.
- Used to bundle multiple devices into one presettable block, move them
  together, apply a shared modulator, or add a Mix control to devices that
  lack one.
- No routing intelligence — it just wraps a chain. Audio in → chain → out.

### 6.2 FX Chain (Chain with multiple internal chains)

- Technically "Chain" in the UI. Distinct from "FX Layer".
- Used for serial bundling of audio effects.

### 6.3 FX Layer (parallel audio)

- N parallel effect chains, each with its own mixer strip (volume, pan, mute,
  solo). SAME input audio is copied to each chain; outputs are summed.
- Post-5.1 Bitwig expanded parallel processing opportunities here — multi-
  threaded per-layer processing for CPU gains.

### 6.4 Instrument Layer (parallel instruments)

- N parallel instrument chains with independent mixer strips. Any incoming
  note triggers every layer that accepts it.

### 6.5 FX Selector / Instrument Selector / Note FX Selector

- ONE active child chain at a time with smooth crossfade on switch (fade time
  user-adjustable; audio being received during switch fades in on new chain).
- Voice modes on Instrument Selector include Manual, Round-robin, Random,
  Keyswitches, CC control, Program Change.
- Chain-selector remote controls act as a proxy to the active chain — so a
  pinned device cursor on the selector will forward to whichever chain is
  active. This is Bitwig's best built-in pattern for "dynamically-routed
  device racks".
- Useful for live A/B, guitar-amp selection, performance switching.

### 6.6 Replacer

- Audio → level detector → generates single-pitch MIDI notes when the signal
  crosses a threshold → drives a nested Generator (instrument) chain. Original
  audio still flows to the Replacer's output alongside the generated audio.
- Classic drum-replacement pattern.

### 6.7 Drum Machine

- Up to 128 parallel instrument sub-chains, each triggered by a distinct
  MIDI note (plus Choke groups — triggering note 42 closes the voice at note
  46, etc.).
- Each sub-chain is a full Bitwig device chain with its own modulators.

### 6.8 The Grid (Poly Grid / FX Grid / Note Grid)

- Modular-style environment with ~231 modules in Bitwig 5.3/6.0.
- All signals are stereo; pitch / phase / trigger signals are also stereo
  internally.
- Poly Grid supports full polyphony including Voice Stacking (up to 16
  stacked voices with per-voice modulation and parameter spread).
- FX Grid and Note Grid support polyphonic effects / note processors
  including voice stacking.
- **Cannot host a VST3 as a module**. The Grid has Pre-FX and Post-FX slots
  at the device-container level; VSTs go in those slots, not as patchable
  modules inside the Grid.
  https://bitwish.top/t/a-device-vst-plugin-module-for-the-grid/572
- Modulator Out module lets Grid signals leave the Grid to drive Pre-FX /
  Post-FX parameters (including nested VSTs).

### 6.9 Audio Receiver

- `Audio Receiver` is a device that pulls audio from anywhere in the project:
  any track (pre- or post-), any nested chain, any specific multi-out chain of
  a multi-out plug-in.
- Multiple instances per track allowed. Has Gain, Mix (modulatable), optional
  Source FX slot. Can live inside group tracks, layers, chains.
- This is the canonical Bitwig pattern for "I need audio from over there,
  here".
  https://polarity.me/posts/bitwig-guides/2023-03-01-audio-receiver-bitwig-audio-fx-guide/

### 6.10 Note Receiver

- Mirror of Audio Receiver for MIDI note signals. Pulls note data from any
  note-producing track.

### 6.11 Implications for a plug-in rack design

- Bitwig already has rich containerization WITHIN a track. What it does NOT
  have is containerization ACROSS tracks via a single plug-in instance.
- A plug-in that wanted to look like "Bitwig's FX Layer but hosted externally"
  would need to present N output buses (which Bitwig does support on the
  output side via multi-out chains) but the INPUTS are capped at 4 channels
  (2 stereo) regardless of what the plug-in declares.

---

## 7. Expression sequencer + VST3 params

Bitwig's step-sequencing modulation is split between:

### 7.1 Modulator devices

- **Steps** modulator: classic bipolar step sequencer with step count,
  direction (forward/back/pingpong), polarity, phase. Each step's value is
  a modulation signal sent to any number of mapped parameters, including
  VST3 params.
- **ParSeq-8**: parameter sequencer — each step is its own mod source, so
  each step can target a DIFFERENT set of parameters. Each step also has a
  bipolar depth slider scaling all of that step's modulations. Excellent for
  per-step patch morphing on a VST3.
- **4-Stage**: 4-stage envelope generator.
- **Expressions**: generic control modulator attachable to any device, used
  particularly with MPE.
  https://www.bitwig.com/userguide/latest/modulator/

### 7.2 Per-event expression (clip-level)

- Bitwig clip editor has per-note expression lanes: Velocity, Pressure,
  Timbre (CC74), Gain (per-note gain ±24 dB), Pan, Pitch, Chance, Ratcheting.
- These map to MPE channels for VST3 instruments that support MPE.
- A VST3 audio effect hosted downstream sees the resulting modulated audio;
  it does not receive per-note expression data directly unless it is an MPE-
  aware effect (some pitch-shifters, etc.).

### 7.3 Stepwise (Bitwig 5.3) and Note Grid

- **Stepwise** is a step sequencer NOTE device (added in 5.3): generates
  notes + expressions from a step grid. Sits in the Note FX chain upstream
  of the instrument.
- **Note Grid** lets you build arbitrary note-event generators; its outputs
  include per-note expressions that flow into MPE-aware VST3 instruments.
- Both of these emit NOTES + expressions, not direct parameter modulation to
  audio-effect VST3 plug-ins. To modulate a VST3 FX param from a step
  sequence, use Steps/ParSeq-8 modulators.

### 7.4 Automation

- The automation editor writes values directly into parameter lanes. For
  VST3/CLAP, Bitwig 6.0 uses the plug-in's native parameter units in the
  UI; VST2 is still 0–100%.
- Modes: absolute, additive (±50% of range), multiplicative (scales toward 0).
- Sample-accurate automation recording is supported from controllers and from
  the plug-in window itself (5.1+).

---

## 8. Known VST3 quirks / bugs in Bitwig; missing features vs. other hosts

### 8.1 Quirks / historical bugs

- **Sidechain bus swapping while plug-in active**: violates VST3 spec, caused
  missed `prepareToPlay` calls in JUCE plug-ins. Workaround: plug-ins use
  `processorLayoutsChanged()`.
  https://forum.juce.com/t/bitwig-vst3-preparetoplay-not-called-after-sidechain-activation/50883
- **Invalid parameter ID on state restore**: plug-ins calling performEdit
  during setState get warnings. Fix: don't performEdit during setState.
  https://forum.juce.com/t/bitwig-invalid-parameter-id-message-vst3/50253
- **VCV Rack VST3 dynamic parameters reset to 0 on load** — fixed on VCV's
  side late 2022; root cause was dynamic parameter-list timing with Bitwig.
  https://community.vcvrack.com/t/vst3-and-bitwig-modules-loading-with-parameters-set-to-zero/18830
- **VST3 crashes more than VST2** — community reports. Hosting modes "Within
  Bitwig", "Vendor" (by manufacturer), "Plugin" (by plug-in), "Individually"
  all have crash trade-offs; "Together" and "By Plug-in" are reportedly most
  stable in user testing.
  https://polarity.me/posts/polarity-music/2025-01-14-state-of-bitwig-53/
- **Changing audio buffer size reloads every plug-in**: the audio engine
  reinits. Annoying for live tweaking.
- **Linux VST3 editor resize lag** (JUCE plug-ins): close/reopen editor to
  fix.
- **Offline-oversampling-mode plug-ins**: bad PDC reporting during realtime
  ↔ offline switches leads to timing issues in exports.
- **Sonible Smart EQ 4 cross-instance comms** requires "By manufacturer" or
  "By plug-in" hosting mode; default "Together" may block it.

### 8.2 Features Bitwig has that other hosts generally don't

- Unified modulator system reaching VST3 params (most hosts only automate,
  not modulate, third-party plug-ins at par with native).
- Per-plug-in hosting-mode override and sandboxed recovery from crashes.
- Multi-out VST chain mixer inline in the device panel.
- Full CLAP first-class support including per-voice modulation.

### 8.3 Missing VST3 features vs. other hosts (as of 5.3 / 6.0 beta)

- **Multiple sidechain buses**: Bitwig only exposes one stereo sidechain
  input to a plug-in. Cubase/Nuendo/Studio One allow multiple.
- **Multichannel tracks / surround**: Bitwig is stereo-only at track level.
  No 5.1/7.1/Atmos support. This cascades into VST3 multichannel limits.
- **More than 4 input channels to a VST3**: hard cap; wishlist item.
- **VST3 program change lists / preset list browsing from host UI**: Bitwig
  doesn't expose the VST3 UnitInfo program-change model the way Cubase does.
  It shows individual parameters and lets you save Bitwig-side presets.
- **Note-name (UnitInfo) support**: Bitwig 5.2 added VST3 note-name support
  (for drum kits displaying note labels). Previously absent.
- **IMidiMapping** (VST3 MIDI CC → parameter): Bitwig does support it, but
  controller-to-CC-to-VST is not the primary workflow — users prefer
  modulators/Remote Controls.
- **IPlugInterfaceSupport**: added in 5.2. Before 5.2, plug-ins querying host
  capabilities via this interface got nothing.
- **VST3 offline processing / analyze** (`IAudioProcessor kOffline` for non-
  realtime analyze-then-process): partially supported; many plug-ins gate
  features on this.
- **Sample-accurate cross-patching between VST3 plug-ins**: Bitwig's modulation
  is sample-accurate native-side, but inter-plug-in parameter linkage still
  goes through the host's parameter system.

---

## 9. Modulation reach parity: native vs. VST3

### 9.1 Modulation amount

- VST3 params get the SAME modulation-amount UI, polarity, curve, and
  scaling as native parameters. Bitwig treats them identically as modulation
  targets.
- One caveat: **native devices can expose internal sub-parameters (nested
  chain slot parameters, per-voice states, Grid module outs) that are NOT
  parameters in the VST3 sense**. These have no equivalent in VST3 hosting.
  So "reach parity" is yes for the param list the plug-in exposes, no for
  hidden internals.

### 9.2 Polyphonic modulation

- **Native Bitwig devices (Polymer, Phase-4, Poly Grid, Sampler, etc.)**:
  full per-voice modulation. Each voice sees its own modulator value.
- **CLAP plug-ins supporting per-voice param mod**: full per-voice modulation
  (u-he Diva, Zebra, Surge, Vital via CLAP).
- **VST3 plug-ins**: Bitwig SUMS polyphonic modulation to monophonic before
  delivering to the VST3 parameter. Per-voice depth is lost. VST3's MPE path
  (per-note pitch bend / pressure / timbre) still works.
- **MPE Mode**: "Force MPE Mode" on the plug-in header forces Bitwig to
  split polyphonic voices onto MIDI channels 2–16. Useful for synths that
  are MPE-aware but don't advertise it.

### 9.3 Polyphonic reach: workarounds

- For per-voice effects on a VST3, the only Bitwig-native way is to run the
  VST3 inside an Instrument Layer with each layer getting its own
  monophonic/single-voice note stream — but each layer is a separate INSTANCE
  of the plug-in, not one instance with poly params.
- Voice Stacking spreads modulation across stacked voices on a native
  instrument; it does NOT per-stack modulate a downstream VST3 effect.

---

## 10. Note FX chain and VST3 audio plugins

- Note FX devices manipulate MIDI note messages only. Examples: Arpeggiator,
  Note Echo, Humanize, Note Harmonizer, Note Latch, Micro-pitch, Note
  Repeats, Multi-Note, Chord Repeats, Transpose Map, Velocity Curve.
- Placement: Note FX chain → instrument → Instrument FX post-chain → track
  post-device-chain.
- **Note FX does NOT affect audio-effect VST3 plug-ins.** The audio signal
  passes through Note FX devices unchanged; only notes are transformed.
- Note FX can be fed into a VST3 INSTRUMENT (whose output then flows through
  a downstream audio FX chain including VST3 effects).
- A VST3 effect placed AFTER a VST3 instrument sees the resulting audio; it
  does not see MIDI notes unless it declares an event input bus. Some hybrid
  VST3 plug-ins (MIDI-controlled effects, gates, triggered delays) do declare
  note inputs and Bitwig will route notes to them.
- https://www.bitwig.com/userguide/latest/note_fx/

---

## 11. Any VST3 API for plug-ins to declare they want multi-track audio?

**No standard VST3 way exists, and Bitwig does not implement any extension
for it.**

VST3 has:
- **Multiple input/output audio buses** (`AudioBusBuffers[]` in ProcessData).
  Plug-ins can declare many. Bitwig only feeds 2 stereo buses in (main +
  sidechain) and outputs whatever the plug-in declares (for multi-out
  chains).
- **Main + Aux bus kinds** (`kMain`, `kAux`). Bitwig recognizes the first
  kMain input and one kAux input.
- **Event input/output buses** for MIDI notes.
- No API for a plug-in to say "I want to be instantiated once and appear on
  N tracks."

Bitwig doesn't implement any private VST3 extension along those lines either.
Its cross-instance communication story is:

1. **Host process sharing** (`Plugin Hosting Mode: By plug-in` or `By
   manufacturer`) — multiple INSTANCES of the plug-in share the same sandbox
   so the plug-in's own IPC (shared memory, named pipes, etc.) works. NI
   Komplete Kontrol, Celemony ARA-like workflows, iZotope Relay use this.
2. **Audio Receiver pulling** — a second track can ingest the first plug-in's
   audio.
3. **Multi-out chains** — one instance's N outputs reach N downstream
   destinations.

None of these is "one instance, two strips." For that, the closest VST3 has
is declaring N main output buses, which Bitwig surfaces as the multi-out
chain mixer — but each "chain" is just an audio output, not a first-class
track strip with its own note input / parameter pane / send system.

CLAP also does not have such a concept. CLAP's `note_ports` and audio ports
are per-instance, same constraint. CLAP's promise relative to VST3 is
per-voice modulation and non-destructive param automation — not multi-track
ownership.

---

## Hard constraints (for plugin-rack design)

1. **One VST3 instance = one track**. No official way to span strips.
2. **Max 4 audio input channels to any VST3** (2 stereo pairs: main +
   sidechain). Extra declared input buses are IGNORED regardless of what the
   plug-in says.
3. **Max 1 stereo sidechain source**. No multi-sidechain routing.
4. **Stereo-only track architecture**. No surround/Atmos/multichannel tracks.
5. **Note FX never affect audio VST3s**. Notes and audio are strictly
   separate signal types until an instrument bridges them.
6. **Polyphonic modulation to VST3 is summed to mono**. True per-voice
   parameter mod is CLAP-only (and certain Bitwig natives).
7. **Changing audio buffer size reinits the engine** (reloads all plug-ins).
8. **Plug-ins cannot request a specific buffer size** from Bitwig. They get
   `maxSamplesPerBlock` once via `setupProcessing` and must tolerate any
   block size ≤ that.
9. **Variable block sizes are the norm** inside a callback due to sample-
   accurate automation/modulation splitting. Plug-ins must handle any size.
10. **No host-provided per-plugin zoom factor beyond OS DPI + bilinear
    stretch toggle.** Well-built VST3s self-scale.
11. **Sidechain bus may be toggled while plug-in is active** — violates VST3
    spec, historically broke prepareToPlay. Plug-ins must use
    `processorLayoutsChanged` to recover.
12. **VST3 dynamic parameter add/remove works** but bindings reference
    parameter ID — orphaned parameters drop their modulator routings.
13. **VST3 param automation from Bitwig 6.0 displays native units**; VST2
    still 0–100%.
14. **No VST3 API for multi-track ownership**. No Bitwig extension for it.
15. **The Grid cannot host VST3 as a patchable module**. Only Pre-FX and
    Post-FX slots at the device boundary.

---

## Workaround space (if the goal is a multi-strip rack on top of Bitwig)

Given the above, a plugin-rack project that wants to expose two or more
independent per-track strips from a single VST3 plug-in inside Bitwig must
use one of the following patterns. None is perfect.

### Pattern A: Multi-output broadcast

- Plug-in is a multi-output VST3. One instance on track A (instrument).
- Outputs 2..N of the plug-in are picked up by Audio Receivers on tracks B,
  C, etc., OR via the destination track's audio-input chooser pointing to
  the plug-in's internal chains.
- Track A hosts the plug-in's GUI, parameter control, sidechain, and MIDI.
- Tracks B..N are read-only audio sinks. They can add their own post-FX
  chains and mixer strips, giving a partial "multi-strip" illusion.
- **Limitation**: Only ONE track (track A) can feed the plug-in. Tracks
  B..N cannot deliver audio or MIDI to the plug-in.

### Pattern B: Multiple instances sharing state via IPC

- Use "By plug-in" hosting mode so every instance of the plug-in runs in the
  same OS process (BitwigPluginHost).
- The plug-in implements its own cross-instance comms (shared memory,
  named sockets, ring buffer).
- Each instance on a different track can be a distinct strip; they share a
  mutable state object so they behave as facets of one "virtual rack".
- **Limitation**: state is out-of-band from Bitwig's project format. Undo,
  preset save/load, and automation lanes all live per-instance. Requires
  careful design of what state is per-instance vs. shared.
- **Limitation**: Bitwig's isolation may kill one while the other keeps
  running, breaking invariants. Crash-recovery reloads each instance
  separately.

### Pattern C: One container track with Audio Receivers

- A single "rack" track hosts ONE plug-in instance and N Audio Receivers
  pulling audio from N "strip tracks" (which are otherwise empty).
- Strip tracks are just audio sources; the rack track does all processing.
- Routing plug-in outputs back to the strip tracks requires: plug-in
  declares multi-out, each strip track has its input set to the plug-in's
  corresponding chain (as in Pattern A).
- **Works as a "fan-in → process in one instance → fan-out"** pattern.
- **Limitations**:
  - Only 4 input channels into the plug-in (2 stereo pairs). So at most 2
    strips can be distinguishable — main + sidechain — and the plug-in must
    interpret "sidechain" as "strip 2". Bad for >2 strips.
  - Tracks are summed if multiple Audio Receivers target the same stereo
    input.
  - Latency compensation is per-track; fan-in/fan-out paths must be balanced.

### Pattern D: Control surface / scripting

- Bitwig Controller API scripts can read/write plug-in params and push
  modulation. A script could synthesize a "rack" by coordinating multiple
  plug-in instances on multiple tracks (Pattern B) via the Bitwig-side
  scripting layer instead of custom IPC.
- **Limitation**: scripts run on the control-surface thread, not the audio
  thread. No sample-accurate comms.

### Pattern E: Do it in CLAP instead

- If you control the plug-in format, CLAP gives you:
  - True per-voice parameter modulation.
  - Better note port model.
  - Same multi-out-chain support in Bitwig.
- Still doesn't solve multi-track-ownership (no host does), but it widens
  what per-voice/per-note modulation can reach.

### Pattern F: Group track processing

- For effect-style racks, a Group track with the plug-in as the single
  device on it accepts summed audio from all child tracks.
- The plug-in sees ONE stereo input (the sum) plus one sidechain.
- Child tracks remain independent up until the sum point and can have their
  own pre-group plug-ins.
- **Limitation**: Cannot separate children at the plug-in level; it's one
  bus.

### Ranking for a "plugin-rack" goal

If the goal is "single VST3 owns N strips":

1. **Pattern B + D** (multiple instances + controller scripting for
   coordination) gives the best user-perceived unification inside Bitwig
   without fighting the architecture.
2. **Pattern C** is the only way to get actual audio routed through ONE
   instance from multiple strips, and it's capped at 2 (main + sidechain).
3. **Pattern A** works if the rack's primary job is SPLITTING one source
   to many (e.g. Kontakt-style multitimbral).
4. **Pattern E** if you can dictate the plug-in format.

---

## Source summary

- Bitwig User Guide (routing, container, vst_plug-ins, vst_plug-in_handling_
  and_options, modulator, unified modulation system, note_fx, advanced
  device concepts):
  https://www.bitwig.com/userguide/latest/routing/
  https://www.bitwig.com/userguide/latest/container/
  https://www.bitwig.com/userguide/latest/vst_plug-ins/
  https://www.bitwig.com/userguide/latest/vst_plug-in_handling_and_options/
  https://www.bitwig.com/userguide/latest/modulator/
  https://www.bitwig.com/userguide/latest/the_unified_modulation_system/
  https://www.bitwig.com/userguide/latest/note_fx/
  https://www.bitwig.com/userguide/latest/advanced_device_concepts/
- Bitwig support article on multi-out VSTs:
  https://www.bitwig.com/support/technical_support/how-do-i-use-multi-out-vst-plug-ins-27/
- Bitwig sidechaining tutorial:
  https://www.bitwig.com/learnings/sidechaining-tutorial-49/
- Bitwig plug-in hosting & crash protection:
  https://www.bitwig.com/learnings/plug-in-hosting-crash-protection-in-bitwig-studio-20/
- Bitwig 5.1.1 release notes:
  https://downloads.bitwig.com/5.1.1/Release-Notes-5.1.1.html
- Bitwig 5.2 release notes:
  https://downloads.bitwig.com/5.2/Release-Notes-5.2.html
- Bitwig 6.0 release notes (beta-era):
  https://downloads.bitwig.com/6.0/Release-Notes-6.0.html
- The Grid overview:
  https://www.bitwig.com/the-grid/
- CLAP standard story:
  https://www.bitwig.com/stories/clap-the-new-audio-plug-in-standard-201/
- JUCE / Bitwig VST3 sidechain prepareToPlay bug:
  https://forum.juce.com/t/bitwig-vst3-preparetoplay-not-called-after-sidechain-activation/50883
- JUCE / Bitwig VST3 invalid parameter ID:
  https://forum.juce.com/t/bitwig-invalid-parameter-id-message-vst3/50253
- VCV Rack VST3 parameter-reset bug:
  https://community.vcvrack.com/t/vst3-and-bitwig-modules-loading-with-parameters-set-to-zero/18830
- Polarity Audio Receiver guide:
  https://polarity.me/posts/bitwig-guides/2023-03-01-audio-receiver-bitwig-audio-fx-guide/
- Polarity: State of Bitwig 5.3 (Jan 2025):
  https://polarity.me/posts/polarity-music/2025-01-14-state-of-bitwig-53/
- Bitwish (wishlist):
  https://bitwish.top/t/routing-more-than-4-channels-of-audio-to-plugins/614
  https://bitwish.top/t/multichannel-tracks/2588
  https://bitwish.top/t/a-device-vst-plugin-module-for-the-grid/572
- KVR Audio Bitwig Forum threads:
  https://www.kvraudio.com/forum/viewtopic.php?t=515542 (VST audio inputs)
  https://www.kvraudio.com/forum/viewtopic.php?t=515446 (routing limits)
  https://www.kvraudio.com/forum/viewtopic.php?t=522328 (bus equivalence)
  https://www.kvraudio.com/forum/viewtopic.php?t=552244 (multiple sidechain)
  https://www.kvraudio.com/forum/viewtopic.php?t=545358 (same plugin multiple tracks)
  https://www.kvraudio.com/forum/viewtopic.php?t=573288 (multichannel into VST)
  https://www.kvraudio.com/forum/viewtopic.php?t=608500 (plugin UI scaling)
  https://www.kvraudio.com/forum/viewtopic.php?t=582715 (HiDPI defaults)
  https://www.kvraudio.com/forum/viewtopic.php?t=596390 (buffer size reload)
  https://www.kvraudio.com/forum/viewtopic.php?t=524441 (offline render)
  https://www.kvraudio.com/forum/viewtopic.php?t=603810 (VST3 missing sidechain)
  https://www.kvraudio.com/forum/viewtopic.php?t=613693 (Bitwig 5.2.3 thread)
  https://www.kvraudio.com/forum/viewtopic.php?t=565969 (scripting params)
  https://www.kvraudio.com/forum/viewtopic.php?t=540750 (audio rate mod of VST)
  https://www.kvraudio.com/forum/viewtopic.php?t=595782 (inter-instance VST3 comms)
- VST3 SDK references (for API constraints):
  https://steinbergmedia.github.io/vst3_doc/vstinterfaces/classSteinberg_1_1Vst_1_1IHostApplication.html
  https://steinbergmedia.github.io/vst3_doc/vstinterfaces/classSteinberg_1_1Vst_1_1IParameterChanges.html
  https://steinbergmedia.github.io/vst3_dev_portal/pages/Technical+Documentation/Change+History/3.6.6/IPlugViewContentScaleSupport.html
