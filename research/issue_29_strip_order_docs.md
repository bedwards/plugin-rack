# Issue #29 — strip_order audio-path contract

## Rationale

`PluginRackParams::strip_order: Arc<Mutex<Vec<StripState>>>` is persisted
via `#[persist = "strips"]` and mutated only from the GUI + save/load
paths. There is no audio-path reader today — `process()` is pure
passthrough. The mutex is safe in practice but nothing in the type or
source enforces the invariant, so a future contributor could trivially
add a `.lock()` call inside `process()` and introduce realtime priority
inversion.

Issue #29 (Gemini review on PR #27) asked for either a documentation
fix or a runtime guard. This PR lands the documentation fix.

## What shipped

- Doc comment above `strip_order` calls out the invariant, names the
  future `rack-ipc` snapshot consumer that audio-path reads must use,
  and pins the `#[persist = "strips"]` attribute as load-bearing for
  DAW session compatibility.
- Bumped `rack-plugin` 0.5.0 → 0.6.0 (no heartbeat PR open for #12 yet,
  so no concurrent bump to sync against).

## NoAudioLock<T> newtype — NOT shipped

Considered a debug-only newtype that panics on `.lock()` from a thread
named `"audio"`. Deferred because:

- `nih_plug` does not label its audio thread; `std::thread::current().name()`
  returns `None` on the realtime callback in most hosts.
- Detecting the audio thread reliably requires host cooperation (e.g.
  marking the thread at `initialize()`) or process-level state that
  adds complexity disproportionate to the risk.
- Revisit once strip scheduling actually lands and an audio-thread
  reader is imminent. At that point the snapshot path (arc_swap /
  rtrb) replaces the mutex on the read side anyway, making the guard
  moot.

The doc comment mentions the deferred guard so the next contributor
sees the rationale.

## Gates

- `cargo fmt --all --check`: clean
- `cargo clippy --workspace --all-targets -- -D warnings`: clean
- `cargo test --workspace`: all passing
