# Prior Art: Plugin Racks and Mixing-Console Plugins

Research compiled April 2026. Survey of commercial and open-source plugin hosts that embed other plugins, or that implement "channel-strip / mixing-console" behavior through inter-instance linking. Focus is on patterns worth borrowing for a new rack/console plugin.

The recurring design problem: **a single DAW insert slot should host multiple DSP blocks and, optionally, coordinate with copies of itself on other tracks**. Most serious products solve either the "many plugins in one instance" side (StudioRack, PatchWork, Metaplugin, Snap Heap) or the "many instances talk to each other" side (FabFilter Instance List, iZotope Relay). A few attempt both.

---

## Waves StudioRack

Sources:
- https://www.waves.com/plugins/studiorack
- https://www.waves.com/introducing-the-new-studiorack-plugin-chainer
- https://www.waves.com/easy-parallel-processing-studiorack-parallel-racks
- https://www.waves.com/parallel-and-multiband-processing-waves-studiorack
- https://www.waves.com/studiorack-now-hosts-vst3
- https://www.kvraudio.com/news/waves-ships-enhanced-studiorack-plugin-chainer---new-macros-parallel-processing-racks-and-multiband-racks-48752
- https://bedroomproducersblog.com/2022/09/07/waves-studiorack-14/

### What it is
Free plugin-chainer from Waves, distributed as a VST2/VST3/AU/AAX. Originally Waves-only; since v14 (2022) hosts any third-party VST3.

### Multi-track-in-one-instance?
No — it is a single-channel chain host, not a multi-track console. "Multi-track" behavior is achieved by saving chains and recalling them on different inserts, plus the StudioVerse cloud preset library.

### Hosting model
- Up to **8 plugins per chain** in series.
- **Parallel racks**: from any slot you can branch into up to 8 parallel chains (mono / stereo / M-S). Each parallel lane is itself a chain.
- **Multiband racks**: any slot can become a crossover with per-band chains. Makes any plugin multiband.
- **Macros**: 8 macros per chain; each macro maps to any combination of sub-plugin parameters with per-mapping min/max scaling. This is how user-facing automation is achieved.
- Cross-DAW preset portability: saved chain files load in any DAW because StudioRack itself is identical across them.

### GUI for nested plugins
Chain view is a horizontal strip of plugin tiles. Clicking a tile opens the sub-plugin's native GUI in a **separate floating window** owned by StudioRack. Not embedded — resize/zoom is whatever the sub-plugin supports. StudioRack's own header UI is fixed size with no zoom.

### Parameter exposure to host
DAW sees **StudioRack's own parameters only**: 8 macros per chain plus bypass/mute per slot, plus parallel/multiband mix controls. Sub-plugin parameters are NOT exposed to the host automation lane — if the user wants to automate `Pro-Q 4 Band 3 Gain` they must map it to a macro first. This is a critical design choice: it hides the 10,000-parameter mess from the DAW, at the cost of requiring macro assignment for automation.

### Inter-instance linking
None. Each insert is isolated.

### Licensing / price
Free (Waves account + WUP required to redownload eventually). Closed source.

---

## Blue Cat Audio PatchWork

Sources:
- https://www.bluecataudio.com/Products/Product_PatchWork/
- https://www.bluecataudio.com/Doc/Product_PatchWork/
- https://www.kvraudio.com/product/patchwork-by-blue-cat-audio
- https://www.production-expert.com/home-page/2017/2/14/blue-cat-audio-launch-mb-7-mixer-v3-patchwork-v2-plug-ins
- https://gearspace.com/gear/blue-cat/patchwork

### What it is
€99 / $99 universal plugin-patchbay. Hosts up to **64 VST / VST3 / AU plugins** per instance. Runs as plugin or standalone.

### Multi-track-in-one-instance?
Not as a console metaphor, but its 64-slot matrix is expansive enough that you can implement per-bus chains with routing, which reaches console-like capability.

### Hosting model
- Matrix GUI: add rows (parallel chains, up to 8) and columns (serial slots, up to 8 pre / 8 post) like a spreadsheet.
- **Flexible audio routing**: any sub-plugin I/O can be connected to any of up to 16 audio channels.
- **MIDI routing** between sub-plugins and host.
- **External sidechain** and multi-out supported.
- **Latency compensation** per slot.

### GUI for nested plugins
Sub-plugin windows float. PatchWork remembers **per-plugin window position** so session recall restores the layout. Own UI zooms 70-200%. Individual plugin windows remain at their own native size.

### Parameter exposure to host
**Parameter mapping editor** — user picks which sub-plugin parameters are exposed. The exposed set becomes DAW-automatable parameters on the PatchWork instance. Same macro-style pattern as StudioRack but explicit and per-parameter, not limited to 8.

### Inter-instance linking
None first-party. MIDI routing to host can be abused as a side channel.

### Licensing / price
Commercial, closed source. €99/$99.

---

## Blue Cat Audio MB-7 Mixer

Sources:
- https://www.bluecataudio.com/Products/Product_MB7Mixer/
- https://www.kvraudio.com/product/mb-7-mixer-by-blue-cat-audio
- https://www.pluginboutique.com/product/2-Effects/16-EQ/260-Blue-Cat-s-MB-7-Mixer

### What it is
$129 seven-band splitter / multiband mixing console. Splits incoming audio into up to 7 bands, treats each band as its own channel strip (gain/pan/mute/solo), optionally hosts up to 4 plugins per band pre or post fader.

### Multi-track-in-one-instance?
**Yes, in the multiband sense.** Each band is a lane with its own plugin host chain. Closest commercial precedent for a "console inside a plugin" where the channels are frequency bands rather than tracks.

### Hosting model
- 7 bands max; crossover 12-192 dB/oct.
- Per-band: 4 plugin slots pre-fader + 4 post-fader, stereo or M/S, full sidechain, freely routable I/O.
- "Dual mode": per-band independent L/R or M/S controls — essentially two parallel mini-consoles per instance.

### GUI for nested plugins
Fixed tabular layout: bands across columns, slot stack per band. Sub-plugin GUIs float separately.

### Parameter exposure
Exposed: per-band gain/pan/mute/solo/width, crossover frequencies, host-level bypass. Sub-plugin parameters via Blue Cat's parameter-mapping system, same as PatchWork.

### Inter-instance linking
**Yes** — "multiple instance linking" groups bands within one instance or across instances of MB-7 Mixer. Uses Blue Cat's shared IPC layer. One of very few commercial plugins that explicitly advertises cross-instance linking.

### Licensing / price
Commercial, closed source. $129.

---

## DDMF Metaplugin

Sources:
- https://ddmf.eu/metaplugin-chainer-vst-au-rtas-aax-wrapper/
- https://ddmf.eu/pdfmanuals/MetapluginManual.pdf
- https://www.soundonsound.com/reviews/ddmf-metaplugin-3
- https://www.kvraudio.com/product/metaplugin-by-ddmf

### What it is
$59 plugin-inside-plugin host. Emphasizes graph-based routing and bridging features (32-bit plugins in 64-bit host, Intel plugins on Apple Silicon, mid-side matrix).

### Multi-track-in-one-instance?
Not explicitly a console, but the effect version supports up to 8 individual channels (surround/multichannel), and the instrument version has 16 stereo out buses. With careful routing you can build a bus-style layout, but no channel-strip UI.

### Hosting model
- **Free-form node graph**: drag plugins onto a canvas, draw curved cables between I/O. Signal can fan in / fan out / feed back.
- Bundled modular pieces: mid-side matrix, 4-band crossover, routing plugin.
- **16x realtime oversampling**, 64x offline.
- **Full PDC** across the graph automatically.
- Per-node wet/dry.

### GUI for nested plugins
Canvas with nodes + cables. Sub-plugin GUIs open in floating windows. Metaplugin's own shell has no resize story; sub-plugin windows are whatever the sub-plugin offers.

### Parameter exposure
**100 host-exposed parameters**, each mappable to any sub-plugin parameter via a Learn function. "The host DAW obviously can't directly see the plug-ins hosted in Metaplugin" — explicit design statement from the Sound On Sound review.

### Inter-instance linking
None.

### Licensing / price
Commercial, closed source. $59. Low price point + broad compatibility has kept it a go-to for odd routing tasks.

---

## Kilohearts Snap Heap and Multipass

Sources:
- https://kilohearts.com/products/snap_heap
- https://kilohearts.com/products/multipass
- https://kilohearts.com/docs/snap_heap
- https://kilohearts.com/blog/wtf_is_a_snap_heap
- https://www.sweetwater.com/insync/understanding-kilohearts-snapins-ecosystem/

### What they are
Two related "Snapin" hosts. Snap Heap is **free** (was $29, now $0); Multipass is $99. Snapins are Kilohearts' own modular effect units — NOT generic VST3. The closed ecosystem is the point: Snapins fit into any Snapin host without cable-drawing.

### Multi-track-in-one-instance?
Not across DAW tracks, but Snap Heap is 7 parallel/serial lanes, Multipass is 5 frequency bands with Snap-Heap-like lane infrastructure inside each.

### Hosting model
- **Snapins plug in as first-class modules**. Serial by default, lanes for parallel. Drag and drop from a tray.
- Built-in **modulation matrix**: 2 LFOs, 2 envelopes, 8 macros, pitch tracker, MIDI modulators, audio-follower. Every parameter on every Snapin is a modulation destination.
- Modulation depth per-target with visual "ring" overlay on the destination knob — major UX idea worth copying.

### GUI for nested plugins
Because Snapins are Kilohearts-native, their UIs are **embedded** directly in the host pane, same visual language. No "open a floating window" dance. The rack scales as a single uniform UI surface.

### Parameter exposure
Macros are the host-facing parameters. The DAW sees a fixed macro set, not per-Snapin params. Same pattern as StudioRack, but visually integrated.

### Inter-instance linking
None documented.

### Licensing / price
Closed source. Snap Heap free. Multipass $99. Subscription bundle $9.99/mo includes all Snapins.

### Key insight
The **closed-module ecosystem** is what makes the GUI cohesive. Hosting arbitrary third-party VST3 necessarily produces a messier UI because each sub-plugin has its own visual language and window-management needs.

---

## FabFilter Linking (Pro-Q 4 Instance List)

Sources:
- https://www.fabfilter.com/help/pro-q/using/instance-list
- https://www.fabfilter.com/news/1768467600/fabfilter-introduces-multi-plugin-instance-list-in-pro-q-410
- https://www.fabfilter.com/products/pro-q-4-equalizer-plug-in
- https://www.soundonsound.com/reviews/fabfilter-pro-q-4
- https://www.fabfilter.com/help/pro-q/using/eqmatch

### What it is
Pro-Q 4 (and later Pro-Q 4.10) embed a shared "Instance List" UI panel. Every Pro-Q 4, Pro-C 3, Pro-DS, and Pro-G instance in the session shows up as a thumbnail in the list, ordered by track order, colored by track color.

### Multi-track-in-one-instance?
**Effectively yes for the mix workflow**, while remaining technically one plugin per insert. Pro-Q 4.10's Instance List lets one instance act as a mission-control panel over all the others.

### Linking features
- See all other instances' spectra and EQ curves at once, including across tracks.
- **Collision detection**: show masking conflicts between two Pro-Q curves on different tracks.
- **Cross-control**: from one Pro-Q 4 you can bypass, solo, and edit other instances.
- **EQ Match**: pulls spectral data from another instance set as reference.

### How linking is implemented
Not officially disclosed. Help documentation says: "Pro-Q 4 instances know the track order and color of the track they are placed on" using **plugin-format and DAW-specific features**. In VST3 this uses `IInfoListener` / `IContextMenuTarget` / `IHostApplication` for track context, plus some shared-data channel. In Audio Units on Logic the functionality is degraded because "the format doesn't support communicating these to the plug-in yet."

Technically the candidate mechanisms are:
- **Shared memory** within the host process (all plugin instances live in the DAW's address space), using a named mmap region for state + atomic ring buffers for spectra.
- **Static globals** in the shared library with the host process as the sync authority.
- Occasional **named pipes / sockets** for cross-process hosts.

FabFilter almost certainly uses option 1 (shared mmap / statics + a small registry singleton).

### Parameter exposure
Still per-instance. DAW doesn't get new parameters from the linking feature.

### Licensing / price
Closed source. Pro-Q 4 is €169, Pro-C 3 €149, Pro-G €129, Pro-DS €129. The Mix Bundle bundles them.

### Key insight
**Linking doesn't have to expose combined state to the DAW**. FabFilter keeps each instance self-contained; the linking is a GUI-layer and DSP-reference-layer feature, not a parameter-model feature. This sidesteps all the nasty "what does it mean for a parameter to belong to multiple instances" automation questions.

---

## iZotope Nectar / Ozone Module Strips

Sources:
- https://www.izotope.com/en/products/nectar.html
- https://www.izotope.com/en/products/nectar/features
- https://www.izotope.com/en/products/ozone/features.html
- https://docs.izotope.com/ozone11/en/general-controls/index.html
- https://www.izotope.com/en/learn/rulebreakers-how-to-use-ozone-modules-for-mixing-and-production

### What they are
Nectar 4 ($249) and Ozone 12 (~$499 Advanced) are multi-module "channel strip" plugins. Ozone is mastering-focused; Nectar is vocal-focused. Both expose a horizontal module chain UI where modules can be added/removed/reordered.

### Multi-track-in-one-instance?
No. Both are single-channel processors. However Nectar 4 Advanced ships each module as an independent VST3 too, so you can "decompose" the strip into individual plugins and distribute them across a DAW template — this is the pattern they push.

### Hosting model
Closed: modules are first-party. Ozone 12 modules: Clarity, Dynamic EQ, Dynamics, EQ, Exciter, Imager, Impact, Bass Concentration, Master Rebalance, Matching EQ, Maximizer, Spectral Shaper, TBC, Vintage Comp, Vintage EQ, Vintage Limiter, Vintage Tape.

### GUI for nested modules
**Single resizable window**. Module chain at top; selected module's full editor fills the body. Whole plugin resizes by dragging bottom-right corner (known to have quirks in some DAWs, see Cakewalk forum thread). This is the cleanest "nested plugin UI" in commercial audio: modules speak the same visual language and scale with the shell.

### Parameter exposure
All module parameters exposed to DAW directly (not hidden behind macros). Works because modules are first-party with stable ABIs; param count is bounded.

### Inter-instance linking
Via **Relay** (see next section).

### Licensing
Closed source.

---

## iZotope Relay + Neutron Inter-Instance Linking

Sources:
- https://www.izotope.com/en/products/neutron/features/relay
- https://www.izotope.com/en/products/insight/features/relay
- https://s3.amazonaws.com/izotopedownloads/docs/relay101/en/ipc/index.html
- https://s3.amazonaws.com/izotopedownloads/docs/neutron300/en/relay/index.html
- https://www.izotope.com/en/learn/inter-plugin-communication-explained.html

### What it is
**Relay** is a free lightweight utility plugin that sends/receives metering and control data to/from iZotope's IPC-aware hosts (Neutron, Nectar, VocalSynth, Ozone, Insight, Tonal Balance Control). Instantiate Relay on every track, then the "Visual Mixer" and "Mix Assistant" (in a Neutron instance) operate on all of them.

### Linking features
- **Visual Mixer** shows every Relay node as a dot in a 2D pan/level/width field. Drag the dot and the target instance's gain/pan/width slider moves.
- **Masking Meter**: pick two tracks, Neutron shows masking between them — implies spectrum data is streaming between instances.
- **Mix Assistant**: automatic initial level balance across all Relay-tagged tracks.
- **Tonal Balance Control**: aggregates EQ state from all Relay/Neutron instances, compares to a target curve.

### How linking is implemented
iZotope calls it **Inter-plugin Communication (IPC)**. Docs are sparse on mechanism but the system assumes all instances live in one host process (standard VST3/AU hosting) and uses a shared-memory / static-global registry keyed per-DAW-project. The Relay→Neutron communication is unidirectional to the controller UI; the "Visual Mixer" then reverse-controls individual Relay params.

### Parameter exposure
Each Relay instance has its own small param set (gain, pan, HPF, width). The linking layer composes them; the DAW never sees a "meta" parameter.

### Key insight
**Relay is a "tag" plugin**. By requiring the user to drop Relay on every track, iZotope avoids having to auto-discover tracks via DAW hacks. The user explicitly opts each track in. Much cleaner than trying to enumerate sibling instances.

---

## UAD Console / Apollo Console

Sources:
- https://www.uaudio.com/products/uad-console-app
- https://help.uaudio.com/hc/en-us/articles/25347160337556-UAD-Console-Overview
- https://help.uaudio.com/hc/en-us/articles/24433952524308-FAQ-UAD-Console
- https://www.uaudio.com/blogs/ua/console-getting-started

### What it is
UAD Console is **not a plugin** — it is a companion application that runs alongside the DAW and controls the DSP mixer inside an Apollo audio interface. It exists to give near-zero-latency monitoring through UAD plugins during tracking. Relevance here: it is a mixing-console UI that integrates with a DAW session via a "Console Recall" plugin that saves Console state in the DAW project.

### Hosting model
Console app offers 8-24 input channels (depending on Apollo model), aux busses, Unison preamp slot (mic-pre modeling), up to four UAD plugin slots per channel, monitor section. All DSP runs on Apollo SHARC chips, not host CPU. Plugins are UAD-format (closed, proprietary to UA).

### GUI
**Full hardware-console GUI**: channels as vertical strips, faders, inserts, sends. Full resize. Native-quality UI. This is the reference for what a plugin-rack aspiring to console feel should look like — but Console is not a plugin, it is a stand-alone app talking to DSP hardware over Thunderbolt.

### Parameter exposure
Only through the Console Recall plugin, which dumps Console state into the DAW project as opaque data. Individual Console parameters are NOT automatable from the DAW.

### Inter-instance linking
N/A — there is only one Console per Apollo.

### Licensing / price
Free, but requires Apollo hardware ($700-$4000+). Closed source.

### Key insight
UAD Console proves that a console-style UI inside a DAW session is viable UX, but the cost is that it sits outside the DAW's automation/mixer graph. That's the fundamental trade.

---

## Softube Console 1

Sources:
- https://www.softube.com/us/console-1-mixing-system-mk-iii
- https://www.soundonsound.com/reviews/softube-console-1
- https://www.softube.com/user-manuals/console-1-mk-ii
- https://www.softube.com/custom-channel-strips-in-console-1-mixing-system

### What it is
Hardware control surface + channel-strip plugin. Hardware is ~$899 (Channel Mk III) or $1,099 (Fader Mk III). Plugin is included. Each insert of the Console 1 plugin is one channel strip; strip loads emulations (SSL 4000E, Summit Grand Channel, Weiss, British Class A, Empirical Labs) or "Console 1-Ready" third-party plugins mapped into its section slots (Input, EQ, Shape, Comp, Drive, Output).

### Multi-track-in-one-instance?
No. One insert = one channel strip. Multi-track console behavior comes from the **hardware controller plus cross-instance linking**: the hardware lists all Console 1 plugin instances in the session and lets you jump between them with the Track Selector button.

### Linking features
- **Track Selector**: hardware button picks which Console 1 instance is "focused". DAW-side plugin reports its presence to the Console 1 global service.
- **Group**: hold Group + select multiple tracks. Subsequent knob moves fan out to every track in the group, diffing rather than overwriting — only the tweaked parameter changes.
- **Copy**: hold Copy + All to duplicate settings from one strip to many.
- **Shift**: while held, knobs control the DAW track's sends.

### How linking is implemented
A persistent **Console 1 background service** coordinates all plugin instances through IPC. Plugin instances register on load; the service maintains the list of tracks and dispatches knob events from hardware to the currently focused instance(s). Some features (Group, Copy, A/B/C/D, Favorites) require the hardware controller — they are gated in software-only mode.

### Parameter exposure
Strip has a fixed parameter set (gain, EQ bands, comp threshold/ratio/attack/release, drive, etc.) — identical across all strips, regardless of which emulation is loaded. This **normalized parameter schema** is the key design idea: DAW automation lanes stay stable when you swap the underlying emulation.

### Licensing / price
Closed source. Hardware $899-$1099 for controller; emulations $199-$299 each; Flow subscription $24.99/mo.

### Key insight
**Normalized parameter schema across interchangeable DSP implementations** solves the "automation breaks when I change the plugin" problem. The rack defines the parameter shape, not the sub-plugin.

---

## Bitwig Grid + FX Chain

Sources:
- https://www.bitwig.com/the-grid/
- https://www.bitwig.com/userguide/latest/grid_modules/
- https://www.bitwig.com/userguide/latest/advanced_device_concepts/
- https://www.bitwig.com/stories/behind-the-scenes-modularity-in-bitwig-studio-21/

### What it is
Bitwig Studio's in-DAW modular environment. Three Grid devices: Poly Grid (synth), FX Grid (effect), Note Grid (MIDI processor). Each is a nestable device that hosts a user-built graph of modules plus any VST/CLAP plugins.

### Nested chains
Bitwig's killer feature for this discussion: nearly every Bitwig device has **nested device chains**. Common types:
- **Pre FX**: processes signal before the device.
- **Post FX / FX**: processes device output.
- **Wet FX**: only processes the "wet" portion of delays/reverbs.
- **FB FX**: inside a feedback loop.
- Containers: **Drum Machine** (up to 128 note-triggered chains), **Instrument Layer** (parallel instrument stack), **FX Layer** (parallel FX stack).

### GUI for nested chains
Inline in the device panel row. Each nested chain has a breadcrumb; you zoom into it and the containing device collapses to a compact header. No modal pop-ups. This is the best nested-plugin UX in any commercial DAW.

### Parameter exposure
Bitwig has a **Unified Modulation System**: any control anywhere can be a modulation destination. Macro knobs on containers forward to the DAW (which is Bitwig itself). Modulators (LFO, envelope, step seq, random) are added per-device and target per-parameter depth independently. Every parameter is ring-visualized when modulated.

### Inter-instance linking
N/A — everything is in the DAW's device tree.

### Licensing
Bitwig Studio is commercial ($399 full / $99 Essentials). Not a plugin; relevant as a UX reference for nested chains.

---

## Reaper ReaPlugs + JSFX + FX Chain

Sources:
- https://www.reaper.fm/reaplugs/
- https://www.reaper.fm/sdk/js/js.php
- https://forums.cockos.com/showthread.php?p=2595837
- https://forum.cockos.com/archive/index.php/t-182434.html

### What it is
Reaper's native FX Chain is the closest DAW-builtin to a plugin rack: per-track FX list, save/load as `.RfxChain` file, drag plugins between slots. Plus **JSFX** — Cockos's scripting format for DSP (text source, JIT-compiled EEL2, can draw its own UI). ReaPlugs is the subset of Reaper's native plugins (ReaEQ, ReaComp, ReaXcomp, etc.) distributed as VST for non-Reaper DAWs.

### Multi-track-in-one-instance?
No. FX Chains are per-track in Reaper; in non-Reaper DAWs the only route is YSFX, a third-party VST3 host for JSFX files (https://github.com/JoepVanlier/ysfx-vst3 — lets JSFX chains work as a single plugin insert).

### Hosting model
Reaper's FX Chain lives at the track level, not plugin level. No plugin-inside-plugin story. JSFX can import other JSFX files (`@import`) so scripts can compose, but it's source-level composition, not runtime hosting.

### GUI
FX Chain window is a left-pane list + right-pane selected-plugin UI. Sub-plugins open in separate floating windows or dock into the main FX window. User can pin, group, rename.

### Parameter exposure
All sub-plugin parameters visible to Reaper automation by default. Reaper is unusually permissive about this.

### Inter-instance linking
None native. Reaper has **ReaLink** / JS `jsfx-shared` for cross-instance shared memory in JSFX; third-party tools exist.

### Licensing / price
Reaper $60 discounted / $225 commercial. ReaPlugs free. JSFX source plugins widely available on GitHub, mostly open source (e.g., JoepVanlier/JSFX, chkhld/jsfx).

### Key insight
**File-based chain presets (`.RfxChain`) that carry full sub-plugin state** are the gold standard for portability. Most serious plugins should be able to emit their state as a binary blob that the rack can serialize alongside its own state, and re-inflate on reload. This is what Reaper, StudioRack, and PatchWork all do.

---

## iPlug2 / JUCE Hosting Examples

Sources:
- https://juce.com/tutorials/tutorial_audio_processor_graph
- https://docs.juce.com/master/classAudioProcessorGraph.html
- https://iplug2.discourse.group/t/can-iplug2-plugins-host-other-plugins/556
- https://iplug2.github.io/
- https://forum.juce.com/t/juce-plugin-host/61888

### JUCE AudioProcessorGraph
**The canonical C++ pattern for plugin-inside-plugin.** `AudioProcessorGraph` IS an `AudioProcessor`, so a plugin's top-level processor can literally be a graph of nodes. Special `AudioGraphIOProcessor` nodes represent the outer plugin's audio-in / audio-out / MIDI-in / MIDI-out. Generic nodes hold plugin instances or built-in processors. Connections are `(sourceNodeID, channel) → (destNodeID, channel)`.

JUCE tutorial builds a three-slot channel strip: three selector tiles, each can load a gain / filter / oscillator, signal flows in → slot1 → slot2 → slot3 → out. This **three-slot channel-strip shape** is the foundational pattern.

Key limitations:
- No PDC across nodes built in; you add it manually.
- GUI: each hosted processor has its own editor window; the graph host renders tile UIs and spawns editors on click. No automatic embedding.
- Parameter forwarding: NOT automatic. Host plugin must expose its own `AudioProcessorParameter`s and route them manually to child processors. Same macro-assignment pattern as commercial products.

### iPlug2
- Targets CLAP, VST2, VST3, AUv2, AUv3, AAX, WAM.
- Forum thread explicitly asks about hosting other plugins; the answer is "not out of the box, but the framework doesn't prevent it — you'd pull in a separate hosting lib."
- iPlug2's "Distributed Plugins" is a different feature (remote rendering over network, not relevant to rack use case).

### Licensing
Both frameworks are dual-licensed open source (JUCE: GPLv3 / commercial; iPlug2: WWS / MIT-style). Both are C++.

---

## Hosting SDKs — CLAP Host Support and Rust Crates

### CLAP built-in host support
Sources:
- https://github.com/free-audio/clap
- https://cleveraudio.org/developers-getting-started/

CLAP's design is symmetric: the same header (`clap/clap.h`) defines both plugin and host interfaces. A CLAP host implements `clap_host_t` with callbacks (`request_process`, `request_callback`, `get_extension`, `request_restart`). Hosting is therefore simpler than VST3 (no COM-like reference-counted object graph). The `clap_host_log`, `clap_host_thread_check`, `clap_host_params`, `clap_host_gui`, `clap_host_audio_ports`, `clap_host_note_ports` extensions cover almost all host responsibilities.

### Rust CLAP host crate: clack
Sources:
- https://github.com/prokopyl/clack
- https://github.com/MeadowlarkDAW/clack
- https://kwarf.com/2024/07/writing-a-clap-synthesizer-in-rust-part-1/

**Clack** provides `clack-host` and `clack-plugin` + `clack-extensions`. Low-level but safe. Author claims it is the only working Rust CLAP host wrapper. Design: plugin instances wrap CLAP plugin handles; process-call API is type-safe over raw CLAP buffers. Extensions are explicit opt-in via `clack-extensions`.

Basic flow for a host:
1. Load bundle (`clack_host::factory::PluginFactory::entry`).
2. Instantiate plugin with a `HostInfo` struct (plugin sees caller name, version).
3. Activate with sample rate and frame counts.
4. Per-block: call `process` with input/output buffers and an `Events` list for parameter changes.

### Rust VST3 host crates
Sources:
- https://github.com/RustAudio/vst3-sys
- https://crates.io/crates/vst3
- https://github.com/jesnor/vst3-rs
- https://lib.rs/crates/plugin_host
- https://crates.io/crates/rack

Rust VST3 hosting is more fragmented:
- **vst3-sys** (RustAudio): raw COM bindings. Unsafe, unmaintained.
- **vst3-rs** (jesnor): safe wrapper over vst3-sys. Low stars, unclear maintenance.
- **vst3** crate on crates.io: pure binding generator, user supplies SDK.
- **cutoff-vst**: aims to be a robust safe host API.
- **plugin_host** and **rack** crates: higher-level, wrap CLAP + VST3 + AU. `rack` crate (MIT) is the most ambitious cross-format Rust host crate, as of 2026 it is actively maintained and exposes `Plugin` + `Host` traits.

Practical conclusion: **for a Rust-based rack, hosting CLAP via clack is clean and viable; hosting VST3 is doable but the Rust ecosystem is thin**. Many Rust rack projects either focus on CLAP-only or embed JUCE/VST3SDK via FFI for VST3 support.

### NIH-plug multi-bus / sidechain examples
Sources:
- https://github.com/robbert-vdh/nih-plug
- https://github.com/robbert-vdh/nih-plug/tree/master/plugins/crossover
- https://github.com/robbert-vdh/nih-plug/tree/master/plugins/spectral_compressor
- https://nih-plug.robbertvanderhelm.nl/nih_plug/plugin/trait.Plugin.html

NIH-plug models multi-bus and sidechain through the `AudioIOLayout` struct:

```rust
const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
    main_input_channels: NonZeroU32::new(2),
    main_output_channels: NonZeroU32::new(2),
    aux_input_ports: &[new_nonzero_u32(2)],     // sidechain
    aux_output_ports: &[new_nonzero_u32(2); 4], // multi-out
    names: PortNames { ... },
}];
```

At process time, `process(&mut self, buffer: &mut Buffer, aux: &mut AuxiliaryBuffers, ctx: &mut impl ProcessContext)` — `aux.inputs` is the sidechain buffers, `aux.outputs` is the extra bus buffers.

The **crossover** plugin is the clearest example of multi-out bus handling: main output silent, bands go to `aux.outputs[0..N]`. Requires DAW-side setup to wire up the aux outputs to other tracks/buses.

The **spectral_compressor** plugin is the clearest example of sidechain handling: reads `aux.inputs[0]` as a modulator for its spectral envelope.

NIH-plug is **the most idiomatic Rust framework** for plugin development. For a rack plugin:
- Main I/O = the rack's main stereo signal.
- `aux_input_ports` = external sidechain (to feed hosted compressors' sidechain inputs).
- `aux_output_ports` = direct outs per channel strip (if the rack implements per-track internal strips and wants to send them to separate DAW buses).

MIT/ISC licensed; very permissive.

---

## Other related art

### discoLink (2024-2026)
Sources:
- https://www.kvraudio.com/product/discolink-by-discodsp
- https://github.com/reales/discolink

Open-source C++17 library (MIT) specifically for cross-plugin shared-memory IPC. Uses:
- Lock-free SPSC ring buffer, 16384 samples per channel, cache-line-aligned atomics.
- Shared-memory "bulletin board" with 16 slots; devices register on load, hosts scan and auto-discover.
- Supports VST3 and AU on macOS, Linux, Windows.

**Direct open-source reference for FabFilter-style inter-instance linking.** Exactly the pattern a new rack/console plugin could embed for its linking layer.

### HISE / JUCE InterProcessConnection
Sources:
- https://forum.juce.com/t/communication-between-different-plugins-within-the-same-host/4030
- https://forum.hise.audio/topic/1056/api-for-interprocess-communications/1

JUCE ships `InterProcessConnection` using named pipes or TCP sockets. Slower than shared memory but cross-process, useful when the DAW sandboxes plugins (Bitwig's per-plugin process option, for example).

---

## Patterns to Borrow

Here is the distillation of the above, organized by subsystem, for a new rack/console plugin:

### Hosting model
1. **Fixed-shape chain with parallel/multiband escape hatches.** StudioRack's "8 plugins serial + 8 parallel + crossover-in-any-slot" model is the sweet spot between "linear channel strip" (Console 1) and "free graph" (Metaplugin). Users find linear chains easy to reason about; the parallel/multiband modes cover the 20% cases without making 80% cases complicated.
2. **Bounded slot count per chain, not unbounded.** 8 or 16. Unbounded lists breed UX chaos (PatchWork's 64-slot matrix is impressive but intimidating). StudioRack, Snap Heap, and MB-7 Mixer all cap at ~7-8 per dimension and it works.
3. **Bundle first-party "utility" DSP blocks.** Metaplugin ships a mid-side matrix, crossover, and routing plugin. These are the 90% of what users actually want to glue between third-party plugins. Ship them free, ship them first-party, ship them with consistent look.

### GUI
4. **Open sub-plugin GUIs in floating windows, persist positions in session.** Nobody has cracked true embedding of arbitrary third-party VST3 UIs; every commercial product gives up and uses floating windows. Do this but (a) remember position per-slot per-session, and (b) offer a "tile" layout that auto-arranges them.
5. **First-party modules embed directly; third-party float.** The Kilohearts / iZotope UX cohesion only works for closed ecosystems. Acknowledge this: build a small set of first-party modules that render inline, and put third-party VST3 in floating windows.
6. **Ring-style modulation visualization on destination knobs.** Kilohearts and Bitwig both do this; it is the single best idea in modern rack UI for making modulation legible.
7. **Resize the shell, not the sub-plugin.** Ozone's approach. The rack's own chrome (slot rail, macro panel, meter strip) scales with window size; sub-plugin windows remain at their native size.

### Parameter exposure
8. **Macros-to-DAW, inner-params-by-mapping.** Every serious product does this: rack exposes a fixed, small set of automatable parameters (macros + shell controls), and users map inner plugin params onto macros. Never expose the flat union of every inner plugin's parameters — that way lies an 8000-parameter automation lane.
9. **Normalized parameter schema per slot role.** Console 1's pattern: a slot marked "Compressor" always exposes `Threshold / Ratio / Attack / Release / Makeup` regardless of which compressor implementation is loaded. DAW automation survives a swap of the underlying DSP. Borrow this for a channel-strip product.
10. **Save full sub-plugin state as opaque blobs.** Reaper `.RfxChain` + StudioRack preset model. The rack's state file is a thin envelope that carries each sub-plugin's `getState()` byte-blob verbatim, plus the macro map. No attempt to semantically understand sub-plugin state.

### Inter-instance linking
11. **Tag plugin on every track, not auto-discovery.** iZotope Relay's pattern. User explicitly instantiates a small tag plugin per track. Clean, DAW-agnostic, no track-enumeration hacks. Relay also demonstrates that the tag plugin can itself do useful DSP (gain/pan/HPF), so it is not dead weight.
12. **Shared-memory registry + SPSC ring buffers per instance.** discoLink is the open-source reference. Bulletin board in `/dev/shm` (or Windows equivalent), each instance registers on load and emits parameter/meter updates at audio rate to a ring buffer other instances can read lock-free.
13. **One "controller" instance, many "followers".** FabFilter Pro-Q 4 model: any instance can become the controller for the session by opening the Instance List panel. Followers publish their spectra; the controller renders them all and can remote-edit. No central daemon needed; the controller is just whichever instance the user is looking at.
14. **Don't expose linked state as new DAW parameters.** Linking is a GUI/DSP-reference layer, not a parameter-model layer. Keeps automation semantics per-instance and avoids "which instance owns this curve?" ambiguity.

### Implementation
15. **CLAP-first for Rust.** `clack-host` (https://github.com/prokopyl/clack) is workable and actively maintained. VST3 hosting in Rust is painful. If the rack must host both: CLAP via clack, VST3 via JUCE FFI or via `rack` / `cutoff-vst` crates.
16. **NIH-plug for the outer plugin.** `aux_input_ports` for external sidechain passthrough to hosted plugins; `aux_output_ports` for direct-outs if the rack implements multi-strip behavior; `AudioIOLayout` declares all of this at compile time. MIT-licensed and doesn't drag in JUCE.
17. **Use JUCE AudioProcessorGraph as a reference architecture.** Even if implementing in Rust, the JUCE `AudioProcessorGraph` + `AudioGraphIOProcessor` pattern (special node types for audio-in / audio-out / MIDI-in / MIDI-out at the graph boundary, generic nodes for plugin instances, typed connections) is the simplest and most battle-tested interior model.
18. **Plugin Delay Compensation is non-negotiable.** Metaplugin, PatchWork, StudioRack all implement automatic PDC across the chain. A rack that doesn't sum-align latencies across parallel branches will be immediately exposed on any kick-drum parallel-compression test.

### What nobody has cracked well
- **True embedded third-party VST3 GUIs** (no floating windows): limited by VST3's `IPlugView` being designed to own a native window. Everyone gives up.
- **Lossless automation when swapping the sub-plugin in a slot**: Console 1's normalized schema partially solves it for channel-strip roles; general case is unsolved.
- **Cross-DAW session linking**: iZotope's IPC is per-process, FabFilter's is per-process. Nobody links instances across two different DAWs on the same machine. (discoLink's shared-memory approach could in principle do it.)

---

## Quick comparison matrix

| Product | Price | OSS | Hosts 3rd-party | Parallel | Multiband | Macros | Inter-inst link |
|---|---|---|---|---|---|---|---|
| Waves StudioRack | Free | No | VST3 | Yes (8) | Yes | 8/chain | No |
| Blue Cat PatchWork | $99 | No | VST/VST3/AU (64) | Yes (8) | Via MB-7 | Param map | Via MB-7 |
| Blue Cat MB-7 Mixer | $129 | No | VST/VST3/AU (4/band) | Yes (bands) | Yes (7) | Param map | **Yes** |
| DDMF Metaplugin | $59 | No | VST/VST3/AU | Graph | Via modules | 100 params | No |
| Kilohearts Snap Heap | Free | No | Snapins only | Yes (7) | No | 8 | No |
| Kilohearts Multipass | $99 | No | Snapins only | Yes | Yes (5) | 8 | No |
| FabFilter Pro-Q 4 | €169 | No | N/A | N/A | N/A | N/A | **Yes** |
| iZotope Nectar 4 | $249 | No | First-party only | No | No | N/A | Via Relay |
| iZotope Ozone 12 | ~$499 | No | First-party only | No | No | N/A | Via Relay |
| iZotope Relay | Free | No | N/A (tag plugin) | N/A | N/A | N/A | **Yes** |
| UAD Console | Free+HW | No | UAD only | Mixer | No | No | N/A (1 per HW) |
| Softube Console 1 | $249+HW | No | "Console 1 Ready" | No | No | No | **Yes (via HW+svc)** |
| Bitwig Grid | DAW | No | VST/CLAP | Containers | Crossover mods | Macros | N/A |
| Reaper FX Chain | DAW | No | Any | Via routing | Via JSFX | Parameter Modulation | None native |
| JUCE AudioProcessorGraph | OSS | GPL/commercial | Any (you write) | Graph | You write | You write | You write |
| iPlug2 | OSS | MIT-style | Not built in | You write | You write | You write | You write |
| NIH-plug | OSS | MIT/ISC | No (framework) | N/A | N/A | N/A | N/A |
| clack (Rust CLAP host) | OSS | MIT | CLAP | You write | You write | You write | You write |
| discoLink | OSS | MIT | N/A (library) | N/A | N/A | N/A | **Yes (lib)** |

---

## Primary URLs (consolidated)

Commercial rack/console plugins:
- Waves StudioRack: https://www.waves.com/plugins/studiorack
- Blue Cat PatchWork: https://www.bluecataudio.com/Products/Product_PatchWork/
- Blue Cat MB-7 Mixer: https://www.bluecataudio.com/Products/Product_MB7Mixer/
- DDMF Metaplugin: https://ddmf.eu/metaplugin-chainer-vst-au-rtas-aax-wrapper/
- Kilohearts Snap Heap: https://kilohearts.com/products/snap_heap
- Kilohearts Multipass: https://kilohearts.com/products/multipass
- FabFilter Pro-Q 4: https://www.fabfilter.com/products/pro-q-4-equalizer-plug-in
- FabFilter Instance List docs: https://www.fabfilter.com/help/pro-q/using/instance-list
- iZotope Nectar 4: https://www.izotope.com/en/products/nectar
- iZotope Ozone 12: https://www.izotope.com/en/products/ozone
- iZotope Relay: https://www.izotope.com/en/products/insight/features/relay
- UAD Console: https://www.uaudio.com/products/uad-console-app
- Softube Console 1 Mk III: https://www.softube.com/us/console-1-mixing-system-mk-iii

DAW references:
- Bitwig Grid: https://www.bitwig.com/the-grid/
- Bitwig nested device chains: https://www.bitwig.com/userguide/latest/advanced_device_concepts/
- Reaper ReaPlugs: https://www.reaper.fm/reaplugs/
- Reaper JSFX: https://www.reaper.fm/sdk/js/js.php

Hosting SDKs and Rust crates:
- CLAP spec: https://github.com/free-audio/clap
- clack: https://github.com/prokopyl/clack
- clack (Meadowlark fork): https://github.com/MeadowlarkDAW/clack
- JUCE AudioProcessorGraph tutorial: https://juce.com/tutorials/tutorial_audio_processor_graph
- JUCE AudioProcessorGraph class: https://docs.juce.com/master/classAudioProcessorGraph.html
- iPlug2: https://iplug2.github.io/
- NIH-plug: https://github.com/robbert-vdh/nih-plug
- NIH-plug crossover: https://github.com/robbert-vdh/nih-plug/tree/master/plugins/crossover
- NIH-plug spectral_compressor: https://github.com/robbert-vdh/nih-plug/tree/master/plugins/spectral_compressor
- vst3-sys: https://github.com/RustAudio/vst3-sys
- vst3-rs: https://github.com/jesnor/vst3-rs
- rack crate: https://crates.io/crates/rack
- discoLink: https://github.com/reales/discolink
