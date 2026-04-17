# Issue #12 — IPC sibling registry (foundation PR)

**Scope delivered:** shared-memory slot table + heartbeat + siblings
discovery. Enough for two rack instances to find each other by
`link_tag`. Ring buffer and state publishing are deferred to the
follow-up (issue #13 / console-view).

## Honest scope label

Contributes to #12 — the four acceptance criteria are covered for the
*registry* layer, but the issue body also mentions "publishes strip
state at 30 Hz" which lives on top of an SPSC ring that is explicitly
out of scope here. Issue stays open.

## Architecture landed

One POSIX shared-memory segment:

- **macOS / Linux:** `/plugin-rack.reg.v1` (19 bytes; macOS App Sandbox
  PSHMNAMLEN is 31). Opened via `libc::shm_open(O_RDWR|O_CREAT, 0o600)`
  + `ftruncate` + `memmap2::MmapOptions::map_mut`.
- **Windows:** stub that bails with a clear "not implemented" message.
  The API surface compiles so `cargo build` on the CI Windows runner
  succeeds; tests that exercise the segment are `#[cfg(unix)]`-gated.
  Follow-up: use `CreateFileMappingW` in `Local\plugin-rack.reg.v1`.

Layout inside the segment:

```
+---- Header (64 bytes) ------------+
| magic: "PLRACKR1"                 |
| version: u32 (= 1)                |
| slot_count: u32 (= 64)            |
| _pad: [u8; 48]                    |
+---- Slot 0 (96 bytes) ------------+
| alive: u32 (atomic CAS target)    |
| pid: u32                          |
| last_heartbeat_nanos: u64         |
| instance_uuid: [u8; 16]           |
| link_tag: [u8; 32] (NUL-padded)   |
| _pad: [u8; 32]                    |
+----------------------------------+
| ... 63 more slots                |
```

Total segment: 64 + 64 * 96 = 6208 bytes. Pinned in shm per-host.

## Key design decisions

### 1. `AtomicU32::from_ptr` for the `alive` field

Each slot is plain `repr(C)`, no `AtomicU32` in the struct definition.
To CAS the `alive` field, we take its address and wrap it via
`AtomicU32::from_ptr` (stable since 1.75). Benefit: `Slot` stays
literally POD, which makes shared-memory layout trivially stable across
independently-compiled binaries and lets us use `read_volatile` /
`write_volatile` for all the non-atomic reads and writes without
worrying about the compiler assuming it's an aligned `Atomic`.

### 2. Timestamp source: `clock_gettime(CLOCK_MONOTONIC)`

Alloc-free by construction — the kernel writes into a stack `timespec`.
Monotonic (never jumps), comparable across processes on the same host,
no `Instant::now()` overhead. `std::time::Instant` would add an
unnecessary wrapper and cannot be transmitted cross-process anyway.

### 3. No allocation on `heartbeat`

The contract is documented in the `heartbeat()` docstring. Exact work:

1. One `clock_gettime` syscall.
2. One `AtomicU32::load(Relaxed)` defensive check.
3. One `write_volatile` of a `u64`.

No `Vec`, no `Box`, no `format!`, no file IO. A Miri / `assert_no_alloc`
test is not plumbed for this crate yet — rack-ipc is not on the audio
path in this PR, so we document the invariant and cover it with the
same tooling once #13 wires it into the audio-thread-adjacent
discovery-timer path.

### 4. `SlotHandle` as an RAII release token

Drop on handle → `AtomicU32::store(0, Release)`. That's the entire
release step. Siblings scan reads `alive.load(Acquire)` and skips 0.
No separate `release()` method needed; losing the handle (e.g., the
plugin gets dropped by the host) implicitly frees the slot. Cloneable
`Arc<MmapMut>` inside `SlotHandle` keeps the mapping alive until all
handles go, avoiding use-after-unmap on drop.

### 5. Process-death detection via heartbeat TTL

Per research/ipc.md §4.5: PIDs are reused, so we cannot trust "is this
PID still alive?" alone. Instead, `siblings()` takes a `(now, ttl)`
pair and filters out any slot whose `last_heartbeat_nanos < now - ttl`.
Suggested TTL in production: 4s (issue #12 acceptance "within 4
seconds"), with a 500 ms heartbeat cadence from the discovery thread
that lands with #13.

### 6. Fresh UUID / tag generation without a `uuid` dep

Rolled a SplitMix64-based 128-bit generator seeded from PID + monotonic
ns + stack address. We don't need cryptographic randomness — the UUID
is only used to disambiguate slots within a single host's registry,
and collisions are caught by the CAS on `alive`. Zero extra deps.

`rack_ipc::fresh_link_tag()` returns a 24-char hex string (96 bits of
entropy), used by `rack-plugin` to initialise its `link_tag` to
something that does NOT auto-link with siblings. The user opts in to
a group by pasting a peer's tag into the GUI (UI lands with #13).

## Files touched

- `crates/rack-ipc/Cargo.toml`: 0.1.0 → 0.2.0, added `memmap2`,
  `bytemuck`, `libc` (unix-only) deps.
- `crates/rack-ipc/src/lib.rs`: full implementation (820 lines).
- `crates/rack-plugin/Cargo.toml`: 0.4.0 → 0.5.0.
- `crates/rack-plugin/src/lib.rs`: added `link_tag: Arc<Mutex<String>>`
  persisted field with `#[persist = "link_tag"]`. Defaulted to
  `rack_ipc::fresh_link_tag()`.

## Tests

Six unit tests cover the user-visible invariants:

- `claim_two_slots_same_tag_discovers_both` — the acceptance-criterion
  happy path.
- `dropped_slot_is_removed_from_siblings` — RAII release.
- `stale_heartbeat_hides_slot_via_ttl` — the "drop within N seconds"
  acceptance criterion.
- `heartbeat_updates_timestamp_monotonically` — heartbeat is a no-op
  only when the slot is freed.
- `siblings_ignores_other_tag` — `link_tag` is the only group key.
- `claim_too_long_tag_errors` — bounds check.

Plus four pure-logic tests that don't touch shm:
- `header_size_stable`, `tag_equality`, `trimmed_tag_roundtrip`,
  `link_tag_unlinked_default`.

One test in `rack-plugin` (`link_tag_default_is_fresh_and_nonempty`)
verifies that two freshly-instantiated `PluginRackParams` do not share
a tag — critical to avoid surprise auto-linking.

Tests are serialized within `rack-ipc` via a `Mutex` because they all
share the one OS-wide registry segment. `#[cfg(unix)]` gates skip the
segment-touching tests on Windows.

## Known follow-ups (not this PR)

- SPSC ring over a second shm segment keyed by the winner's UUID —
  issue #13.
- Discovery thread in `Plugin::initialize()` that calls `heartbeat` on
  a 500 ms timer and emits `PeerJoined` / `PeerLeft` events to the
  GUI — also #13.
- `Plugin::deactivate()` graceful join that drops the `SlotHandle`
  before the host unloads us — #13.
- Windows `CreateFileMappingW` implementation + `QueryPerformanceCounter`
  timestamp — separate platform-completion issue.
- GUI field to edit `link_tag` (and a "Link with…" picker) — #13.
- Audio-rate cross-instance data stays permanently out of scope per
  SPEC.md §Two-track answer; documented in the lib-level doc comment.
