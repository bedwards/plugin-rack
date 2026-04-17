# Issue #11 — State persistence for rack + guest states

Implementation journal. Short-form, decisions + surprises.

## Scope (per issue + CLAUDE.md scope line)

- Schema + round-trip fidelity only.
- No strip-loading orchestration (future issue).
- No UI for missing guests.

## Decisions

- **Types land in `rack-core`** (`StripState`, `GuestFormat`, `GuestStateSource`). Format-agnostic, no nih_plug dep — preserves the workspace layout rule that `rack-core` doesn't link audio framework code.
- **`GuestFormat` enum** `Clap | Vst3`. Drives the reload code path on open.
- **`StripState` fields** match the issue spec exactly:
  - `format`, `path`, `class_id`, `plugin_id`, `macro_map`, `component_state`, `controller_state`.
  - `class_id: Option<[u8; 16]>` — `None` for CLAP, Some (16-byte TUID) for VST3.
  - `plugin_id: Option<String>` — `Some(id)` for CLAP, `None` for VST3.
- **Derives**: `Serialize, Deserialize, Clone, Debug` per spec, plus `PartialEq + Eq` added so `assert_eq!` on `Vec<StripState>` works in tests. Cheap, no downside.
- **`strip_order: Arc<Mutex<Vec<StripState>>>`** on `PluginRackParams` with `#[persist = "strips"]`. Matches the existing `macro_names` shape exactly (same `Arc<Mutex<Vec<T>>>` pattern, same `parking_lot` Mutex). nih_plug's `PersistentField` blanket impls (persist.rs L148-L169) cover this case.
- **Default is empty `Vec`** — strip loading is not in scope for #11.
- **Trait shim `GuestStateSource`** for `ClapGuest` + `Vst3Guest` — lets the rack drive state uniformly across formats. Implementations are thin delegating shims (`Vst3Guest::get_state(self)` etc.); zero duplication.
- **VST3 controller-state** is documented as a separate follow-up — the rack stores it on `StripState.controller_state` already, but the trait only covers processor state for v1. Matches the two-state VST3 reality (research/vst3_spec.md §463).

## Tests

- `strip_state_roundtrip_bytes` — VST3 variant with non-trivial 0..=255 and full-range controller bytes, JSON round-trip, field-by-field byte-equality. CLAP variant sanity check.
- `mock_guest_state_cycle` — `MockGuest` struct implementing `GuestStateSource`; wrap in `StripState`; serialise a `RackPersistedState` analogue containing `macro_names + strips`; deserialise; feed restored bytes into a fresh `MockGuest`; assert inner blob matches. Mirrors the nih_plug save→load path.
- `strip_order_default_empty` in `rack-plugin` — default `PluginRackParams` has zero strips.

## Surprises

- **`serde_json` not in workspace** — issue said "already listed as workspace deps — reuse." It's present in `Cargo.lock` (transitive via nih_plug) but not declared. Added it to `[workspace.dependencies]`.
- **`objc2-app-kit` features list shifted** during this branch (NSResponder removed upstream). Unrelated to #11; left alone.
- **`PartialEq` not in the spec's derive list** for `StripState`. Added it (plus `Eq`) because the `mock_guest_state_cycle` test's `RackPersistedState` analogue derives `PartialEq` and needs the inner `Vec<StripState>` to compare. Field-by-field `assert_eq!` would also work; this is cleaner and matches standard Rust container ergonomics.

## Version bumps

- `rack-core` 0.2.0 -> 0.3.0
- `rack-host-clap` 0.2.0 -> 0.3.0
- `rack-host-vst3` 0.2.0 -> 0.3.0
- `rack-plugin` 0.3.0 -> 0.4.0

## Verification run

- `cargo fmt --all --check` — clean.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `cargo test --workspace` — 6/6 rack-core, 5/5 rack-plugin, all others green.
- `cargo xtask bundle rack-plugin --release` — CLAP + VST3 bundles produced.
