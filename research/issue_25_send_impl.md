# Issue #25 — remove unsafe `Send` impl on `GuestEditorView`

## Goal

Make `GuestEditorView` `!Send` so the Rust type system prevents it from
crossing a thread boundary. The struct wraps a raw `NSView` pointer and its
`Drop` calls `removeFromSuperview` + `IPlugView::removed()`, both of which
must execute on the main thread. A background-thread drop is UB.

## What we changed

**File:** `crates/rack-gui/src/guest_view.rs`

1. **Deleted** the `unsafe impl Send for GuestEditorView {}` block in the
   `macos` module.
2. **Expanded** the doc comments on `GuestEditorView` to explain the
   thread-safety contract: construct lazily inside the editor's
   spawn / update callback (main thread); never store in a `Send` parent.
3. **Updated** the `Drop` impl's SAFETY comment to reflect the new
   invariant — because the struct is `!Send`, `drop` always runs on the
   thread that built it, which by `attach()`'s contract is the main thread.
4. **Added** a compile-time regression trip-wire
   (`_assert_guest_editor_view_is_not_send`) using the standard
   trait-coherence ambiguity trick: if `GuestEditorView` ever becomes
   `Send` again, `cargo check` fails with E0283 "multiple `impl`s
   satisfying ... found".
5. **Made the non-macOS stub `!Send` too** via a
   `PhantomData<*mut ()>` marker, so consumers get the same threading
   contract on every platform — accidental cross-thread moves fail to
   compile on Linux / Windows just like on macOS.

**File:** `crates/rack-gui/Cargo.toml`

- Bumped `rack-gui` version: `0.3.0` → `0.4.0`.

## Why no lazy-construction wrapper was needed

Issue brief (step 3) suggests that if the Editor trait requires `Send` on
the return type, we should introduce a `Send` parent that lazily constructs
`GuestEditorView` inside `Editor::spawn`.

I searched the workspace and found `GuestEditorView` has **zero consumers
outside its own source file**: it is infrastructure for future issues. The
GUI's `GuiState` (passed to `nih_plug_egui::create_egui_editor`, which has
`T: Send`) does NOT hold a `GuestEditorView`. The `Editor` trait itself
(`pub trait Editor: Send`) does require `Send` on the editor handle
returned from `Plugin::editor()`, but that handle is a
`Box<dyn Editor>` built by `create_egui_editor`, not a `GuestEditorView`.

Therefore simply removing the unsafe impl is sufficient today, and future
code that wants to embed `GuestEditorView` will need to construct it
lazily inside an egui callback (main thread, no `Send` required) — which
is the correct architecture anyway.

## Why `GuestEditorView` is `!Send` automatically

- `ns_view: NonNull<NSView>` — `NonNull<T>` is `!Send + !Sync` by default
  (raw pointers opt out of the auto-traits). This is the field that makes
  the struct `!Send`.
- `plug_view: ComPtr<IPlugView>` — this IS `Send` (conditional on
  `IPlugView: Sync + Send`, which is declared in the `vst3` crate
  bindings at `src/bindings.rs:1778-1779`).
- `size: (u32, u32)` — `Send`.

Auto-trait derivation: `Send` is composed over all fields; one `!Send`
field (`NonNull<NSView>`) taints the whole struct.

## Verification

- `cargo fmt --all --check` clean.
- `cargo clippy --workspace --all-targets -- -D warnings` clean.
- `cargo test --workspace` — 19 tests passing (no new tests added; the
  compile-time assertion is `#[allow(dead_code)]` — it only needs to
  *compile*, never run).
- `cargo xtask bundle rack-plugin --release` — CLAP + VST3 bundles built
  on macOS 15 (Apple Silicon), including signing.
- **Regression sentry verified**: temporarily re-added
  `unsafe impl Send for GuestEditorView {}`, confirmed `cargo check`
  fails with E0283 on the assertion function, then reverted.

## Not in scope (per issue instructions)

- Restoring `forbid(unsafe_op_in_unsafe_fn)` — tracked separately as #26.
- Any changes to `rack-host-vst3`, `rack-host-clap`, `rack-ipc`, CI.

## Acceptance criteria

- [x] `GuestEditorView` is `!Send` (verified by compile-time assertion).
- [x] No `unsafe impl Send` anywhere in `rack-gui` (grep clean).
- [x] macOS build succeeds; `cargo xtask bundle rack-plugin --release`
  produces signed `.vst3` + `.clap` bundles.

Editor embedding not re-tested visually in a DAW — no behavioral change
since `GuestEditorView`'s code paths are unchanged; only the auto-trait
derivation differs.
