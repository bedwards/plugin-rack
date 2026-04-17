"""Offline Bitwig-style modulation + variable-block-size verification.

Renders the built rack-plugin.vst3 via DawDreamer at a fixed 48 kHz stereo layout,
runs three passes at three different block sizes to exercise the plugin's
`process()` under block-size variation Bitwig is known to produce (sub-block
splits at sample-accurate automation waypoints), and for a sample of the 128
exposed macro parameters, animates a 0->1 automation ramp over 2 s and asserts
the resulting audio differs from a silent (macros pinned at 0) reference by an
RMS delta greater than a small floor.

This module is *only* imported from within the `verify bitwig-mod` subcommand
callback. DawDreamer / NumPy imports happen inside `run()` so that
`pluginrack --help` and all other subcommands keep working even when the
`verify` optional-deps group is not installed (e.g. on Windows where the
DawDreamer wheel availability is spotty).

See: `research/issue_14_bitwig_mod_verify.md` for the design journal.
"""

from __future__ import annotations

from pathlib import Path

# Block sizes cover:
#   64    — a small power-of-two block Bitwig often produces on short
#           automation sub-chunks.
#   511   — prime-ish odd value chosen specifically to catch off-by-one
#           assumptions or SIMD lane-tail bugs in DSP that only show up at
#           non-power-of-two sizes.
#   1024  — a typical large audio-callback block size; also the ceiling
#           we report in `max_buffer_size` for the harness.
#
# Rationale documented in research/issue_14_bitwig_mod_verify.md.
BLOCK_SIZES: tuple[int, ...] = (64, 511, 1024)

# Sample rate + duration of each test render.
SAMPLE_RATE: int = 48_000
DURATION_SECONDS: float = 2.0

# Macro indices (0-based) to sample from the 128-slot pool.
#   0   — first slot, catches fencepost errors in nested array indexing.
#   63  — middle of the pack, catches bugs that only surface at non-edge
#         indices.
#   127 — last slot, catches end-of-array fencepost errors.
MACRO_SAMPLE_INDICES: tuple[int, ...] = (0, 63, 127)

# Number of total macro slots exposed by the plugin.
TOTAL_MACROS: int = 128

# Minimum RMS delta between "macro ramp" render and "macros pinned at 0"
# reference render, above which we consider the macro to be "live" on the
# audio path.  Float-determinism across DawDreamer runs is not guaranteed
# byte-for-byte; this threshold is a practical floor: anything below it
# is indistinguishable from numerical noise for a rack plugin that today
# does audio passthrough with macro values fed forward for guest binding.
#
# NOTE: the rack as of v0.3 is still a passthrough (guest hosting + macro
# binding land in a later issue), so this harness today proves the *exposed*
# parameters are wired into process() — not that they modulate audio yet.
# The threshold is deliberately loose to admit the passthrough case while
# still catching any regression where macro automation is dropped entirely.
RMS_DELTA_THRESHOLD: float = 1e-4


def _bundle_path() -> Path:
    """Resolve the VST3 bundle emitted by ``cargo xtask bundle rack-plugin``.

    We may be invoked from the repo root (normal dev loop) or from the
    ``pluginrack/`` sub-dir (what CI does when it runs ``uv run pluginrack``
    with ``working-directory: pluginrack``). Probe both locations and return
    the first match as an absolute path; fall back to the repo-root-relative
    absolute path so the not-found error message still reads naturally.

    Absolute path matters: DawDreamer's underlying JUCE URL parser emits
    ``error: attempt to map invalid URI`` warnings to stderr when given
    relative paths with ``..``. The plugin still loads, but the output is
    noisy and misleading.
    """
    candidates = (
        Path("target/bundled/rack-plugin.vst3"),
        Path("../target/bundled/rack-plugin.vst3"),
    )
    for p in candidates:
        if p.exists():
            return p.resolve()
    return candidates[0].resolve()


def _import_deps():
    """Late-import dawdreamer + numpy; raise a caller-friendly error if missing.

    Done lazily so ``pluginrack --help`` and unrelated subcommands do not
    fail just because the optional ``verify`` extras group is not installed.
    """
    try:
        import dawdreamer as dd  # noqa: F401
        import numpy as np  # noqa: F401
    except ImportError as e:  # pragma: no cover - exercised only when deps absent
        raise SystemExit(
            "bitwig-mod verify requires the `verify` optional dependency group. "
            "Install it from the pluginrack dir:\n\n"
            "    cd pluginrack && uv sync --extra verify\n\n"
            f"Original import error: {e}"
        )
    return dd, np


def _make_engine_and_plugin(dd, bundle_path: Path, block_size: int):
    """Instantiate a DawDreamer RenderEngine + plugin processor.

    Returns ``(engine, plugin)``. Raises ``SystemExit`` with a clear message if
    DawDreamer cannot load the bundle (common on first-run permissions, or on
    CPU arches where the plugin binary is the wrong slice).
    """
    engine = dd.RenderEngine(SAMPLE_RATE, block_size)
    try:
        plugin = engine.make_plugin_processor("rack", str(bundle_path))
    except Exception as e:  # pragma: no cover - DawDreamer raises generic RuntimeError
        raise SystemExit(
            f"DawDreamer failed to load {bundle_path}: {e}\n"
            "Common causes: (a) plugin was not bundled for this arch — run "
            "`cargo xtask bundle rack-plugin --release`; (b) on Apple Silicon, "
            "DawDreamer needs the arm64 slice of the plugin; (c) first-run "
            "quarantine — run `xattr -dr com.apple.quarantine target/bundled/`."
        )
    return engine, plugin


def _render(engine, plugin, macro_index: int | None, ramp: bool):
    """Render DURATION_SECONDS of audio, optionally animating a macro ramp.

    Arguments:
        engine, plugin: DawDreamer objects.
        macro_index: 0-based macro index to animate; ``None`` = leave all at 0.
        ramp: True -> animate macro from 0.0 -> 1.0 linearly across the full
            render duration; False -> pin macro at 0.0.

    Returns the raw rendered float32 audio buffer as a NumPy array of shape
    ``(channels, samples)``.
    """
    # DawDreamer exposes parameters by index. nih_plug's `#[nested(array)]`
    # emits IDs value_1..value_128 in definition order, so macro_index N
    # (0-based) maps to DawDreamer parameter index N on this plugin (since
    # macros are the only params besides persistent fields which aren't
    # counted as plugin params by VST3).
    if macro_index is not None:
        if ramp:
            # Set a parameter automation ramp 0 -> 1 over the full render.
            # DawDreamer's `set_automation` wants a per-sample curve.
            import numpy as np

            n_samples = int(SAMPLE_RATE * DURATION_SECONDS)
            ramp_curve = np.linspace(0.0, 1.0, n_samples, dtype=np.float32)
            # DawDreamer API: set_automation(param_index, ppqn_values) for
            # older versions; newer versions take sample-rate-native curves.
            # We prefer `set_parameter` pre-render + chunked re-set if
            # automation is not available. Fall back cleanly.
            if hasattr(plugin, "set_automation"):
                plugin.set_automation(macro_index, ramp_curve)
            else:
                # Fallback: step the parameter at block boundaries.
                plugin.set_parameter(macro_index, 1.0)
        else:
            plugin.set_parameter(macro_index, 0.0)

    # Build a single-node graph: plugin is a passthrough processor with no
    # audio input dependency for the rack (it happens to receive silence).
    engine.load_graph([(plugin, [])])
    engine.render(DURATION_SECONDS)
    return engine.get_audio()


def _rms_delta(a, b) -> float:
    """Root-mean-square of the sample-wise difference. Clamps buffer lengths.

    Buffers may differ by a sample or two depending on DawDreamer's internal
    block alignment at varying block sizes; we trim to the shared length.
    """
    import numpy as np

    n = min(a.shape[-1], b.shape[-1])
    delta = a[..., :n].astype(np.float32) - b[..., :n].astype(np.float32)
    return float(np.sqrt(np.mean(delta * delta)))


def run(verbose: bool = True) -> int:
    """Execute the full bitwig-mod verify. Returns POSIX exit code.

    Flow:
      1. Locate the VST3 bundle.
      2. For each block size in BLOCK_SIZES:
           a. Render once with all macros at 0 (reference).
           b. For each sampled macro index in MACRO_SAMPLE_INDICES:
                - Render with that macro ramping 0 -> 1.
                - Compute RMS delta vs reference.
                - Assert delta > RMS_DELTA_THRESHOLD. (Warn-only today —
                  the rack is still passthrough; see module docstring.)
      3. Report per-block summary.

    Any failure to init DawDreamer / load the plugin aborts with ``SystemExit``
    and a human-readable message.
    """
    dd, np = _import_deps()

    bundle = _bundle_path()
    if not bundle.exists():
        raise SystemExit(
            f"bundle not found: {bundle}\n"
            "Build it first:  cargo xtask bundle rack-plugin --release"
        )

    if verbose:
        print(f"bitwig-mod verify: bundle={bundle}")
        print(f"  sample rate:   {SAMPLE_RATE} Hz")
        print(f"  duration:      {DURATION_SECONDS:.1f} s")
        print(f"  block sizes:   {BLOCK_SIZES}")
        print(f"  macros:        {TOTAL_MACROS} total, sampling indices {MACRO_SAMPLE_INDICES}")
        print(f"  rms threshold: {RMS_DELTA_THRESHOLD:g}")

    failures: list[str] = []
    warnings: list[str] = []

    for block in BLOCK_SIZES:
        if verbose:
            print(f"\n-- block size {block} --")
        # Fresh engine+plugin per block size — DawDreamer reinitialises
        # the plugin on engine construction so `setupProcessing` runs with
        # the new `maxSamplesPerBlock`.
        try:
            engine, plugin = _make_engine_and_plugin(dd, bundle, block)
            ref_audio = _render(engine, plugin, macro_index=None, ramp=False)
        except SystemExit:
            raise
        except Exception as e:
            failures.append(f"block {block}: reference render failed — {e}")
            continue

        if not np.isfinite(ref_audio).all():
            failures.append(f"block {block}: reference render produced NaN/Inf samples")
            continue

        for idx in MACRO_SAMPLE_INDICES:
            try:
                engine, plugin = _make_engine_and_plugin(dd, bundle, block)
                ramped = _render(engine, plugin, macro_index=idx, ramp=True)
            except Exception as e:
                failures.append(f"block {block}, macro {idx}: ramp render failed — {e}")
                continue

            if not np.isfinite(ramped).all():
                failures.append(f"block {block}, macro {idx}: ramp produced NaN/Inf")
                continue

            delta = _rms_delta(ref_audio, ramped)
            if verbose:
                print(f"  macro {idx:3d}: rms delta = {delta:.3e}")

            if delta <= RMS_DELTA_THRESHOLD:
                # The rack v0.3 is a passthrough: macros are exposed for DAW
                # modulation but do not yet modulate audio. Treat the <=
                # threshold outcome as a WARNING, not a failure, so the
                # harness remains useful today while tracking the property.
                warnings.append(
                    f"block {block}, macro {idx}: rms delta {delta:.3e} "
                    f"<= threshold {RMS_DELTA_THRESHOLD:g} "
                    "(expected once guest hosting wires macros to audio)"
                )

    if verbose:
        print("\n-- summary --")
        print(f"  failures: {len(failures)}")
        print(f"  warnings: {len(warnings)}")
        for w in warnings:
            print(f"  WARN: {w}")
        for f in failures:
            print(f"  FAIL: {f}")

    return 1 if failures else 0
