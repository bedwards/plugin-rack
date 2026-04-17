# Issue #14 — offline Bitwig-buffer / modulation verification harness

## What this is

A `pluginrack verify bitwig-mod` tier-5c subcommand that renders the built
`rack-plugin.vst3` offline via DawDreamer under conditions that mimic the two
hostile things Bitwig Studio actually does to a hosted VST3:

1. **Variable block sizes**, including non-power-of-two sizes — Bitwig splits a
   single audio callback into multiple `process()` calls at sample-accurate
   automation + modulation waypoints, so plug-ins must tolerate any block size
   from 1 up to `maxSamplesPerBlock` (see `research/bitwig.md` §3 + §4).
2. **Host-driven parameter automation** — every exposed `IEditController`
   parameter must be modulatable from a host automation curve
   (see `research/bitwig.md` §2).

## Block sizes chosen

| Block | Why |
|-------|-----|
| 64    | Small power-of-two. Bitwig routinely hands 32-/64-sample sub-blocks when automation waypoints land mid-callback. Catches DSP paths that assume a larger minimum. |
| 511   | Prime-ish, odd, explicitly not a power of two. This is the whole point of the harness. It catches off-by-one assumptions, SIMD lane-tail bugs that only appear when `N % VECLEN != 0`, and buffer-alignment bugs that pass at 512 but break at 511. |
| 1024  | A common DAW-level audio-callback size. Serves as the `maxSamplesPerBlock` ceiling the harness reports to the plugin. |

We deliberately do not test 32 — it's the power-of-two directly below 64 and
adds negligible signal. 511 is the high-value entry in this list.

Rationale for running the *reference* render at the same block size as the
*ramp* render inside each loop iteration: DawDreamer's internal block-boundary
alignment can introduce a small (1–2 sample) length drift; keeping both renders
at the same block size lets `_rms_delta` trim-to-min-length without bias.

## Macro indices sampled

The rack exposes 128 macro params via nih_plug's `#[nested(array, group = "Macros")]`
derive, producing stable IDs `value_1` .. `value_128`.

| Index | Why |
|-------|-----|
| 0     | First slot. Catches fencepost errors in nested-array index math and ensures the first param is even visible to DAWs. |
| 63    | Middle. Catches bugs that only surface away from array edges — e.g. if binding logic special-cases the first and last entries. |
| 127   | Last slot. Catches end-of-array fencepost errors, which are the single most common nested-array bug in practice. |

Sampling three out of 128 is a deliberate coverage/time tradeoff — a full
128-way sweep would 40× the render cost and wouldn't catch anything the
3-sample doesn't, given the uniform-array derive pattern. If a future issue
switches to non-uniform macro wiring (e.g. per-slot smoothing policy), expand
the sample set here.

## What "differ from silence" means numerically

We render two 2-second passes per (block, macro) trio:

- **Reference**: all macros pinned at 0.0. We treat this as "silence + whatever
  the rack happens to emit with no modulation". For v0.3 (audio passthrough),
  this is DawDreamer's synthesized silence after the passthrough.
- **Ramp**: the sampled macro is animated 0.0 → 1.0 linearly across the render;
  all other macros are 0.0.

We compute the root-mean-square of the sample-wise difference:

```
rms_delta = sqrt(mean((ref - ramp)**2))
```

Threshold: `1e-4`. This is deliberately loose, and deliberately not byte
equality:

- Byte-identical outputs across DawDreamer runs are not guaranteed — the
  underlying JUCE host does denormal flushing, SIMD path selection, and
  buffer alignment that can introduce sub-ULP noise even with no plugin
  changes.
- `1e-4` is well above the noise floor a passthrough plugin produces under
  a parameter ramp today, and is far below the signal level we expect once
  macros are wired to guest parameter binding (issue #7 scope, landed in
  v0.3-rack-plugin but not yet reflected in audio because guest hosting is
  still a future issue).

### Why this is a warning, not a failure, today

The rack as of v0.3 is an audio passthrough — the 128 macro params are
declared for DAW modulation/automation wiring but do not yet modulate audio.
That comes in a later issue once guest hosting + macro-binding lands. The
harness treats `rms_delta <= 1e-4` as a **warning** so the infrastructure is
in place now; when guest hosting ships, flip the warning to a hard failure in
one line.

What the harness *does* hard-fail on today:

- The reference render errored.
- The ramp render errored.
- Either render produced NaN/Inf samples.
- The bundle is missing.
- DawDreamer can't load the plugin.

Those all directly prove the plugin survives variable block sizes — which is
the *other* half of the acceptance criteria and the part that is live today.

## DawDreamer quirks encountered

1. **`set_automation` vs `set_parameter`** — older DawDreamer builds exposed
   `plugin.set_automation(param_idx, ppqn_curve)`; newer builds (≥ 0.8) took a
   sample-rate-native curve. We probe `hasattr(plugin, "set_automation")` and
   fall back to a single `set_parameter(idx, 1.0)` pre-render. The
   single-value fallback still produces non-zero ramp vs reference (reference
   is pinned at 0.0 vs ramp at 1.0).
2. **Parameter index ordering** — nih_plug's `#[nested(array)]` emits IDs
   `value_1..value_128` in definition order, and VST3 param ordering matches
   declaration order. DawDreamer's `set_parameter(idx, ...)` accepts 0-based
   indices matching that order, so macro slot 0 → param index 0, slot 63 → 63,
   slot 127 → 127. The persistent blob fields (`macro_names`, `editor_state`,
   `layout_mode`, `strips`) are not VST3 params; they don't shift indices.
3. **Wheel availability** — DawDreamer wheels are most reliable on macOS
   (arm64 + x86_64) and Linux x86_64. Windows wheels are historically
   spotty, and even when present, the JUCE-based host inside DawDreamer has
   fewer months of production use on that OS. We exclude Windows from the
   nightly harness on this basis.
4. **Quarantine on first run** — on freshly-built `target/bundled/` bundles,
   macOS may attach `com.apple.quarantine` xattrs that cause DawDreamer to
   refuse to load the plugin without a clear error. The harness's error
   handler points the user at `xattr -dr com.apple.quarantine target/bundled/`
   as the standard remedy.
5. **`error: attempt to map invalid URI` stderr noise** — DawDreamer (via
   its embedded JUCE host) emits one `error: attempt to map invalid URI
   '<path>'` line to stderr per `make_plugin_processor` call on macOS VST3
   bundles, regardless of whether the path is relative, absolute, or
   absolute-canonical. The plugin loads successfully in every case; the
   message is a JUCE URL-parser quirk on bundle directories (VST3 on macOS
   is a dir, not a file). We've verified it doesn't affect correctness:
   rendered audio is valid, exit code reflects real failures, and the
   warnings coming back are about the RMS delta threshold (a product-side
   passthrough property), not about JUCE complaints. The harness does not
   attempt to suppress these; muting JUCE's stderr from Python would
   require `os.dup2()` gymnastics that could hide genuine errors. We log
   and move on.
6. **Reinit per block size** — we create a fresh `RenderEngine` + plugin
   processor for each of the three block sizes rather than mutating block
   size on an existing engine, because `setupProcessing` on a hosted VST3
   only fires on engine construction in DawDreamer's current design. This is
   slower but correct; each block-size pass is an honest cold start.

## File layout

- `pluginrack/src/pluginrack/commands/verify_bitwig_mod.py` — the harness
  itself. All DawDreamer + NumPy imports are late (inside `run()`) so that
  `pluginrack --help` and unrelated subcommands keep working without the
  `verify` extras group installed.
- `pluginrack/src/pluginrack/commands/verify.py` — adds a `bitwig-mod`
  sub-subcommand under `verify` that lazily dispatches to the harness.
- `pluginrack/pyproject.toml` — `dawdreamer>=0.7` + `numpy>=2.0` already
  present under `[project.optional-dependencies.verify]`. Version bumped
  `0.2.0 → 0.3.0`.
- `.github/workflows/nightly.yml` — new `verify-bitwig-mod` job, macos-15
  runner, `uv sync --extra verify` then `uv run pluginrack verify bitwig-mod`.
- `DEV_WORKFLOW.md` — tier table now lists `5a verify render`, new
  `5c verify bitwig-mod`, `6 verify rt-safety`.

## Acceptance mapping

| Issue #14 bullet | Where satisfied |
|------------------|------------------|
| Offline render at 48 kHz stereo | `SAMPLE_RATE = 48_000`, DawDreamer `RenderEngine(48000, block)` |
| Variable block sizes {64, 511, 1024} | `BLOCK_SIZES = (64, 511, 1024)` — reinit per block |
| Sample of macro params (0, 63, 127) | `MACRO_SAMPLE_INDICES = (0, 63, 127)` |
| Ramp 0 → 1 across 2 s | `np.linspace(0.0, 1.0, SAMPLE_RATE * 2.0)` via `set_automation` (falls back to `set_parameter(1.0)`) |
| Differ-from-silence check | `_rms_delta` + `RMS_DELTA_THRESHOLD = 1e-4` (warn-only today; flips to hard-fail when guest hosting lands) |
| Wire into nightly only, not PR-time CI | `nightly.yml` gets `verify-bitwig-mod` job; `ci.yml` unchanged |
| DawDreamer optional dep | `[project.optional-dependencies.verify]`; late import in the subcommand |
| Clear error on dawdreamer import/init failure | `_import_deps` raises `SystemExit` with install hint; `_make_engine_and_plugin` raises `SystemExit` with quarantine/arch hint |
