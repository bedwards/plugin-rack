#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11,<3.13"
# dependencies = [
#   "dawdreamer>=0.7",
#   "numpy>=2.0",
#   "soundfile>=0.12",
# ]
# ///
"""Tier 5a — bit-identical passthrough render verification for rack-plugin.

Loads ``target/bundled/rack-plugin.vst3``, feeds a deterministic 2-second
48 kHz stereo test signal through it via DawDreamer's offline RenderEngine,
and asserts the output is *bit-identical* to the input.

This check is valid ONLY while rack-plugin's ``process()`` is a literal
passthrough (as of v0.6.0: ``ProcessStatus::Normal`` with no writes to the
buffer). The moment the rack introduces any actual DSP — guest hosting,
gain-staging, macro-to-audio modulation, anything — ``np.array_equal``
will start failing and the acceptance criterion must loosen (RMS delta
threshold, spectral-distance, or a golden reference WAV).

Called by ``pluginrack verify render`` and by the nightly CI workflow.

Runs under ``uv run --python 3.12 --extra verify``. The ``--python 3.12``
pin is because DawDreamer's PyPI wheels at v0.7+ are cp311 / cp312 only —
no cp313 wheel as of 2026-04.

Exit codes:
  0  — INPUT == OUTPUT bitwise AND output is non-silent
  1  — bundle missing / load failed / any other error
  2  — mismatch detected (diagnostic printed to stderr)
"""

from __future__ import annotations

import sys
from pathlib import Path


# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

# Sample rate + duration for the test render. 48 kHz is the rack's target
# rate and what Bitwig drives at by default.
SAMPLE_RATE: int = 48_000
DURATION_SECONDS: float = 2.0

# Block size the RenderEngine will chunk the render into. 512 is the
# canonical audio-callback size in most DAWs and matches what
# research/ci_verification.md §"Offline audio rendering" recommends.
BLOCK_SIZE: int = 512

# Seed for the deterministic test waveform. Fixed so the same input is
# produced on every run, across machines.
RNG_SEED: int = 0xD06F00D

# Amplitude floor for the "non-silent" assertion guard. The plugin could
# theoretically zero the buffer and still match np.array_equal if our
# input were all zeros — so we assert the input itself is non-trivial.
MIN_PEAK: float = 0.05


# ---------------------------------------------------------------------------
# Bundle resolution
# ---------------------------------------------------------------------------

def _bundle_path() -> Path:
    """Locate ``rack-plugin.vst3`` from repo root or from ``pluginrack/`` CWD.

    Returns an absolute path. DawDreamer's underlying JUCE URL parser warns
    on stderr when handed a relative path containing ``..`` — the plugin
    still loads but the output is noisy. Always absolute.
    """
    candidates = (
        Path("target/bundled/rack-plugin.vst3"),
        Path("../target/bundled/rack-plugin.vst3"),
    )
    for p in candidates:
        if p.exists():
            return p.resolve()
    return candidates[0].resolve()


# ---------------------------------------------------------------------------
# Test signal
# ---------------------------------------------------------------------------

def _make_test_signal(np):
    """Build the deterministic 2-second stereo test waveform.

    Composition (stereo float32, shape ``(2, n_samples)``):
      * seeded white noise scaled to ~0.1 RMS
      * + 440 Hz sine tone at 0.25 amplitude

    The combination guarantees broadband excitation (noise) + a known
    periodic component (sine) so a downstream spectral-diff test, if ever
    added, has something meaningful to latch onto. For pure passthrough
    verification only the determinism + non-silence properties matter.
    """
    n = int(SAMPLE_RATE * DURATION_SECONDS)

    rng = np.random.default_rng(RNG_SEED)
    noise = rng.standard_normal((2, n)).astype(np.float32) * 0.1

    t = np.arange(n, dtype=np.float32) / SAMPLE_RATE
    sine = (np.sin(2.0 * np.pi * 440.0 * t) * 0.25).astype(np.float32)

    sig = noise + sine[np.newaxis, :]  # broadcast mono sine over both channels
    # Clip to [-1, 1] to avoid any host-side clamping surprise; then ensure
    # contiguous float32 because DawDreamer's playback processor expects it.
    return np.ascontiguousarray(np.clip(sig, -1.0, 1.0), dtype=np.float32)


# ---------------------------------------------------------------------------
# Import helper (lazy — PEP 723 block handles deps under `uv run`)
# ---------------------------------------------------------------------------

def _import_deps():
    try:
        import dawdreamer as dd
        import numpy as np
    except ImportError as e:
        sys.stderr.write(
            "verify_render requires dawdreamer + numpy. Run under `uv run`:\n"
            "    uv run --python 3.12 --extra verify python scripts/verify_render.py\n"
            f"Original import error: {e}\n"
        )
        sys.exit(1)
    return dd, np


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main() -> int:
    dd, np = _import_deps()

    bundle = _bundle_path()
    if not bundle.exists():
        sys.stderr.write(
            f"bundle not found: {bundle}\n"
            "Build it first:  cargo xtask bundle rack-plugin --release\n"
        )
        return 1

    print(f"verify_render: bundle = {bundle}")
    print(f"  sample rate = {SAMPLE_RATE} Hz")
    print(f"  duration    = {DURATION_SECONDS:.1f} s")
    print(f"  block size  = {BLOCK_SIZE}")

    # Build input signal.
    test_input = _make_test_signal(np)
    n_samples = test_input.shape[1]

    # Sanity-check the input is not silent.
    peak = float(np.max(np.abs(test_input)))
    if peak < MIN_PEAK:
        sys.stderr.write(
            f"test signal peak {peak:.3e} below MIN_PEAK {MIN_PEAK:g} — "
            "something is wrong with _make_test_signal\n"
        )
        return 1

    # Instantiate engine + plugin.
    engine = dd.RenderEngine(SAMPLE_RATE, BLOCK_SIZE)
    try:
        plugin = engine.make_plugin_processor("rack", str(bundle))
    except Exception as e:
        sys.stderr.write(
            f"DawDreamer failed to load {bundle}: {e}\n"
            "Common causes: (a) plugin was not bundled for this arch — run "
            "`cargo xtask bundle rack-plugin --release`; (b) on Apple Silicon, "
            "DawDreamer needs the arm64 slice; (c) first-run quarantine — "
            "run `xattr -dr com.apple.quarantine target/bundled/`.\n"
        )
        return 1

    # Feed the test signal into the plugin via a playback processor.
    # Graph: source (playback) -> plugin (connected to source's output).
    source = engine.make_playback_processor("src", test_input)
    engine.load_graph([(source, []), (plugin, ["src"])])

    # Render DURATION_SECONDS.
    engine.render(DURATION_SECONDS)
    output = engine.get_audio()

    # DawDreamer may return a buffer a sample or two longer/shorter due to
    # internal block alignment. Trim to the common length for comparison.
    n = min(test_input.shape[1], output.shape[1])
    in_trim = test_input[:, :n]
    out_trim = np.ascontiguousarray(output[:, :n], dtype=np.float32)

    # Non-silence guard on the output.
    if not out_trim.any():
        sys.stderr.write(
            "output buffer is all zeros — passthrough would emit the input, "
            "so a silent output means the plugin dropped or zeroed audio\n"
        )
        return 2

    # Bit-identical check. np.array_equal compares shape + element-wise
    # equality without tolerance — this is only valid for a literal
    # passthrough plugin. See module docstring.
    if np.array_equal(in_trim, out_trim):
        in_peak = float(np.max(np.abs(in_trim)))
        out_peak = float(np.max(np.abs(out_trim)))
        print(f"  samples compared = {n}")
        print(f"  input peak       = {in_peak:.6f}")
        print(f"  output peak      = {out_peak:.6f}")
        print("PASS: input == output, bit-identical passthrough")
        return 0

    # Mismatch — print a diagnostic.
    diff = out_trim - in_trim
    rms = float(np.sqrt(np.mean(diff * diff)))
    max_abs = float(np.max(np.abs(diff)))
    nonzero = int(np.count_nonzero(diff))
    sys.stderr.write(
        "MISMATCH: output differs from input\n"
        f"  samples compared    = {n}\n"
        f"  rms(output-input)   = {rms:.6e}\n"
        f"  max|output-input|   = {max_abs:.6e}\n"
        f"  differing samples   = {nonzero} / {diff.size}\n"
        "If rack-plugin gained DSP on purpose, loosen this script's assertion "
        "(e.g. RMS threshold or spectral distance) — bit-identical only holds "
        "for a literal passthrough plugin.\n"
    )
    return 2


if __name__ == "__main__":
    sys.exit(main())
