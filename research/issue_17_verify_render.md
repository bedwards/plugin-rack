# Issue #17 — offline bit-identical passthrough render verification

## What this is

`scripts/verify_render.py` — a tier-5a offline render harness that renders a
deterministic 2-second 48 kHz stereo test signal through the bundled
`rack-plugin.vst3` via DawDreamer and asserts the output is **bit-identical**
to the input (`np.array_equal(input, output)` AND `output.any()`).

Invoked by `pluginrack verify render` and by the `verify-render` job in
`.github/workflows/nightly.yml`.

## Why bit-identical, and only for now

As of rack-plugin v0.6.0, `process()` is a literal passthrough:

```rust
fn process(
    &mut self,
    _buffer: &mut Buffer,
    _aux: &mut AuxiliaryBuffers,
    _context: &mut impl ProcessContext<Self>,
) -> ProcessStatus {
    ProcessStatus::Normal
}
```

No writes to the audio buffer. VST3 hosts interpret that as "pass the input
through unchanged". DawDreamer obligingly renders the input straight back out,
and we get byte-for-byte equality — no denormal flushing, no SIMD rounding
drift, no sub-ULP noise, because the rack never touches the samples.

That property **will break** the moment rack-plugin does any DSP of its own:

- guest hosting lands (real plugins in the chain)
- macro-to-audio modulation wires macros to gain stages
- even a single "apply output gain of 1.0" changes the float path and
  introduces at least ULP-level difference from multiplications that are not
  compile-time-folded

When that happens, the script's assertion has to loosen to one of:

1. **RMS-delta threshold** below a tolerance (e.g. `1e-6` or `1e-4`).
2. **Golden-reference WAV** checked into `tests/fixtures/` and compared
   sample-by-sample or spectrally.
3. **Spectral distance / centroid** assertion — sometimes the right call for
   non-deterministic DSP (reverbs, randomized modulation).

The script's module docstring and its PASS/MISMATCH diagnostic both call this
out so future maintainers don't "fix" the assertion without thinking.

## Test signal

Deterministic, seeded-so-reproducible-across-machines:

- **Seed**: `0xD06F00D` (NumPy `default_rng`)
- **White noise**: stereo Gaussian, scaled to ~0.1 RMS
- **Plus 440 Hz sine**: 0.25 amplitude, same signal on both channels
- **Clipped to [-1, 1]** to avoid host-side clamping surprises
- **float32, shape (2, SR * 2)**

Rationale for a noise+sine composite rather than pure noise or pure sine:

- Noise alone is fine for bit-identity but gives a future spectral-distance
  test nothing to latch onto — every bin is ~flat.
- Sine alone is too narrowband; it wouldn't exercise broadband DSP when the
  rack eventually does something interesting.
- Noise + tonal component buys both: broadband excitation for future
  spectral tests, and a known periodic feature for visual debugging if a
  WAV is dumped for inspection.

For today's bit-identity check, neither property is load-bearing — any
non-silent deterministic waveform would suffice — but the signal is designed
to remain useful when this script gets extended.

## Graph topology

DawDreamer renders plugin processors inside a directed graph. We need input
to *arrive* at the plugin or else a passthrough emits silence. The graph is:

```
(source: PlaybackProcessor, inputs=[]) --> (plugin: PluginProcessor, inputs=["src"])
```

The playback processor wraps our in-memory NumPy test signal. The plugin
processor names "src" as its audio input. When `engine.render(2.0)` runs,
each 512-sample block pulls from source, feeds the plugin, and the plugin's
output is captured by `engine.get_audio()`.

Without the playback source, `engine.get_audio()` returns whatever DawDreamer
synthesizes as "no input" — effectively silence — and the "passthrough"
emits silence too, which would trivially pass `np.array_equal` on a zero
input but fails the `output.any()` non-silence guard. Belt and braces.

## Block size

512 samples. The canonical audio-callback size. We don't iterate multiple
block sizes here — that's `verify bitwig-mod`'s job (tier 5c). This tier is
about byte-for-byte I/O correctness, not variable-block tolerance.

## Length trim

DawDreamer's internal block alignment can produce an output buffer one or
two samples longer or shorter than the input. We `min(in.shape[1],
out.shape[1])` trim before comparing, identical to what `verify bitwig-mod`
does in its `_rms_delta`. This trim is safe for passthrough today because
the rack adds zero latency; the moment the rack adds latency (a buffered
FX, lookahead), this trim has to become a delay-compensated alignment.

## Python pin

DawDreamer 0.7+ ships PyPI wheels for cp311 and cp312 only — no cp313 as of
2026-04. Both the CLI wrapper (`pluginrack verify render`) and the nightly
job pin `--python 3.12`. When DawDreamer ships 3.13 wheels, drop the pin.

## `uv run --script` vs `uv run --extra verify`

The predecessor issue #14 (`verify bitwig-mod`) imports DawDreamer as a
Python module and so was wired through the pluginrack project's
`[project.optional-dependencies].verify` extras group. That works when
invoked from the `pluginrack/` sub-directory.

For this script, `pluginrack verify render` can be invoked from anywhere —
the repo root most commonly — and `uv run --extra verify python ...` from
outside a project triggers `warning: --extra verify has no effect when used
outside of a project` and then fails because dawdreamer is missing.

Solution: the script carries a PEP-723 inline metadata header declaring its
own deps, and `pluginrack verify render` invokes it via `uv run --script`.
That activates PEP 723 regardless of cwd. The `[verify]` extras group is
still kept as a belt-and-braces path for anyone who prefers `uv sync --extra
verify` and running the script under a pre-synced venv.

## What "pass" means

Exit 0 from the script, printed summary:

```
verify_render: bundle = /abs/path/rack-plugin.vst3
  sample rate = 48000 Hz
  duration    = 2.0 s
  block size  = 512
  samples compared = 96000
  input peak       = 0.656486
  output peak      = 0.656486
PASS: input == output, bit-identical passthrough
```

Exit codes:

- **0** — INPUT == OUTPUT (np.array_equal) AND OUTPUT is non-silent (`any`).
- **1** — bundle missing, DawDreamer import failed, or plugin failed to load.
- **2** — mismatch detected. Diagnostic prints `rms(output-input)`,
  `max|output-input|`, and count of differing samples.

## Local verification run

On macos-15 (Apple Silicon) with the VST3 pre-bundled via
`cargo xtask bundle rack-plugin --release`:

```
$ uv run --python 3.12 --script scripts/verify_render.py
error: attempt to map invalid URI `/Users/.../rack-plugin.vst3'   # benign JUCE warning
verify_render: bundle = /Users/.../rack-plugin.vst3
  ...
  samples compared = 96000
  input peak       = 0.656486
  output peak      = 0.656486
PASS: input == output, bit-identical passthrough
```

The `attempt to map invalid URI` warning is a JUCE stderr nuisance also seen
by `verify bitwig-mod` — harmless, the plugin still loads and renders.
