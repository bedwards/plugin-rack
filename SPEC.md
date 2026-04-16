# SPEC.md — plugin-rack

**Status:** v0.1 decisions locked 2026-04-16 from `research/*.md` (8 deep dives). Edit via ADR-style amendments, not full rewrites.

## Product

Production-grade plugin rack / mixing-console plugin. Hosts nested plugins, exposes their params for DAW modulation, offers three fluid layouts, scales per-strip. Bitwig-first, works in any VST3/CLAP host. Amazing audio, low CPU, sample-accurate.

## Two-track answer

**Hard constraint confirmed:** no VST3 host (Bitwig included) lets a single instance own two tracks' signal chains. Bitwig hard-caps VST3 input at 4 channels (stereo main + stereo sidechain). Multi-out works (one-to-many) but multi-in does not. Source: `research/bitwig.md`, `research/vst3_spec.md`.

**Solution:** each `plugin-rack` instance = one track's channel strip. Instances link via shared memory (keyed by user-facing `link_tag` string persisted in plugin state), publish state at 30 Hz, render a unified console view that shows all linked sibling strips. Audio path stays per-track (host rule, can't be broken). Bitwig modulators still map cleanly — each strip's params live on the track where the host expects them.

## Locked technical decisions

| Area | Decision | Source |
|------|----------|--------|
| Framework | `BillyDM/nih-plug` (community fork, 2026-03-29) | `research/nih_plug.md` |
| Plugin formats | CLAP native; VST3 + AU via `free-audio/clap-wrapper` | `research/nih_plug.md`, `research/hosting.md` |
| CLAP hosting (guests) | `prokopyl/clack` (`clack-host`) | `research/hosting.md` |
| VST3 hosting (guests) | `coupler-rs/vst3-rs` + custom loader | `research/hosting.md` |
| GUI primary | `vizia` (CSS theming, user scale factor, Skia text) | `research/gui.md` |
| GUI wrap layout | `iced` 0.14 if vizia flex-wrap proves insufficient | `research/gui.md` |
| GPU backend | GL / Skia (NOT wgpu — segfaults in hosts) | `research/gui.md` |
| Accessibility | AccessKit blocked on `baseview#200` — track, don't ship as gated | `research/gui.md` |
| Parameters | Fixed slot pool, 128 macro params | `research/hosting.md`, `research/prior_art.md` |
| Modulation | Host automation only on VST3; CLAP `PARAM_MOD` on CLAP path | `research/vst3_spec.md` |
| IPC | shared-memory SPSC ring (`rtrb` + `memmap2`) + PID registry + heartbeat | `research/ipc.md` |
| IPC latency | one-block (sub-block infeasible) | `research/ipc.md` |
| Build | `cargo xtask bundle-universal` per format | `research/ci_verification.md` |
| Validation | `pluginval --strictness-level 10` + Steinberg `validator` | `research/ci_verification.md` |
| CI matrix | macos-15 arm64 / windows-2025 / ubuntu-24.04 | `research/ci_verification.md` |
| Offline smoke | DawDreamer via `uv run` script | `research/ci_verification.md` |
| Licensing foundation | VST3 SDK 3.8.0 MIT (Oct 2024 Steinberg relicense) | `research/nih_plug.md` |
| Real-time safety | `assert_no_alloc` on process path | `research/ci_verification.md` |

## Workspace layout

```
crates/
  rack-core/        # DSP, nested plugin scheduling, state, macro bus
  rack-host-clap/   # clack-host integration
  rack-host-vst3/   # vst3-rs integration + clap-wrapper bridge
  rack-gui/         # vizia GUI (with iced fallback crate if needed)
  rack-ipc/         # shared-memory link, group registry, console-view sync
  rack-plugin/      # nih_plug plugin entry wrapping all the above
xtask/              # nih_plug_xtask + custom bundle-universal tasks
pluginrack/         # Python CLI (uv-managed) — orchestration entry point
```

## v1 feature set (maps to GH issues)

1. **Rust workspace scaffold** — empty CLAP plugin that Bitwig detects.
2. **VST3 + AU exports** via `clap-wrapper` in xtask.
3. **CLAP guest hosting** — load a `.clap`, call `process()`, passthrough audio.
4. **VST3 guest hosting** — load a `.vst3`, call `process()`, passthrough audio.
5. **Nested plugin editor embed** — open guest's native GUI inside rack strip.
6. **Fixed macro param pool** — 128 params, mappable to guest params.
7. **Layout engine** — row / col / wrap, live toggle, persisted.
8. **Per-strip scale** — independent scale per nested GUI (0.5× – 2×).
9. **State persist** — save rack + all guest state blobs through `IComponent::getState`.
10. **Inter-instance IPC** — shared-memory sibling discovery by `link_tag`.
11. **Console-view render** — one instance's GUI shows all linked strips.
12. **Bitwig buffer / mod verification** — offline scripts that prove the rack handles variable block sizes and that exposed params are modulatable.

## Non-goals (v1)

- Out-of-process sandbox. Accept crash = lose DAW track. Address in v2.
- Parallel / multiband chains (research doc calls this StudioRack escape hatch — worth but not v1).
- Polyphonic modulation via CLAP — deferred; most of our use is audio FX.
- Audio-rate cross-instance send/link (sub-block impossible).
- Bundled preset library.

## Performance budget

- Empty rack process() ≤ 0.3% CPU @ 512 samples, 48 kHz, Apple Silicon M-class.
- GUI redraw at 60 Hz when interacting, 30 Hz idle; ≤ 3% single-core when visible.
- IPC sibling sync ≤ 30 Hz; publishes strip param snapshot (few KB).
- `criterion` bench in CI; regression gate: +10% over baseline blocks merge.
