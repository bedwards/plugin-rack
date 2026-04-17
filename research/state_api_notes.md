# `GuestStateSource` API — allocation notes

**Status:** decided — keep `get_state(&mut self) -> anyhow::Result<Vec<u8>>`.
**Source:** issue #28 (Gemini review of PR #27).
**Date:** 2026-04-17.

## Context

`rack_core::GuestStateSource` is the uniform state round-trip trait the rack
uses to drive CLAP and VST3 guests through DAW save/load:

```rust
pub trait GuestStateSource {
    fn get_state(&mut self) -> anyhow::Result<Vec<u8>>;
    fn set_state(&mut self, bytes: &[u8]) -> anyhow::Result<()>;
}
```

Gemini flagged that `get_state` returning an owned `Vec<u8>` forces one
heap allocation per call, and asked whether a borrowed-slice return or a
caller-provided `&mut Vec<u8>` out-param would be feasible.

## (a) Per-call ownership constraints from each format

### CLAP (`clack_host::extensions::state::PluginState::save`)

Signature (effectively):

```
save(&self, plugin: &mut PluginMainThreadHandle, stream: &mut impl io::Write)
```

The host passes a writer; CLAP serialises into it. We use
`Cursor::new(Vec::new())` — i.e. the writer **owns** a `Vec<u8>` that grows
as the guest writes. When `save()` returns we pull the `Vec` out of the cursor
and return it.

Constraint: CLAP never borrows bytes back to us. It only *accepts* a sink.
We can choose what backs that sink.

### VST3 (`IComponent::getState(IBStream*)`)

The host passes an `IBStream` COM object. The guest calls our `write()` vtable
method as it serialises. Our `MemStream` is a `#[repr(C)]` struct whose
*first field* is the `IBStream` ABI header, so casting `*mut MemStream` to
`*mut IBStream` is valid; it stores its bytes in an internal `Vec<u8>`.

The COM lifetime rules force `MemStream` to be `Box`-allocated (the guest
holds an AddRef'd pointer during the call, and the vtable is read via
`(*this).vtbl` indirection), so we already pay one `Box::new` per call in
addition to the state `Vec`. Switching the *return type* does not touch
that path.

Constraint: VST3 never borrows bytes back to us. Steinberg chose a
streaming-write model precisely to let plugins produce state without
predeclaring size.

### Common thread

Both formats are **pull-by-the-guest, push-into-our-sink**. Neither format
exposes a pointer into the guest's own state memory. The allocation is not
an API-shape artefact of `GuestStateSource`; it is an artefact of the
underlying plugin ABIs. `Vec<u8>` is the cheapest sink that satisfies both.

## (b) Would `&mut Vec<u8>` out-param actually reduce allocations?

Proposed alternative:

```rust
fn get_state(&mut self, out: &mut Vec<u8>) -> anyhow::Result<()>;
```

This saves an allocation **only if** the caller keeps a persistent `Vec<u8>`
and reuses it across successive calls. The allocations per call look like:

| path | current | with `&mut Vec<u8>` out-param |
|---|---|---|
| CLAP `save`: sink `Vec<u8>` | 1 (grows) | 1 (grows, reused if `clear()`'d) |
| VST3 `MemStream` `Box<MemStream>` | 1 | 1 (unchanged — COM needs heap) |
| VST3 `MemStream.data: Vec<u8>` | 1 | 1 (cannot reuse — stream owns it) |

In the VST3 path the `Vec<u8>` lives inside the `Box<MemStream>` that the
guest writes into via the COM vtable. To "reuse" the caller's buffer we
would have to make `MemStream` hold a `&mut Vec<u8>` instead of an owned
`Vec<u8>`, which would require threading a lifetime parameter through the
COM `#[repr(C)]` struct and through the `Send`/`Sync` impls. That is a
cost we would pay at every VST3 call site for a single-digit-nanosecond
saving. Not worth it.

In the CLAP path reuse is possible but irrelevant in practice — see
call-frequency below.

### Call frequency

Guest state is read under exactly three triggers:

1. DAW save / save-as.
2. DAW preset export from the rack.
3. Preset-switching inside the rack's own strip list.

All three are user-driven, main-thread, main-loop — not the audio callback.
Expected rate: **well under 1 Hz**, typically one call per manual save.
Saving one allocation per save on a path that allocates at <1 Hz is
rounding error next to DAW save-file I/O itself (milliseconds of disk,
megabytes of file).

Moreover, the rack does not batch state reads across strips in a tight
loop with a shared scratch buffer. Each call site is one-shot per strip
per save, and the `Vec<u8>` returned flows directly into
`StripState.component_state` (an owned `Vec<u8>` field on
`rack_core::StripState`). Handing it off by value is exactly what we want;
a `&mut Vec<u8>` out-param would just force the call site to do
`take()`/`replace()` dance before handing the bytes to `StripState`.

### Borrowed-slice return (`-> &[u8]`)

Would require `Vst3Guest`/`ClapGuest` to retain a cached buffer across
calls. Semantically worse: the guest may mutate its parameters between
state reads (a common pattern during a DAW save when the host scans all
parameter values before asking for state), and a stale `&[u8]` would
silently return the previous snapshot. Rejected.

## (c) Verdict

**Keep `get_state(&mut self) -> anyhow::Result<Vec<u8>>` as is.**

Rationale:

- The allocation is forced by the underlying plugin ABIs, not by our trait
  shape. An out-param does not eliminate it on the VST3 path.
- The path runs at DAW-save rate (<1 Hz), not audio rate. There is no
  measurable saving to chase.
- `Vec<u8>` by value composes naturally with `StripState.component_state:
  Vec<u8>`, the serde-persisted destination. An out-param would add a
  `take()` at every call site for zero benefit.
- A borrowed return is semantically wrong because guest state mutates
  between calls.

Closed as wontfix-by-design.

## References

- `crates/rack-core/src/lib.rs` — trait definition.
- `crates/rack-host-clap/src/lib.rs` — `ClapGuest::get_state`, CLAP sink is a
  growing `Vec<u8>` inside `Cursor`.
- `crates/rack-host-vst3/src/lib.rs` — `Vst3Guest::get_state`, `MemStream`
  is `Box`-allocated and owns its `Vec<u8>`.
- `research/vst3_spec.md` §9 "Two states, two streams" — Steinberg's
  processor/controller split that requires per-call owned buffers.
- `research/issue_11_state_persistence.md` — original design doc for the
  `StripState` schema.
