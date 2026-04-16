# Inter-Instance Linking for Audio Plugins

**Research date:** April 16, 2026
**Context:** VST3 plugin rack in Rust (nih_plug). A single VST3 instance can only
process one track's audio. To present two instances on two Bitwig tracks as one
"mixing console" UI, we need cross-instance linking. This document surveys prior
art, Rust crates, real-time-safety concerns, and lands on a recommended
architecture.

---

## 1. Prior art: how commercial plugins link instances

### 1.1 FabFilter Pro-Q 3 / Pro-Q 4

Pro-Q has had auto-discovered inter-instance communication since Pro-Q 3
(2018) and expanded it in Pro-Q 4.10 (2026) with a multi-plugin Instance List.
Behaviour, as documented by FabFilter and reviewers:

- Every Pro-Q instance in a session advertises itself.
- Any instance can pick another from a menu and superimpose its spectrum.
- Collision detection highlights overlapping bands across tracks.
- Track-name display requires VST3 or AU (needs host-provided track-name API,
  which VST2 does not expose).

FabFilter does not publish the mechanism. What the feature *implies* technically:

- It works across processes (some DAWs sandbox plugins; Bitwig has a bit-bridge
  and per-plugin sandbox option).
- It works even when the host does not wire any audio between tracks.
- Discovery is automatic — no user-entered group ID.

The strong signal from other plugins in this class (below) is that this is a
shared-memory bulletin board keyed on a well-known name. There is no public
teardown that enumerates FabFilter's exact calls, but the behaviour matches
`shm_open` / `CreateFileMapping` with a registry slot table almost exactly.

Sources:

- https://www.fabfilter.com/news/1768467600/fabfilter-introduces-multi-plugin-instance-list-in-pro-q-410
- https://www.pluginboutique.com/articles/1420-FabFilter-Pro-Q-3-Review-of-Key-New-Features
- https://www.fabfilter.com/forum/topic/5901/use-pro-q-3-instances-as-multiple-side-chain-sources

### 1.2 iZotope Relay / Ozone / Neutron / Insight (IPC)

iZotope brands this as "Inter-Plugin Communication" (IPC). Relay is a
near-transparent utility plugin whose sole job is to *publish* metering and
receive control data so that Insight, Neutron, Nectar, Tonal Balance Control,
etc. can consume it.

Public docs never name the transport, but the feature set tells us:

- Sends per-channel meter / spectrum data at GUI rates (tens of Hz).
- Sends audio (or at least feature vectors — loudness, spectra, LUFS windows)
  to Neutron's Visual Mixer.
- Remote-controls Relay gain/pan from Visual Mixer: bidirectional.
- Works across tracks in any DAW that will load two iZotope plugins.
- Installed as a system-wide helper (Splice even ships it separately), which
  suggests a shared service file and/or a fixed shared-memory prefix.

Again, almost certainly POSIX shm / CreateFileMapping with a directory segment
plus per-instance ring buffers.

Sources:

- https://www.izotope.com/en/learn/inter-plugin-communication-explained.html
- https://s3.amazonaws.com/izotopedownloads/docs/relay101/en/ipc/index.html
- https://www.izotope.com/en/products/insight/features/relay.html
- https://support.splice.com/en/articles/8652846-what-is-relay-and-why-am-i-seeing-it-installed-in-the-splice-app

### 1.3 Sonible smart:limit, smart:EQ group

Sonible exposes "group" linking: an instance can join a named group, and
instances in a group share sidechain-like loudness information and can gate
each other. Their docs do not describe the transport. The behaviour (named
group, persists across save/reload, works cross-track) is consistent with a
per-group shared memory segment whose name is derived from the user-entered
group ID.

Sources:

- https://www.sonible.com/smartlimit/
- https://help.sonible.com/hc/en-us/sections/4412294263058-smart-limit

### 1.4 discoLink by discoDSP (fully-documented, open-source C++17)

This is the most useful reference point because the source is on GitHub and
the architecture is explicitly described.

- **Transport:** POSIX `shm_open` on macOS/Linux, Win32 `CreateFileMapping` on
  Windows.
- **Audio channel:** lock-free SPSC ring buffer, 16,384 samples per channel,
  cache-line-aligned atomics, no mutexes, no blocking, no allocations in the
  audio path.
- **Control channel:** 256-byte fixed-size messages in a 256-slot ring.
- **Discovery:** "shared memory bulletin board with 16 slots." Each running
  device writes its `hostPid` + a `linkTag` into a slot on register. Hosts
  scan the board. Dead PIDs are GC'd.
- **Persistence:** the `linkTag` is stored in plugin state so that when the
  DAW reloads, an instance rejoins the same logical link.
- **Handshake:** protocol version, capabilities, sample rate, block size,
  channel count, buffer size.

Sources:

- https://github.com/reales/discolink
- https://www.discodsp.com/discolink/
- https://www.kvraudio.com/product/discolink-by-discodsp

### 1.5 Waves StudioRack

Different model — StudioRack is a plugin *chainer* with a proprietary internal
bus. Its cross-track linking is done via "StudioVerse" chains, not a live IPC
channel between two instances on two tracks. Less relevant to our design but
worth noting as an alternative product shape.

Source: https://www.waves.com/plugins/studiorack

### 1.6 KVR developer consensus

The recurring thread "Sending info between multiple instances of the same VST3
plugin" on KVR lands on the same answers every few years:

1. Shared memory if you need low latency or audio.
2. Named pipes / local sockets if you only need control or GUI data.
3. Don't use a file on disk; it works but you'll regret the poll + journal IO.
4. Boost.Interprocess is the usual C++ cheat; discoLink and shmem-ipc are the
   native ones.

Sources:

- https://www.kvraudio.com/forum/viewtopic.php?t=595782
- https://forum.juce.com/t/communication-between-different-plugins-within-the-same-host/4030
- https://forum.cockos.com/archive/index.php/t-182434.html

---

## 2. Rust shared-memory crates — a comparison

| Crate            | Cross-platform | Backing               | RT-safe hot path | Notes                                                    |
|------------------|----------------|-----------------------|------------------|----------------------------------------------------------|
| `shared_memory`  | yes            | POSIX shm / Win32 FM  | yes (after open) | Convenient, little maintenance since 2022; solid for MVP |
| `raw_sync`       | yes            | POSIX / Win32 sems    | no on wait       | Companion to shared_memory; use only for non-RT signals  |
| `memmap2`        | yes            | mmap over file/fd     | yes (after mmap) | Lowest-level; you wire naming + lifetime yourself        |
| `interprocess`   | yes            | local sockets + pipes | no               | Great for GUI channel; not for audio                     |
| `shmem-ipc`      | Linux only     | memfd + eventfd       | yes              | Best for untrusted; does not help us on macOS/Windows    |
| `fdringbuf-rs`   | Linux only     | eventfd signalling    | yes              | Superseded by shmem-ipc                                  |
| `ipmpsc`         | yes            | memmapped file        | "mostly"         | MPSC across processes; convenient, heavier than SPSC    |

### 2.1 `shared_memory` crate

Pros:

- Clean wrapper around `shm_open`/`CreateFileMapping`. Single API to name and
  open a segment by string.
- After open, you have a `&mut [u8]` — then all your RT-safe structure work
  is ordinary atomics on that byte slice.
- Handles the Windows-specific "backing file is reclaimed when last handle
  closes" vs POSIX "needs shm_unlink" difference for you.

Cons:

- Creator-vs-opener semantics leak a bit (creation can race).
- Size is fixed at creation (we want that anyway for RT).
- Crate maintenance has slowed; audit for 2026 before picking.

### 2.2 `memmap2` + hand-rolled `shm_open`

Pros:

- Most direct. On macOS (our primary concern for sandboxing) you want POSIX
  `shm_open` with an App Group prefix — rolling it yourself is trivial FFI.
- Works with any ring-buffer layout you want.

Cons:

- You write the cross-platform goop (Windows `CreateFileMappingW` + 16-bit
  wide-string path munging).
- Easy to misname segments across versions and leak stale ones.

### 2.3 `interprocess`

Pros:

- Excellent ergonomics for local sockets / named pipes.
- Tokio integration for async GUI code.

Cons:

- No raw shared memory.
- RT-unsafe: sockets do syscalls, potentially block, copy.

**Use for:** GUI-path linking (meters at 30 Hz, parameter edits), never for
audio-rate data.

### 2.4 `shmem-ipc` / `fdringbuf-rs`

Linux-only today. If we ship on Linux as a first-class target, these are
excellent — `memfd_create` + sealing makes untrusted-process safety cheap, and
eventfd lets a consumer park on a descriptor rather than spin.

### 2.5 Recommendation

For the audio-rate channel: **`shared_memory` crate** for the segment + a
hand-written SPSC ring atop the segment's `&mut [u8]`. For the control/GUI
channel: **`interprocess`** local sockets. This gives us a single-crate audio
path and a single-crate GUI path, both cross-platform.

---

## 3. Real-time safety on the shared memory

### 3.1 The rules

An audio callback running at 48 kHz / 128-sample blocks has a ~2.6 ms
budget. In that budget you must not:

- Allocate on the heap.
- Take a lock that a non-RT thread holds.
- Call any syscall that can block (`write`, `send`, `futex_wait`, anything
  that can page-fault into disk IO, etc.).
- Busy-wait on shared state without bound.

What you *can* do:

- Read and write already-mapped memory.
- Use atomic load/store/CAS.
- Swap pointers.
- Use a lock-free SPSC ring buffer whose backing memory is pre-mapped.

### 3.2 SPSC vs MPSC

We almost certainly have exactly one audio thread per instance, and exactly
one consumer (the linked instance). So SPSC is natural: the *cheapest* and
simplest wait-free primitive. If we want N-to-N (rack of M instances all
reading each other), we set up N(N-1) SPSC pipes or one MPSC-per-consumer.
SPSC-per-edge scales fine up to ~16 instances and stays wait-free on both
sides.

`rtrb` is the canonical Rust SPSC ring for audio (intra-process). For
cross-process we re-implement the same algorithm over a shared-memory slab —
trivially, because `rtrb`'s internals are just two `AtomicUsize` head/tail
pointers plus a fixed-size data region.

### 3.3 Ring buffer layout

```text
struct RingHeader {
    write_idx: AtomicU64,   // producer-owned, monotonically increasing
    _pad1: [u8; 56],        // cache-line pad
    read_idx: AtomicU64,    // consumer-owned
    _pad2: [u8; 56],
    capacity: u64,          // fixed at creation; power of two
    sample_rate: u32,
    channels: u16,
    flags: u16,
}
// followed by capacity * channels * f32 audio samples
```

Key points:

- Indices are `u64` not `usize` so the layout is stable across 32/64-bit
  mixed processes (unlikely but cheap to future-proof).
- Separate cache lines for write_idx and read_idx — otherwise producer and
  consumer fight for the same line and throughput craters.
- Capacity is a power of two so modulus is a mask.
- `write_idx` / `read_idx` are *monotonic*, wrapping occurs only at read time
  via `idx & (capacity - 1)`. That way full vs empty is unambiguous.

### 3.4 Atomic ordering

- Producer stores samples, then `store(write_idx, Release)`.
- Consumer loads `write_idx` with `Acquire`, reads samples.

This gives a proper happens-before and is the pattern every SPSC ring uses.

### 3.5 Crate survey for the RT side

| Crate              | SPSC | Wait-free | In shared mem | Verdict                        |
|--------------------|------|-----------|---------------|--------------------------------|
| `rtrb`             | yes  | yes       | no (heap)     | Best intra-process reference   |
| `ringbuf`          | yes  | yes       | no            | Also good; more features       |
| `crossbeam-channel`| MPMC | no (uses parking) | no    | Not RT-safe; GUI path only     |
| `shmem-ipc::sharedring` | yes | yes | yes          | Linux only                     |

So we roll our own small `ShmRing` modelled after `rtrb` but parameterised
over the shared-memory slice.

---

## 4. Process / instance discovery

### 4.1 The problem

Instance A in Bitwig loads, starts up, wants to know: are there other
instances of me in this DAW session? If so, which ones, and how do I talk to
them?

### 4.2 Candidate strategies

1. **Fixed, process-wide name.** All instances agree on a single segment
   name, say `/plugin-rack-registry`. They race to create it; whoever wins
   is the owner; others attach. Inside the registry is a slot table of
   active instances.

2. **Per-DAW-host-PID name.** The plugin at startup looks up its parent PID
   (the DAW), then composes `/plugin-rack-<host_pid>-registry`. This isolates
   two concurrent DAW sessions on the same machine.

3. **Per-DAW-project name.** Hash the DAW project path / project name to a
   stable suffix. Survives reload. Problem: we rarely have the project path
   from inside a plugin.

4. **User-entered group ID.** Sonible's approach. Simple, explicit, survives
   reload trivially because the ID is stored in plugin state.

### 4.3 Our recommendation

- **Primary:** per-host-PID registry segment. The host PID is trivially
  available (`std::process::id()` gives us our own; the parent PID on
  macOS/Linux via `libc::getppid()`, on Windows via `ToolHelp32` or
  `NtQueryInformationProcess`). Since plugins run in the host process
  itself in most DAWs, `std::process::id()` *is* the host PID — it differs
  only under sandboxing/bridging.
- **Secondary:** a stable `link_tag` persisted in plugin state. The tag
  says "I want to be in group X" and survives reload; when the registry
  for the new host PID comes up empty, we re-announce under our old tag
  and other instances with the same tag find us.
- **Fallback:** let the user set a group name in the UI. Belt + braces for
  the rare case where auto-detection fails (e.g. bit-bridging plugin
  sandbox puts us in a different PID tree than siblings).

### 4.4 Registry slot layout

```text
struct Registry {
    magic: [u8; 8],                 // "PLRACK\0\1"
    version: u32,
    slot_count: u32,
    slots: [Slot; 32],
}

struct Slot {
    alive: AtomicU64,               // 0 = empty, else unix nanos of last heartbeat
    pid: u32,
    instance_uuid: [u8; 16],        // random at instance creation
    link_tag: [u8; 32],             // user/persistent group tag, NUL-padded
    ring_name: [u8; 64],            // name of the per-instance audio/ctrl ring
    role_flags: u32,                // producer/consumer/etc
    _pad: [u8; 4],
}
```

Each instance claims a slot via CAS on `alive`, writes identifying data,
then heartbeats every ~500 ms from its GUI / timer thread (never the audio
thread). A scanning instance treats any slot whose `alive` is older than
~2 s as dead and may reclaim it.

### 4.5 Dead-process GC

PID-based reaping has a race: PIDs are reused. Hence the heartbeat-based
approach — if an instance hasn't stamped `alive` in 2 s, its slot is reclaimed
regardless of whether the PID is still taken by somebody else. Combined
with the `instance_uuid`, a stale slot cannot be mistaken for a live one.

---

## 5. GUI linking — the slow, well-behaved channel

The audio path and the GUI path have different requirements and should use
different transports.

| Path  | Rate              | Latency target | Transport                        |
|-------|-------------------|----------------|----------------------------------|
| Audio | per-sample / per-block (44.1–192 kHz) | < 1 block | Shared memory + SPSC ring |
| GUI   | 30 Hz meters, occasional param edits | 50 ms | Local socket / named pipe |

Why split? The audio thread *must* avoid syscalls. The GUI thread *should*
avoid shared-memory polling so we can block on it efficiently and we get
change notifications cheaply.

Options for the GUI channel:

- `interprocess::local_socket` — cross-platform, unix domain on *nix, named
  pipe on Windows. Recommended.
- Localhost TCP — simple, universal, but goes through the network stack. Fine,
  mildly wasteful.
- OSC over UDP on localhost (`rosc`) — human-readable, great if you ever
  want external controllers. Slight overkill for a private protocol between
  two copies of the same plugin; accept it if you plan to open the protocol
  for external tools later.
- Another shared-memory segment with a condvar — lowest latency, highest
  complexity. Don't.

We recommend **`interprocess` local sockets** carrying length-prefixed
`bincode`-encoded messages. Use `rosc` only if OSC discoverability from
external tools is a roadmap item.

---

## 6. Session persistence

### 6.1 The problem

On Save, the DAW writes plugin state and closes the plugins. On Reload, the
DAW instantiates fresh plugin copies and feeds them their saved state. The
shared memory segments and sockets are gone. How do instances rediscover
each other?

### 6.2 Approach

- Every instance persists in its plugin state: `instance_uuid` and
  `link_tag` (group membership).
- On `activate()`, instance:
  1. Opens (or creates) the per-host-PID registry.
  2. Claims a slot, writes `instance_uuid` + `link_tag` + fresh `ring_name`.
  3. Scans other slots; anything whose `link_tag` matches is a sibling and
     we open its ring.
- Race: if two instances load "simultaneously" and neither sees the other
  yet, both scan, neither finds, both wait briefly. This is fine — the
  scan is cheap, we can re-scan every heartbeat for a few seconds.

### 6.3 Linking by project path

Tempting, but in VST3 / AU / CLAP there is no reliable API for "what is my
DAW project file path?". Host-supplied metadata like `IHostApplication::getName()`
gives the DAW name but not the project. Avoid.

### 6.4 User-assigned group as the canonical join key

Sonible's model. We adopt it as the *explicit* overlay: if the user has
dragged instance A and instance B into the same visual console, we assign
them the same `link_tag`. That tag is the ground truth — PID and uuid are
just plumbing.

---

## 7. Security

### 7.1 Same-user threat model

All instances run as the same user, inside the DAW, loaded from the same
plugin bundle. An attacker who can inject a malicious plugin into the user's
DAW has already won. So the in-DAW threat model is "we trust our siblings."
We do not need memfd sealing or untrusted-IPC primitives.

### 7.2 macOS App Sandbox

Per Apple's IPC/sandbox docs:

- System V IPC (`shmget`) is discouraged and awkward under the sandbox.
- POSIX shared memory (`shm_open`) is allowed, but the name **must start
  with the App Group identifier** for sandboxed callers, e.g.
  `GROUP_ID/plugin-rack-registry`.
- POSIX semaphores follow the same naming rule.
- Mach ports are the "blessed" cross-sandbox channel but far more complex.

Practical impact for us:

- AU v3 plugins (Audio Unit extensions) are always sandboxed. Shared memory
  between them requires both instances declaring the same App Group
  entitlement and using the App-Group-prefixed name.
- VST3 plugins loaded by an unsandboxed DAW like Bitwig are not sandboxed
  themselves — they inherit the host's rights. `shm_open("/plugin-rack-*")`
  just works.
- If we ship an AU v3 build, add a single App Group, prefix names, ship.

Source: https://developer.apple.com/forums/thread/719897

### 7.3 Windows

`CreateFileMappingW` is fine in a single-user session. For cross-session
work (e.g. services), the `Global\` namespace is restricted; we do not
need it. We use `Local\plugin-rack-*` or an unprefixed name which defaults
to the caller's session.

### 7.4 Linux

Standard POSIX shm in `/dev/shm/`. Permissions default to 0600; no action
needed.

---

## 8. Prior-art Rust audio IPC

- **Bitwig controller extensions** are Java, not Rust. No Rust SDK.
- **DrivenByMoss** — Java Bitwig extension collection with OSC support. Not
  a library we can consume, but the OSC schema is a good reference for "what
  does a DAW actually want to say over a wire."
- **nih-plug** itself — no built-in inter-instance story. `nih-plug`'s
  wrapper code confirms each VST3 instance gets its own `Plugin` and
  `Arc<Params>` with no cross-instance pathway; if we want one, we build
  it.
- **`cpal`** — audio device abstraction only. Not inter-plugin.
- **`shmem-ipc` (diwic)** — Linux-only; used internally by some PipeWire
  integrations.
- **`ipmpsc`** — an MPSC across processes crate, used in some Rust agent
  frameworks. Not audio-tuned but real-time-ish.
- **No Rust crate specifically for cross-plugin audio IPC** as of April 2026.
  discoLink's model remains unported. This is genuine greenfield.

Sources:

- https://github.com/robbert-vdh/nih-plug
- https://github.com/diwic/shmem-ipc
- https://github.com/dicej/ipmpsc

---

## 9. OSC over localhost for GUI sync

### 9.1 Pros

- Textual, self-describing, trivial to inspect with any OSC monitor.
- Opens a side door for external controllers: TouchOSC, OSC/Pilot, a web UI,
  etc. can drive the console GUI without a custom protocol.
- `rosc` is mature pure-Rust, no heavy deps.

### 9.2 Cons

- UDP: occasional packet loss. Fine for meters (next packet comes in 33 ms).
  Not fine for param edits unless we add sequence numbers and retransmit.
- Discovery: OSC has no native discovery; we'd still need the shared-memory
  registry to tell instances each other's ports.
- Schema drift: every field change is a compat negotiation; in-process
  bincode lets the compiler do the work.

### 9.3 Verdict

Overkill for the private channel between two of our own instances.
Right-sized only if we *want* third-party tools to drive the console.
Recommend: start with `interprocess` + bincode; expose an optional OSC
facade later if users ask.

Sources:

- https://github.com/klingtnet/rosc
- https://lib.rs/crates/rosc

---

## 10. Latency targets & feasibility

### 10.1 GUI-path linking

30 Hz update rate for meters, spectra, gain-reduction dots. Budget per
message ~33 ms. `interprocess` local socket can deliver a 4 KiB message
in under 100 µs. Non-issue.

### 10.2 Audio-path linking

Two cases:

**A. Analysis-only (what FabFilter/iZotope do).**
Instance A computes an FFT frame, Instance B wants to display it. Latency
of a full block (2–10 ms) is fine — the user is not hearing the output of
this data path. Shared-memory SPSC handles this trivially.

**B. Actual audio flow (parallel compression across tracks, multiband
  unmasking, etc.).**
Instance A's *processed audio* must feed Instance B's compressor *in the
same audio block*. This is where the DAW graph fights us:

- The DAW runs each track in some order it decides. If B's track runs
  before A's track, B sees A's previous block, not current.
- Within a single block, we cannot introduce a wait between A finishing and
  B starting without adding one full block of latency.
- Most DAWs do not let a plugin declare "I need to run after this other
  track" — they only have the host-level bus routing for that.

Practical options in increasing latency and increasing reliability:

1. **Accept one-block latency.** B uses A's *previous* block. 2.6 ms at
   48k/128 and the user likely can't hear it; for sidechain compression
   the detector is usually lookahead-less anyway.
2. **Require an explicit sidechain routing** set up by the user. Then we
   get synchronous audio via the host and we use IPC only for metadata.
3. **Own-thread ASIO-style shenanigans.** Don't. You will lose.

### 10.3 Sub-buffer latency

Not possible inside the plugin graph without rewriting the host. Forget it.
Design around (1) and (2).

---

## 11. RT-safe shared memory code sketch (Rust)

Minimal, production-shaped, but still a sketch — error paths elided, the
real thing needs proper cleanup and platform-specific shm naming.

```rust
use std::sync::atomic::{AtomicU64, Ordering};
use std::mem::size_of;

// ---------- Layout ----------

#[repr(C, align(64))]
pub struct RingHeader {
    pub write_idx: AtomicU64,
    _pad1: [u8; 64 - size_of::<AtomicU64>()],
    pub read_idx: AtomicU64,
    _pad2: [u8; 64 - size_of::<AtomicU64>()],
    pub capacity: u64,          // in samples; power of two
    pub channels: u32,
    pub sample_rate: u32,
    pub flags: u32,
    _pad3: [u8; 36],
}

const _: () = assert!(size_of::<RingHeader>() % 64 == 0);

// ---------- Producer (audio thread) ----------

pub struct ShmRingProducer {
    header: *mut RingHeader,
    data: *mut f32,             // capacity * channels samples
    capacity_mask: u64,
    channels: u64,
}

unsafe impl Send for ShmRingProducer {}

impl ShmRingProducer {
    /// Try to push `frames` interleaved samples. Returns number written.
    /// RT-safe: no allocation, no syscalls, no locks.
    #[inline]
    pub fn push(&mut self, block: &[f32]) -> usize {
        let header = unsafe { &*self.header };
        let cap = self.capacity_mask + 1;
        let ch = self.channels;
        let frames_in = (block.len() as u64) / ch;

        let write = header.write_idx.load(Ordering::Relaxed);
        let read = header.read_idx.load(Ordering::Acquire);
        let available_frames = cap - (write - read);

        let to_write = frames_in.min(available_frames);
        if to_write == 0 { return 0; }

        let start = (write & self.capacity_mask) * ch;
        let samples = to_write * ch;

        // Two-phase copy to handle wrap.
        let first_run = (cap * ch - start).min(samples);
        let second_run = samples - first_run;

        unsafe {
            core::ptr::copy_nonoverlapping(
                block.as_ptr(),
                self.data.add(start as usize),
                first_run as usize,
            );
            if second_run > 0 {
                core::ptr::copy_nonoverlapping(
                    block.as_ptr().add(first_run as usize),
                    self.data,
                    second_run as usize,
                );
            }
        }

        // Publish.
        header.write_idx.store(write + to_write, Ordering::Release);
        to_write as usize
    }
}

// ---------- Consumer (other instance's audio thread OR GUI thread) ----------

pub struct ShmRingConsumer {
    header: *mut RingHeader,
    data: *const f32,
    capacity_mask: u64,
    channels: u64,
}

unsafe impl Send for ShmRingConsumer {}

impl ShmRingConsumer {
    #[inline]
    pub fn pop(&mut self, out: &mut [f32]) -> usize {
        let header = unsafe { &*self.header };
        let cap = self.capacity_mask + 1;
        let ch = self.channels;
        let frames_out = (out.len() as u64) / ch;

        let read = header.read_idx.load(Ordering::Relaxed);
        let write = header.write_idx.load(Ordering::Acquire);
        let available_frames = write - read;

        let to_read = frames_out.min(available_frames);
        if to_read == 0 { return 0; }

        let start = (read & self.capacity_mask) * ch;
        let samples = to_read * ch;

        let first_run = (cap * ch - start).min(samples);
        let second_run = samples - first_run;

        unsafe {
            core::ptr::copy_nonoverlapping(
                self.data.add(start as usize),
                out.as_mut_ptr(),
                first_run as usize,
            );
            if second_run > 0 {
                core::ptr::copy_nonoverlapping(
                    self.data,
                    out.as_mut_ptr().add(first_run as usize),
                    second_run as usize,
                );
            }
        }

        header.read_idx.store(read + to_read, Ordering::Release);
        to_read as usize
    }
}

// ---------- Setup (not RT; called from GUI/plugin init) ----------

use shared_memory::{Shmem, ShmemConf};

pub fn create_ring(
    name: &str,
    capacity_frames: u64,
    channels: u32,
    sample_rate: u32,
) -> std::io::Result<(Shmem, ShmRingProducer, ShmRingConsumer)> {
    assert!(capacity_frames.is_power_of_two());
    let total_bytes =
        size_of::<RingHeader>()
        + (capacity_frames as usize) * (channels as usize) * size_of::<f32>();

    let shmem = ShmemConf::new()
        .os_id(name)
        .size(total_bytes)
        .create()
        .expect("shm create");

    let base = shmem.as_ptr() as *mut u8;
    let header_ptr = base as *mut RingHeader;
    unsafe {
        core::ptr::write(
            header_ptr,
            RingHeader {
                write_idx: AtomicU64::new(0),
                _pad1: [0; 64 - size_of::<AtomicU64>()],
                read_idx: AtomicU64::new(0),
                _pad2: [0; 64 - size_of::<AtomicU64>()],
                capacity: capacity_frames,
                channels,
                sample_rate,
                flags: 0,
                _pad3: [0; 36],
            },
        );
    }
    let data_ptr = unsafe { base.add(size_of::<RingHeader>()) } as *mut f32;

    let prod = ShmRingProducer {
        header: header_ptr,
        data: data_ptr,
        capacity_mask: capacity_frames - 1,
        channels: channels as u64,
    };
    let cons = ShmRingConsumer {
        header: header_ptr,
        data: data_ptr,
        capacity_mask: capacity_frames - 1,
        channels: channels as u64,
    };
    Ok((shmem, prod, cons))
}
```

Notes:

- The header is explicitly cache-line-aligned with separate padding for
  `write_idx` and `read_idx` to avoid false sharing.
- `push` and `pop` are `#[inline]`, branch-lean, use only
  `Relaxed`/`Acquire`/`Release` — no fences, no SeqCst.
- Nothing on the RT path allocates or calls into the OS.
- Setup uses `shared_memory` (or equivalent `memmap2` wrapper) — this is
  fine because it runs from plugin init, not the audio thread.
- For audio data, a power-of-two capacity is crucial; we use `capacity_mask`
  for wrap rather than modulus.

---

## 12. Recommended architecture

### 12.1 The two channels

```text
+---------- Instance A ----------+       +---------- Instance B ----------+
|                                |       |                                |
|  Audio thread                  |       |  Audio thread                  |
|    writes samples -----------> shm ring -------> reads samples          |
|                                |       |                                |
|  GUI thread                    |       |  GUI thread                    |
|    push metadata ------> local socket -------> pull metadata            |
|                                |       |                                |
+--------------------------------+       +--------------------------------+
                      \                           /
                       \                         /
                        \----> registry shm <---/
                              (slots, tags, discovery)
```

### 12.2 Components

1. **Registry segment** — `/plugin-rack-<host-pid>-registry` (macOS App Group
   prefix if sandboxed). Fixed-size slot table (32 slots). Each slot holds
   `pid`, `instance_uuid`, `link_tag`, `ring_name`, `alive` heartbeat.

2. **Per-instance audio ring** — `/plugin-rack-<host-pid>-ring-<uuid>`.
   Per-pair SPSC if needed. Created by the producer at activation time.
   Consumer-opened on demand when a peer is found.

3. **Per-instance control socket** — `interprocess` local socket,
   `plugin-rack-<host-pid>-ctrl-<uuid>`. Listener in GUI thread. Messages
   are length-prefixed bincode enums.

4. **Discovery thread** — per-instance, low-priority, 500 ms heartbeat.
   - Stamps own slot's `alive`.
   - Scans table for slots whose `link_tag` matches ours.
   - Opens rings/sockets for new peers; closes ones that have gone dark.
   - Fires application-level `PeerJoined` / `PeerLeft` events to our GUI.

5. **Link-tag lifecycle.**
   - On first activation, generate a random `link_tag` and persist in
     plugin state.
   - UI offers: "Link with..." drop-down sourced from the registry, plus
     "New link group" and "Join group X."
   - Selecting a peer's group writes the peer's `link_tag` into our state
     and announces.

### 12.3 What we do NOT do

- No locks in the audio thread.
- No syscalls in the audio thread — even `shm_unlink` and socket IO run
  from the GUI thread only.
- No polling of the DAW project path.
- No TCP, no UDP, no OSC on the default path.
- No cross-platform shared mutex — we rely on atomics + heartbeat.

### 12.4 Phasing

**Phase 1 — discovery + GUI meters only.**
Registry segment, local sockets, 30 Hz meter push. This alone delivers the
"both instances look like one console" story. No audio IPC yet. Low risk.

**Phase 2 — audio ring for analysis.**
Add the SPSC ring. Use it to publish spectrum / loudness / peak buffers
between instances. Still no actual audio flow, just derived data.

**Phase 3 — real audio ring.**
Ship parallel compression across tracks with an *explicit* one-block
latency. Document the constraint; this is not a bug.

**Phase 4 — optional OSC facade.**
If users ask to drive our console from TouchOSC / OSC/Pilot, expose our
control-channel messages over OSC with the same schema. Use `rosc`.

### 12.5 Crate shortlist

- `shared_memory` — audio ring + registry segment.
- `interprocess` — GUI control socket (sync; no Tokio needed for 30 Hz).
- `bincode` + `serde` — wire format for control messages.
- `uuid` v4 — instance IDs.
- `atomic_float` or raw `AtomicU32`/`AtomicU64` — for meter slot values.
- `rtrb` — intra-process ring buffer pattern reference and use it inside
  plugin (audio thread ↔ GUI thread) even when cross-process is via ShmRing.
- `rosc` — only if/when Phase 4.

### 12.6 Testing checklist

- Two instances in same DAW, save, close, reopen — link re-establishes.
- One instance in DAW A, another in DAW B on same machine — they do NOT
  link (per-host-PID registry isolates).
- Kill one instance via DAW crash — other instance sees heartbeat timeout
  and drops the peer cleanly.
- Switch sample rates mid-session — ring header flag is re-negotiated.
- macOS sandboxed AU host — shm names carry App Group prefix.
- Windows: names in `Local\` namespace work across DAW instances.
- RT check: `-Zsanitizer=thread` and `no_std` style audio-thread lint —
  reject anything that calls `alloc` or `syscall!` on the hot path.

---

## 13. Open questions

1. Do we want symmetric links (A sends to B, B sends to A on the same bus)
   or only producer-consumer? Symmetric = two rings. Cost is negligible.

2. For sample-rate mismatches across DAW projects — does our Phase-1
   registry per-host-PID strategy fully isolate, or can two separate Bitwig
   windows share a PID? (Bitwig runs one process; two projects in two
   windows is one PID — so yes, they would collide. Solution: per-window
   tag inside the registry.)

3. Where is the best nih_plug hook to spawn the discovery thread? In
   `initialize()`? On first `editor()` open? Probably `initialize()`, with
   graceful join on `deactivate()`.

4. Do we persist `link_tag` only, or also the last-known peer `instance_uuid`
   list? The latter lets us preserve ordering of instances in the UI across
   reloads.

---

## 14. References

- FabFilter Pro-Q 4 multi-instance list:
  https://www.fabfilter.com/news/1768467600/fabfilter-introduces-multi-plugin-instance-list-in-pro-q-410
- FabFilter Pro-Q 3 feature review:
  https://www.pluginboutique.com/articles/1420-FabFilter-Pro-Q-3-Review-of-Key-New-Features
- FabFilter user forum on multi-instance sidechain:
  https://www.fabfilter.com/forum/topic/5901/use-pro-q-3-instances-as-multiple-side-chain-sources
- iZotope IPC explainer:
  https://www.izotope.com/en/learn/inter-plugin-communication-explained.html
- iZotope Relay 1.0 IPC docs:
  https://s3.amazonaws.com/izotopedownloads/docs/relay101/en/ipc/index.html
- iZotope Relay product page:
  https://www.izotope.com/en/products/insight/features/relay.html
- Sonible smart:limit:
  https://www.sonible.com/smartlimit/
- discoLink — open-source C++17 cross-plugin shared-memory IPC:
  https://github.com/reales/discolink
- discoDSP discoLink product page:
  https://www.discodsp.com/discolink/
- KVR thread on VST3 inter-instance info sharing:
  https://www.kvraudio.com/forum/viewtopic.php?t=595782
- JUCE forum on cross-plugin communication:
  https://forum.juce.com/t/communication-between-different-plugins-within-the-same-host/4030
- JUCE InterProcessConnection class:
  https://docs.juce.com/master/classInterprocessConnection.html
- mgeier/rtrb — realtime-safe SPSC ring buffer:
  https://github.com/mgeier/rtrb
- agerasev/ringbuf:
  https://github.com/agerasev/ringbuf
- kotauskas/interprocess:
  https://github.com/kotauskas/interprocess
- RazrFalcon/memmap2-rs:
  https://github.com/RazrFalcon/memmap2-rs
- diwic/shmem-ipc — Linux-only memfd + eventfd IPC:
  https://github.com/diwic/shmem-ipc
- diwic/fdringbuf-rs:
  https://github.com/diwic/fdringbuf-rs
- klingtnet/rosc — OSC in Rust:
  https://github.com/klingtnet/rosc
- Apple Dev Forums on shm in sandbox:
  https://developer.apple.com/forums/thread/719897
- robbert-vdh/nih-plug:
  https://github.com/robbert-vdh/nih-plug
- Rust Audio forum — rtrb announcement:
  https://rust-audio.discourse.group/t/announcement-real-time-ring-buffer-rtrb/346
- Rafa Calderon — Zero-Copy IPC with Rust:
  https://dev.to/rafacalderon/beyond-ffi-zero-copy-ipc-with-rust-and-lock-free-ring-buffers-3kcp
