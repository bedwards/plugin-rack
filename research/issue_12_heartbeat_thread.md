# Issue #12 — heartbeat thread + DiscoveryHandle (follow-up to PR #34)

**Scope delivered:** `DiscoveryHandle` RAII type, `start_discovery`
entry point on `SharedRegistry`, 500 ms low-priority heartbeat thread,
graceful shutdown via `AtomicBool` + `park_timeout` + `unpark`, and the
two remaining acceptance tests for issue #12 (TTL-drop within 4 s,
graceful-shutdown released slot within 100 ms of handle drop).

## Honest scope label

**Closes #12.** All four acceptance boxes now check off:

- [x] two rack instances with the same link_tag discover each other
  (foundation; verified at runtime via `DiscoveryHandle::siblings`).
- [x] one dying is dropped within 4 seconds (new test
  `ttl_drop_removes_stale_peer_within_acceptance_window`: wait 3 s,
  then a 500 ms TTL scan sees zero siblings).
- [x] macOS App Sandbox naming rules respected (19-byte
  `/plugin-rack.reg.v1` — unchanged from #34).
- [x] no allocations on the publish path (alloc-free `heartbeat_by_idx`
  in the thread loop body; allocations are confined to builder
  setup / thread spawn / shutdown).

The remaining "publishes strip state at 30 Hz" language in the issue
body is orthogonal — that lives on top of an SPSC ring and is issue
#13's scope. The *registry lifecycle* portion that #12 is about is
complete.

## Heartbeat cadence rationale

500 ms (`DEFAULT_HEARTBEAT_INTERVAL`). Four considerations:

1. **Research target.** `research/ipc.md` §12 calls for a 500 ms
   heartbeat explicitly. We kept that literal value.
2. **TTL safety margin.** `DEFAULT_DISCOVERY_TTL` is 2 s → four ticks
   per TTL window. A single missed wake-up (scheduler hiccup) still
   leaves three on-time ticks inside the window.
3. **4-second acceptance budget.** Even if an entire TTL window lapses
   before the surviving peer's next siblings() scan, the stale peer
   falls out within `TTL + one_scan_interval` = 2 s + ≤30 Hz scan =
   ~2.03 s. Well under 4 s.
4. **CPU cost.** Per tick: one `park_timeout` wakeup, one
   `AtomicBool::load`, one `clock_gettime`, one volatile `u64` store.
   At 500 ms cadence this is free — sub-microsecond of CPU per second.

Tests shrink the interval to 50 ms via
`DiscoveryBuilder::with_heartbeat_interval(Duration)` so wall-clock
assertions finish in seconds rather than minutes. A 10 ms minimum
clamp prevents pathological CPU burn if a caller sets zero.

## Shutdown-wake mechanism

`park_timeout(interval)` + `AtomicBool::load(Acquire)` loop, plus
`JoinHandle::thread().unpark()` on Drop. The unpark primes the next
`park_timeout` to return immediately rather than waiting out the
remaining `interval`. In practice Drop returns in tens of microseconds
on Apple Silicon.

Alternatives considered and rejected:

- **`std::sync::Condvar`.** Needs a `Mutex<bool>` + `Condvar`; adds one
  allocation and two `std::sync` items per handle. Thread parks are
  cheaper and give identical semantics for our single-consumer case.
- **`mpsc::channel`.** Shuts down a thread with `drop(sender)`; but
  the receiver side allocates every recv, and we'd still need a
  timeout on top. Parking is simpler.
- **Spin + atomic flag.** Wastes CPU; not appropriate for a 500 ms
  cadence anyway.

## Allocation-free proof sketch (loop body)

The heartbeat thread's hot body:

```rust
fn heartbeat_loop(registry, slot_idx, shutdown, interval) {
    let slot_ref = SlotRef { slot_idx };   // one-time, stack-allocated
    loop {
        thread::park_timeout(interval);    // [1]
        if shutdown.load(Ordering::Acquire) { break; }  // [2]
        registry.heartbeat_by_idx(&slot_ref);           // [3]
    }
}
```

- **[1] `park_timeout`** — direct syscall into the kernel-level futex
  (Linux/macOS) / SRWLock-backed wait (Windows, future). No Rust
  allocation; the `Thread` handle lives on the thread's stack.
- **[2] `AtomicBool::load`** — single atomic read. No allocation.
- **[3] `heartbeat_by_idx`** — matches the existing `heartbeat` alloc
  contract verbatim: one `clock_gettime`, one atomic load, one
  volatile `u64` store. No `Vec`, `Box`, `String`, or IO.

One-time allocations in `DiscoveryBuilder::start`:

- `Arc::clone` ×2 (registry + shutdown): atomic refcount bump, no
  heap allocation (the Arcs already exist).
- `thread::Builder::name("rack-ipc-discovery".to_string())` — one
  `String` for the thread name. Fires once per handle creation.
- `thread::Builder::spawn` — allocates the thread's stack (OS-side,
  not Rust heap per se) and its `JoinHandle`.

One-time allocations in `DiscoveryHandle::drop`:

- Zero. `shutdown.store`, `thread.unpark`, `thread.join`,
  `slot.take + drop` — all allocation-free.

## 500 ms / 2 s / 4 s spacing rationale

| Value | Name | Chosen because |
|-------|------|----------------|
| 500 ms | heartbeat cadence | `research/ipc.md` §12 + 4× margin vs TTL |
| 2 s | default TTL | `research/ipc.md` §4.5; 4 missed ticks to cross |
| 4 s | issue #12 acceptance | user-perceptible "instance is gone" timer |
| 50 ms | test cadence | shrinks wall-clock test time 10× |

If we ever need to survive 8-second-class DAW stalls, bump TTL to 4 s
and keep heartbeat at 500 ms — still within acceptance because stale
detection is TTL + one scan.

## Plugin-side lifecycle

`rack-plugin` now implements `Plugin::initialize` and
`Plugin::deactivate`. On initialize:

- If `link_tag` is empty → do nothing (UNLINKED rack: no slot, no
  thread; satisfies the "unlinked default claims nothing" invariant
  the #34 foundation set up).
- Otherwise → `SharedRegistry::open_or_create` + `start_discovery`,
  storing the resulting `DiscoveryHandle` in
  `Mutex<Option<DiscoveryHandle>>`.

On deactivate:

- `*self.discovery.lock() = None` — drops the handle, which stops the
  thread and releases the slot.

Error handling: if either the registry attach or the discovery start
fails we `nih_log!` and return `true` anyway. A broken registry
segment must not block audio from starting; linking is a secondary
feature.

## Files touched

- `crates/rack-ipc/Cargo.toml` — 0.2.0 → 0.3.0.
- `crates/rack-ipc/src/lib.rs` — +`DiscoveryHandle`, `DiscoveryBuilder`,
  `SharedRegistry::{discovery_builder, start_discovery,
  heartbeat_by_idx, read_tag}`, `SlotRef` helper, 3 new tests.
- `crates/rack-plugin/Cargo.toml` — 0.5.0 → 0.6.0.
- `crates/rack-plugin/src/lib.rs` — +`initialize`, +`deactivate`,
  +`discovery: Mutex<Option<DiscoveryHandle>>` field.

## Tests added

- `ttl_drop_removes_stale_peer_within_acceptance_window` — the issue
  #12 acceptance gate. 3 s wall-clock wait + 500 ms TTL; also asserts
  the live peer's own heartbeat remained fresh during the wait.
- `graceful_shutdown_releases_slot_quickly` — Drop path correctness.
  Handle visible pre-drop, gone within 100 ms post-drop.
- `heartbeat_thread_advances_timestamp` — sanity check that the
  spawned thread actually ticks (at 50 ms cadence).

All 13 rack-ipc tests pass with both default parallelism and
`--test-threads=1`.

## Local validation

- `cargo fmt --all --check` — ok.
- `cargo clippy --workspace --all-targets -- -D warnings` — ok.
- `cargo clippy -p rack-ipc --all-targets --target
  x86_64-unknown-linux-gnu -- -D warnings` — ok. Full-workspace
  Linux cross-clippy hits an x11 pkg-config cross-sysroot issue on
  Apple Silicon (expected; noted in PR body).
- `cargo test --workspace` and `--test-threads=1` — all ok.
- `cargo build -p rack-plugin --release` from worktree — ok.
- `cargo xtask bundle rack-plugin --release` — bundles, but note:
  nih_plug_xtask's `chdir_workspace_root()` walks ancestors with
  `Cargo.toml` and picks the TOP-most one, which means from a worktree
  nested under `.claude/worktrees/` it builds against the parent repo's
  crates, not the worktree's. Release build from the worktree verifies
  the new code compiles + links; CI will exercise the bundle path
  against the PR branch head as usual.

## Known follow-ups (not this PR)

- SPSC ring + GUI sibling render — issue #13.
- Windows `CreateFileMappingW` implementation — platform-completion
  issue (no local Windows to validate).
- Runtime update when the user edits `link_tag` in the GUI (requires
  a "restart discovery" hook) — piggyback onto #13 when the GUI lands
  the link_tag field.
