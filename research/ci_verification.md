# CI / Verification / Dev Workflow for Rust VST3 Plugins

Research snapshot: April 2026. Scope: plugin-rack (a nih-plug-based Rust VST3/CLAP
plugin workspace) driven by a Claude Code orchestrator that opens PRs, waits for
CI, reads Gemini Code Assist reviews, and auto-merges when green.

Bottom line up front: the standard stack is `nih-plug` + `cargo xtask bundle` on
a matrix of macOS-14/15 / windows-2025 / ubuntu-24.04, cached by
`Swatinem/rust-cache@v2`, then validated with `pluginval --strictness-level 10`
and Steinberg's `validator` built from `vst3sdk`. Auto-merge goes through
`gh pr merge --auto --squash` gated on branch protection. Gemini Code Assist is
a GitHub App that comments as `gemini-code-assist[bot]`; its comments are
readable via `gh api /repos/{owner}/{repo}/pulls/{n}/comments`.

## 1. GitHub Actions Matrix Build (macOS / Windows / Ubuntu)

### Runner selection (April 2026)

GitHub's hosted-runner lineup relevant to audio plugin CI:

| Label | Arch | Notes |
|---|---|---|
| `macos-14`, `macos-15` | Apple Silicon (M1/M2) | Default macOS runner tier. `macos-13` is the last Intel runner. |
| `macos-14-large`, `macos-15-large` | Intel x86_64 | Paid tier; use if you need an Intel-native build instead of cross-compiling. |
| `windows-2022`, `windows-2025` | x86_64 | `windows-2025` is GA as of late 2025. |
| `ubuntu-22.04`, `ubuntu-24.04` | x86_64 | `ubuntu-latest` now points at 24.04. |
| `ubuntu-24.04-arm` | aarch64 | Free tier ARM Linux runner. |

Recommended matrix for a VST3/CLAP workspace:

```yaml
strategy:
  fail-fast: false
  matrix:
    include:
      - { name: linux-x86_64,   os: ubuntu-24.04,  target: x86_64-unknown-linux-gnu,  cross: '' }
      - { name: macos-arm64,    os: macos-15,      target: aarch64-apple-darwin,      cross: '' }
      - { name: macos-x86_64,   os: macos-15,      target: x86_64-apple-darwin,       cross: 'x86_64-apple-darwin' }
      - { name: windows-x86_64, os: windows-2025,  target: x86_64-pc-windows-msvc,    cross: '' }
```

Two macOS entries let you ship a **universal** bundle (see `cargo xtask
bundle-universal` below) without paying for a second runner — both slices build
on the same Apple Silicon runner, one native and one via the installed
`x86_64-apple-darwin` target.

### Caching with Swatinem/rust-cache

`Swatinem/rust-cache@v2` is the de-facto standard. Key knobs:

- `shared-key`: stable across jobs — use this if multiple jobs (build, test,
  bench) of the same target should share a cache.
- `key`: differentiates similar jobs — append target triple here.
- `prefix-key`: invalidate the whole cache (bump this when you change Rust
  toolchain).
- `cache-targets: true` (default): caches `target/`. **Disable on Windows** if
  you see "resource busy" flakiness — nih-plug's own CI does this.
- `cache-on-failure: true`: keep partial caches from failed builds (speeds up
  re-runs).
- `save-if`: `${{ github.ref == 'refs/heads/main' }}` — only save cache on
  main; PRs restore but don't write, preventing cache pollution.

```yaml
- uses: Swatinem/rust-cache@v2
  with:
    prefix-key: "v1-rust"
    shared-key: "plugin-rack-${{ matrix.target }}"
    cache-on-failure: true
    save-if: ${{ github.ref == 'refs/heads/main' }}
    # On Windows, target-dir caching is flaky for audio plugin builds
    cache-targets: ${{ !startsWith(matrix.os, 'windows') }}
```

### Linux system dependencies for nih-plug

nih-plug builds against ALSA/X11/XCB. On `ubuntu-24.04`:

```yaml
- name: Install Linux deps
  if: startsWith(matrix.os, 'ubuntu')
  run: |
    sudo apt-get update
    sudo apt-get install -y \
      libasound2-dev libgl-dev libjack-jackd2-dev \
      libx11-xcb-dev libxcb1-dev libxcb-dri2-0-dev \
      libxcb-icccm4-dev libxcursor-dev libxkbcommon-dev \
      libxcb-shape0-dev libxcb-xfixes0-dev \
      libfreetype-dev libfontconfig1-dev
```

Windows and macOS need no extra system packages for a baseline nih-plug build.

### Toolchain setup

nih-plug **requires nightly** (uses feature flags like `simd`). Pin it:

```yaml
- uses: dtolnay/rust-toolchain@nightly
  with:
    toolchain: nightly-2026-03-15
    targets: ${{ matrix.target }}
    components: rustfmt, clippy
```

Pinning a specific nightly date prevents "works yesterday, broken today" and is
what nih-plug upstream does. Update it deliberately in its own PR.

## 2. `cargo xtask bundle` (nih-plug)

nih-plug ships `nih_plug_xtask` — a library you wire into your workspace's
`xtask` crate. The entry point is `cargo xtask bundle <package>`, which:

1. Inspects which `nih_export_*!` macros the plugin calls.
2. Builds the cdylib.
3. Wraps the binary into the correct plugin-format layout:
   - **VST3**: `<name>.vst3/Contents/{arch}/<name>.{so,vst3,dll}` plus
     `PkgInfo` and `moduleinfo.json`.
   - **CLAP**: single `<name>.clap` file (renamed cdylib).
   - **macOS**: also produces a proper `.bundle` with `Info.plist`.
4. Writes into `target/bundled/`.

It does **not** produce Apple Audio Units (`.component`) — that requires the
`auv2` feature and is still considered experimental in nih-plug; for most
workflows, stick to VST3 + CLAP.

### Commands

```bash
# Build all packages named in bundler.toml (release):
cargo xtask bundle --release                # -> target/bundled/<name>.{vst3,clap}

# Build a specific package:
cargo xtask bundle my_compressor --release

# macOS universal binary (lipo'd x86_64 + aarch64 into one bundle):
cargo xtask bundle-universal my_compressor --release

# Discover what's bundleable (used by CI scripting):
cargo xtask known-packages
```

### `bundler.toml`

Lives at the workspace root. Minimal example:

```toml
[my_compressor]
name = "My Compressor"
vendor = "Plugin Rack"
version = "0.1.0"

[my_reverb]
name = "My Reverb"
vendor = "Plugin Rack"
```

nih-plug's own CI pattern: shell out to `cargo xtask known-packages`, parse the
list, pass them all to one `cargo xtask bundle` invocation.

```yaml
- name: Bundle all plugins
  shell: bash
  run: |
    packages=$(cargo xtask known-packages)
    args=()
    for p in $packages; do args+=("$p"); done
    if [[ "${{ matrix.name }}" == "macos-x86_64" ]]; then
      cargo xtask bundle-universal "${args[@]}" --release
    else
      cargo xtask bundle "${args[@]}" --release ${{ matrix.cross && format('--target {0}', matrix.cross) || '' }}
    fi
```

## 3. Steinberg VST3 Validator

Steinberg ships a reference validator (`validator`) as a sample host in the
VST3 SDK. It's the authoritative conformance check — `pluginval` also wraps it
at strictness 5+ when built with `-DPLUGINVAL_VST3_VALIDATOR=1`.

### Getting the binary

Two options:

1. **Build from source** (robust, reproducible):

   ```bash
   git clone --recursive --depth 1 https://github.com/steinbergmedia/vst3sdk.git
   cmake -S vst3sdk -B vst3sdk/build \
     -DCMAKE_BUILD_TYPE=Release \
     -DSMTG_ENABLE_VST3_HOSTING_EXAMPLES=OFF \
     -DSMTG_ENABLE_VST3_PLUGIN_EXAMPLES=OFF
   cmake --build vst3sdk/build --target validator --config Release -j
   # Binary: vst3sdk/build/bin/Release/validator (Linux/macOS) or validator.exe (Windows)
   ```

2. **Pre-built release artifact**: no official Steinberg binary distribution,
   so checking it in or publishing your own via GitHub Releases is the
   pattern. Cache the built binary on each runner by SDK version.

### CLI usage

```bash
# Basic conformity check:
validator /path/to/MyPlugin.vst3

# Extended validation (stricter, runs more tests, slower):
validator -e /path/to/MyPlugin.vst3

# Specific test suite:
validator -suite Standard /path/to/MyPlugin.vst3
validator -suite Extensive /path/to/MyPlugin.vst3
```

Exit code: 0 = pass, non-zero = failures (count reported on stdout).

### Caching the build in CI

```yaml
- name: Cache vst3sdk validator
  id: vst3sdk-cache
  uses: actions/cache@v4
  with:
    path: vst3sdk/build/bin
    key: vst3sdk-validator-${{ runner.os }}-${{ runner.arch }}-v3.7.12

- name: Build validator
  if: steps.vst3sdk-cache.outputs.cache-hit != 'true'
  run: |
    git clone --recursive --depth 1 --branch v3.7.12_build_20 \
      https://github.com/steinbergmedia/vst3sdk.git
    cmake -S vst3sdk -B vst3sdk/build -DCMAKE_BUILD_TYPE=Release
    cmake --build vst3sdk/build --target validator --config Release -j
```

## 4. `pluginval` (Tracktion)

`pluginval` is the go-to cross-platform tester. Runs headless, exits 0 / 1.

### Key CLI flags

| Flag | Meaning |
|---|---|
| `--validate <path>` | Plugin bundle to test (VST/VST3/AU). |
| `--strictness-level N` | 1–10. 5 is "lowest for host compat"; 10 is "ship-ready". |
| `--validate-in-process` | Run the plugin in pluginval's own process (better stack traces, no subprocess timeout indirection). |
| `--skip-gui-tests` | **Required on Linux headless runners** — GUI tests segfault without Xvfb. |
| `--timeout-ms N` | Per-test timeout (default ~30000). Bump for slower CI. |
| `--sample-rates "44100,48000,88200"` | CSV. |
| `--block-sizes "64,128,512,1024"` | CSV. |
| `--num-repeats N` | Repeat each test N times (catches flaky state). |
| `--randomise` | Shuffle test order. |
| `--verbose` | Print each test's output. |
| `--output-dir <dir>` | Write per-test logs. |
| `--data-file <path>` | Load reference input audio for deterministic tests. |

### CI invocation

```bash
# macOS / Windows — GUI tests are fine:
pluginval --strictness-level 10 \
          --validate-in-process \
          --verbose \
          --output-dir pluginval-logs \
          --validate target/bundled/MyPlugin.vst3

# Linux — must skip GUI:
xvfb-run -a pluginval \
  --strictness-level 10 \
  --skip-gui-tests \
  --validate-in-process \
  --validate target/bundled/MyPlugin.vst3
```

Recommended: install `xvfb` on Ubuntu runners and use `xvfb-run` even for
`--skip-gui-tests` builds so anything that pokes X11 lazily still has a display.

### Getting pluginval

Tracktion ships pre-built binaries at
`https://github.com/Tracktion/pluginval/releases/latest`:

- `pluginval_Linux.zip`
- `pluginval_macOS.zip` (universal)
- `pluginval_Windows.zip`

Cache per platform:

```yaml
- name: Install pluginval (Linux)
  if: startsWith(matrix.os, 'ubuntu')
  run: |
    curl -L -o pluginval.zip https://github.com/Tracktion/pluginval/releases/latest/download/pluginval_Linux.zip
    unzip pluginval.zip
    echo "$PWD" >> $GITHUB_PATH
```

## 5. Loading in a Real DAW for Smoke Testing

### REAPER (headless render) — feasible and well-trodden

REAPER ships a proper CLI that can render projects without showing a window:

```bash
# Render a pre-made project (has the plugin on a track) to WAV:
reaper -renderproject test/smoke.rpp -renderfile test/out.wav

# Combined with a Lua ReaScript (build 6.80+):
reaper -renderproject test/smoke.rpp -nosplash -new -noactions test/hook.lua
```

On Linux, wrap with `xvfb-run -a reaper …`. On macOS, the binary is at
`/Applications/REAPER.app/Contents/MacOS/REAPER`. License: REAPER is paid for
commercial use, but CI use under the evaluation license is standard practice
and explicitly tolerated by Cockos.

**Pattern**: hand-author a `.rpp` project that references a plugin by UID, drop
a few MIDI notes or input files, render, then assert output shape (file exists,
non-silent, spectral centroid within range, etc.).

### Bitwig — no official CLI render

Bitwig does not expose a headless / CLI render mode. Don't target it for CI.

### Carla (KXStudio) — great for Linux

`carla-single` is purpose-built for headless plugin testing:

```bash
# Load a single plugin with a dummy engine and smoke-test it:
carla-single dummy vst3 /path/to/MyPlugin.vst3

# The "dummy" audio driver runs without real audio hardware.
```

Carla's `discovery` tool (`carla-discovery-native`) is also invaluable: it
introspects a plugin (name, params, ports) and is fast, making it a good first
gate before running the heavier pluginval.

### Recommendation

For plugin-rack, the CI pyramid should be:

1. **Unit tests** (`cargo test`) — seconds.
2. **Validator** (Steinberg) — VST3 conformance, ~1-2s per plugin.
3. **pluginval level 10** — ~30-90s per plugin per platform.
4. **Offline render** (dawdreamer or REAPER) — golden-output comparison,
   ~seconds.
5. **DAW smoke test** (REAPER or Carla) — optional, nightly only.

## 6. Offline Audio Rendering / Golden-Output Tests

### DawDreamer (Python)

`dawdreamer` hosts VST2/VST3 from Python on macOS/Linux/Windows via JUCE. Ideal
for golden-output regression: feed known input, render, compare to reference.

```python
# /// script
# dependencies = ["dawdreamer>=0.8.4", "numpy", "soundfile"]
# ///
import dawdreamer as dd
import numpy as np, soundfile as sf, sys

SR, BUFFER = 48000, 512
engine = dd.RenderEngine(SR, BUFFER)
plugin = engine.make_plugin_processor("fx", sys.argv[1])  # path to .vst3
plugin.set_parameter(0, 0.5)  # example

# 2 seconds of pink noise in:
rng = np.random.default_rng(42)
noise = rng.standard_normal((2, SR * 2)).astype(np.float32) * 0.1
player = engine.make_playback_processor("in", noise)

engine.load_graph([(player, []), (plugin, ["in"])])
engine.render(2.0)
out = engine.get_audio()
sf.write("out.wav", out.T, SR)
```

Pair with a `reference.wav` checked into `tests/fixtures/`; in CI, diff
RMS/peak/spectral features against tolerances.

### Rust-side: minimal in-process host

Options:

- **`clack-host`** (CLAP): pure-Rust CLAP host; trivial to embed in integration
  tests.
- **`vst3-sys`** / `vst3` crate: usable to call a VST3's process() directly,
  but thin — be ready to write boilerplate.
- **`baseview`**: window abstraction, not a host — don't use for rendering.

For plugin-rack, the CLAP artifact is the easiest to regression-test in-process
from Rust; VST3 conformance goes through the Steinberg validator anyway.

## 7. Gemini Code Assist Integration

### How it triggers

- Installed as the `gemini-code-assist` GitHub App on the repo/org.
- On PR open: automatically posts a **summary** comment and line-level
  **review** comments within ~5 minutes.
- Comments are authored by `gemini-code-assist[bot]`.
- Severity labels: `Critical`, `High`, `Medium`, `Low`.
- Configurable per-repo minimum severity in `.gemini/config.yaml`.

### Manual re-trigger commands (as PR comments)

```
/gemini summary           # re-generate the PR summary
/gemini review            # re-run the full code review
/gemini help              # list commands
@gemini-code-assist ...   # free-form question
```

### Config files

Drop in repo root:

```
.gemini/
  config.yaml        # severity threshold, language, ignored files
  styleguide.md      # project conventions Gemini should enforce
```

Example `.gemini/config.yaml`:

```yaml
have_fun: false
code_review:
  disable: false
  comment_severity_threshold: MEDIUM   # HIGH, MEDIUM, LOW, CRITICAL
  max_review_comments: -1              # unlimited
  pull_request_opened:
    help: false
    summary: true
    code_review: true
ignore_patterns:
  - "**/*.lock"
  - "target/**"
```

### Reading Gemini's comments from CI / Claude orchestrator

`gemini-code-assist[bot]` comments appear in two GitHub APIs:

```bash
# PR conversation (issue-style) comments — the summary + help lives here:
gh api "/repos/$OWNER/$REPO/issues/$PR/comments" \
  --jq '.[] | select(.user.login=="gemini-code-assist[bot]") | {id,body}'

# Inline review comments (line-level feedback):
gh api "/repos/$OWNER/$REPO/pulls/$PR/comments" \
  --jq '.[] | select(.user.login=="gemini-code-assist[bot]")
         | {id, path, line, body}'

# The formal review object (Approve/Request changes/Comment):
gh api "/repos/$OWNER/$REPO/pulls/$PR/reviews" \
  --jq '.[] | select(.user.login=="gemini-code-assist[bot]")'
```

### Turning comments into issues

```bash
gh api "/repos/$OWNER/$REPO/pulls/$PR/comments" \
  --jq '.[] | select(.user.login=="gemini-code-assist[bot]" and (.body|contains("Critical"))) | @json' \
| while read -r line; do
    path=$(jq -r '.path' <<<"$line")
    body=$(jq -r '.body' <<<"$line")
    gh issue create \
      --title "[gemini] $(echo "$body" | head -1 | cut -c1-80)" \
      --body  "From PR #$PR, file \`$path\`:\n\n$body" \
      --label "from-gemini,triage"
  done
```

## 8. Branch Protection & Auto-Merge

### Required checks

Set branch protection on `main`:

```bash
gh api -X PUT "/repos/$OWNER/$REPO/branches/main/protection" \
  --input - <<'JSON'
{
  "required_status_checks": {
    "strict": true,
    "contexts": [
      "build (linux-x86_64)",
      "build (macos-arm64)",
      "build (macos-x86_64)",
      "build (windows-x86_64)",
      "validate (linux-x86_64)",
      "validate (macos-arm64)",
      "validate (windows-x86_64)"
    ]
  },
  "enforce_admins": false,
  "required_pull_request_reviews": {
    "required_approving_review_count": 0,
    "require_code_owner_reviews": false
  },
  "restrictions": null,
  "allow_force_pushes": false,
  "allow_deletions": false
}
JSON
```

### Auto-merge

First enable the repo-level feature flag (GitHub UI or API), then from the
orchestrator:

```bash
gh pr merge $PR --auto --squash --delete-branch
```

`--auto` arms auto-merge; GitHub will merge once required checks pass. Known
caveats (April 2026):

- If no branch protection with required checks is configured, `--auto` errors.
- `--admin` bypasses everything — reserve for break-glass.
- A **merge queue** changes the UX: `gh pr merge --auto` adds to the queue;
  strategy is chosen by the queue config.
- Flaky case: if checks complete between arming and response, GitHub can
  return "already merged" style errors — retry with `gh pr view $PR --json state`.

### Orchestrator-safe merge loop

```bash
gh pr merge "$PR" --auto --squash --delete-branch || {
  state=$(gh pr view "$PR" --json state -q .state)
  [ "$state" = "MERGED" ] || exit 1
}
```

## 9. Versioning

### `cargo-workspaces`

Workspace-aware version bumping. Ideal for plugin-rack where
`plugin-common` + `plugin-compressor` + `plugin-reverb` share a workspace.

```bash
# Bump every crate by a semver level:
cargo workspaces version minor --all --no-git-push

# Bump only changed crates since last tag (detects via git):
cargo workspaces version --since v0.3.0

# Publish in topological order:
cargo workspaces publish --from-git
```

### `cargo-release`

Per-crate releases with git tags + (optional) crates.io publish:

```bash
cargo release minor --execute --no-publish         # bump, commit, tag
cargo release minor --execute --workspace          # entire workspace
```

Typical policy for an actively-developed plugin workspace:

- **Minor bump** per feature PR merged to main (`cargo workspaces version
  minor` in the release workflow).
- **Patch bump** for bugfix branches (`cargo release patch`).
- **Major bump** only when a plugin's parameter schema changes in a non-compat
  way (which breaks saved DAW states).
- Plugins aren't published to crates.io; they go to GitHub Releases as binary
  bundles.

### `cargo-semver-checks`

Run against any crate that **is** published (e.g. `plugin-common`):

```yaml
- uses: obi1kenobi/cargo-semver-checks-action@v2
  with:
    package: plugin-common
```

Fails the PR on breaking API changes that aren't reflected by a major bump.

## 10. Benchmarking with Criterion

Goal: assert a plugin's `process()` stays under ~1% of a 512-sample block's
wall-time budget at 48 kHz (≈ 10.6 ms).

`benches/process.rs`:

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput, BenchmarkId};

fn bench_process(c: &mut Criterion) {
    let mut group = c.benchmark_group("process");
    for block in [64, 128, 256, 512, 1024] {
        group.throughput(Throughput::Elements(block as u64));
        group.bench_with_input(BenchmarkId::from_parameter(block), &block, |b, &n| {
            let mut plugin = plugin_compressor::Compressor::default();
            plugin.initialize(2, 48000.0, n as u32);
            let mut left  = vec![0.0f32; n];
            let mut right = vec![0.0f32; n];
            b.iter(|| {
                plugin.process(black_box(&mut [&mut left, &mut right]));
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_process);
criterion_main!(benches);
```

In CI, `cargo bench --bench process -- --save-baseline pr` then compare to
`main` baseline with `critcmp` or `bencher`-style post. Avoid failing CI on
regressions below ~10% — GitHub runners are noisy. Nightly workflow on a
self-hosted runner is the reliable way to track long-term trends.

Derived assertion: for a 512-sample block at 48 kHz, the process() budget is
`512/48000 = 10.67 ms`. 1% of that is ~107 µs per call. Translate the Criterion
mean into a hard ceiling in a custom `test` target.

## 11. Realtime-Safety Checks

### `assert_no_alloc`

```rust
use assert_no_alloc::*;

#[cfg(debug_assertions)]
#[global_allocator]
static A: AllocDisabler = AllocDisabler;

impl Plugin for MyPlugin {
    fn process(&mut self, buf: &mut Buffer) -> ProcessStatus {
        assert_no_alloc(|| {
            // anything allocating in here aborts in debug or panics
            self.dsp.process(buf);
        });
        ProcessStatus::Normal
    }
}
```

Enable `warn_debug` feature during CI to get stderr warnings + stack traces
rather than aborts.

### pluginval strictness 10 catches

- Unclean state after `setProcessing(false)`.
- Non-determinism across parameter automation.
- Crashes under fuzz.
- Memory leaks via repeated instantiate/destroy cycles.

### Other

- `cargo +nightly miri test` on any DSP code without FFI (skip plugin crates
  themselves — Miri can't load JUCE/VST3 C ABI guests).
- `cargo +nightly udeps` for unused deps (perf hygiene).
- AddressSanitizer for Linux runs:
  `RUSTFLAGS="-Zsanitizer=address" cargo +nightly build -Zbuild-std --target x86_64-unknown-linux-gnu`.
- `thread-sanitizer` once before shipping each release.

## 12. macOS Code Signing & Notarization (future path)

Not urgent for dev-loop CI, but the path is:

1. Apple Developer Program membership ($99/yr).
2. Developer ID Application cert exported as `.p12`, stored as repo secret
   `MACOS_CERT_P12_B64` (base64) + `MACOS_CERT_P12_PASSWORD`.
3. `notarytool` credentials via App Store Connect API key
   (`AppStoreConnect_ApiKey.p8`) — store as `APPLE_API_KEY_B64`,
   `APPLE_API_KEY_ID`, `APPLE_API_ISSUER_ID`.
4. `altool` is retired (late 2023) — use `xcrun notarytool` exclusively.

```yaml
- name: Import signing cert
  if: startsWith(matrix.os, 'macos')
  run: |
    echo "$MACOS_CERT_P12_B64" | base64 -d > cert.p12
    security create-keychain -p actions build.keychain
    security default-keychain -s build.keychain
    security unlock-keychain -p actions build.keychain
    security import cert.p12 -k build.keychain -P "$MACOS_CERT_P12_PASSWORD" -T /usr/bin/codesign
    security set-key-partition-list -S apple-tool:,apple: -s -k actions build.keychain

- name: Sign VST3
  run: |
    codesign --force --deep --options runtime \
      --sign "Developer ID Application: Plugin Rack LLC (TEAMID)" \
      target/bundled/MyPlugin.vst3

- name: Notarize
  run: |
    ditto -c -k --keepParent target/bundled/MyPlugin.vst3 MyPlugin.zip
    xcrun notarytool submit MyPlugin.zip \
      --key AppStoreConnect_ApiKey.p8 \
      --key-id "$APPLE_API_KEY_ID" \
      --issuer "$APPLE_API_ISSUER_ID" \
      --wait
    xcrun stapler staple target/bundled/MyPlugin.vst3
```

Melatonin's blog post "How to code sign and notarize macOS audio plugins in
CI" is the canonical reference; mirror its structure.

## 13. Python + `uv` Orchestration

`uv` (Astral) is the standard 2025/2026 Python package/runner. Use it for
orchestration scripts that call `cargo`, `gh`, `pluginval`, `dawdreamer`, etc.

### Script shebang pattern (PEP 723)

```python
#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = [
#   "dawdreamer>=0.8.4",
#   "numpy",
#   "soundfile",
#   "click",
# ]
# ///
"""Golden-output smoke test for a VST3 via DawDreamer."""
import subprocess, sys, pathlib, click, numpy as np, soundfile as sf
import dawdreamer as dd

@click.command()
@click.argument("plugin_path", type=click.Path(exists=True))
@click.option("--reference", type=click.Path(exists=True), required=True)
@click.option("--tolerance-db", default=-60.0)
def main(plugin_path, reference, tolerance_db):
    SR, BUF = 48000, 512
    eng = dd.RenderEngine(SR, BUF)
    fx = eng.make_plugin_processor("fx", plugin_path)
    rng = np.random.default_rng(42)
    noise = (rng.standard_normal((2, SR * 2)).astype(np.float32) * 0.1)
    src = eng.make_playback_processor("src", noise)
    eng.load_graph([(src, []), (fx, ["src"])])
    eng.render(2.0)
    got = eng.get_audio()
    ref, _ = sf.read(reference, always_2d=True); ref = ref.T.astype(np.float32)
    err = got - ref[:, :got.shape[1]]
    rms = 20 * np.log10(np.sqrt(np.mean(err**2)) + 1e-12)
    print(f"error rms: {rms:.2f} dB (tolerance {tolerance_db})")
    sys.exit(0 if rms < tolerance_db else 1)

if __name__ == "__main__":
    main()
```

Invoke directly as `./scripts/smoke_test.py path/to/MyPlugin.vst3
--reference tests/fixtures/ref.wav` — `uv` handles the venv + deps invisibly.

### Orchestrator script: full CI-style verify locally

```python
#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["click", "rich"]
# ///
"""Run the full verify pipeline locally, matching CI."""
import subprocess, sys, shutil, pathlib, click
from rich.console import Console

C = Console()

def run(cmd, **kw):
    C.print(f"[bold cyan]$ {' '.join(cmd)}[/]")
    return subprocess.run(cmd, check=True, **kw)

@click.command()
@click.option("--plugin", default="my_compressor")
@click.option("--skip-bench", is_flag=True)
def main(plugin, skip_bench):
    run(["cargo", "fmt", "--all", "--check"])
    run(["cargo", "clippy", "--all-targets", "--", "-D", "warnings"])
    run(["cargo", "test", "--workspace"])
    run(["cargo", "xtask", "bundle", plugin, "--release"])

    bundle = pathlib.Path(f"target/bundled/{plugin}.vst3")
    if not bundle.exists():
        C.print(f"[red]bundle not found: {bundle}[/]"); sys.exit(1)

    pluginval = shutil.which("pluginval") or "./tools/pluginval"
    args = [pluginval, "--strictness-level", "10",
            "--validate-in-process", "--verbose",
            "--validate", str(bundle)]
    if sys.platform.startswith("linux"):
        args = ["xvfb-run", "-a"] + args + ["--skip-gui-tests"]
    run(args)

    run(["./scripts/smoke_test.py", str(bundle),
         "--reference", f"tests/fixtures/{plugin}.ref.wav"])

    if not skip_bench:
        run(["cargo", "bench", "--bench", "process"])

    C.print("[bold green]all green[/]")

if __name__ == "__main__":
    main()
```

### PR babysitter (orchestrator helper)

```python
#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["click"]
# ///
"""Watch a PR; when CI fails, dump the failed logs for Claude to read."""
import subprocess, sys, time, json, click

def sh(args): return subprocess.run(args, check=True, capture_output=True, text=True).stdout

@click.command()
@click.argument("pr", type=int)
@click.option("--interval", default=20)
def main(pr, interval):
    while True:
        status = json.loads(sh(["gh", "pr", "view", str(pr),
                                "--json", "statusCheckRollup,state"]))
        if status["state"] == "MERGED":
            print("merged"); sys.exit(0)
        checks = status["statusCheckRollup"]
        pending = [c for c in checks if c.get("status") == "IN_PROGRESS"]
        failed  = [c for c in checks if c.get("conclusion") == "FAILURE"]
        if failed:
            for f in failed:
                print(f"FAIL: {f['name']} — {f['detailsUrl']}")
                run_id = f["detailsUrl"].rsplit("/", 1)[-1]
                logs = sh(["gh", "run", "view", run_id, "--log-failed"])
                print(logs[-4000:])   # tail
            sys.exit(1)
        if not pending:
            print("all checks passed; trying auto-merge")
            subprocess.run(["gh", "pr", "merge", str(pr),
                            "--auto", "--squash", "--delete-branch"])
            sys.exit(0)
        time.sleep(interval)

if __name__ == "__main__":
    main()
```

## 14. Failure Recovery for the Claude Orchestrator

Pattern when CI fails on a PR the orchestrator opened:

1. **Detect failure**:
   ```bash
   gh pr checks "$PR" --watch --fail-fast
   # exits non-zero on first failure
   ```
2. **Enumerate failed jobs**:
   ```bash
   run_id=$(gh pr view "$PR" --json statusCheckRollup \
              --jq '.statusCheckRollup[] | select(.conclusion=="FAILURE") | .detailsUrl' \
              | head -1 | awk -F/ '{print $NF}')
   ```
3. **Pull only failed logs** (tighter context than full logs):
   ```bash
   gh run view "$run_id" --log-failed > /tmp/ci-fail.log
   ```
4. **Extract signal** — grep for `error:`, `FAILED`, `panicked at`,
   `pluginval`, `validator:`:
   ```bash
   grep -E '(^error|FAILED|panicked at|pluginval.*:|validator:)' /tmp/ci-fail.log | head -100
   ```
5. **Delegate to subagent** with scoped context: pass the failing job name,
   file:line from the error, and the 100-line window around it. Do not dump
   the full log (wastes tokens).
6. **Subagent** proposes a diff, orchestrator commits it to the PR branch and
   pushes. Loop.

Known gotcha: `gh run view --log` on large workflow logs can truncate or
error. Prefer `--log-failed`, or fall back to the raw download:

```bash
gh api "/repos/$OWNER/$REPO/actions/runs/$run_id/logs" > logs.zip
unzip -p logs.zip '*/0_*.txt'   # step-by-step
```

## Proposed CI Pipeline

### Workflow graph

```
┌──────────────────────────┐
│  on: pull_request        │
└──────────┬───────────────┘
           │
    ┌──────┴──────┐
    │             │
┌───▼────┐   ┌────▼─────┐   ┌─────────────┐
│  lint  │   │  build   │   │   gemini    │ (auto, app-driven)
│ fmt +  │   │  matrix  │   │  reviews    │
│clippy  │   │ 4 OS/arch│   │  via bot    │
└───┬────┘   └────┬─────┘   └──────┬──────┘
    │             │                │
    │        ┌────▼─────┐          │
    │        │ validate │          │
    │        │ steinberg│          │
    │        │ + plugin │          │
    │        │   val 10 │          │
    │        └────┬─────┘          │
    │             │                │
    │        ┌────▼─────┐          │
    │        │  golden  │          │
    │        │  output  │          │
    │        │ dawdream │          │
    │        └────┬─────┘          │
    │             │                │
    └──────┬──────┴────────────────┘
           │
     ┌─────▼─────┐
     │ auto-merge│
     │  if green │
     └───────────┘
```

### Full example workflow

`.github/workflows/ci.yml`:

```yaml
name: ci

on:
  pull_request:
  push:
    branches: [main]

concurrency:
  group: ci-${{ github.ref }}
  cancel-in-progress: true

env:
  CARGO_TERM_COLOR: always
  RUST_BACKTRACE: 1
  CARGO_INCREMENTAL: 0   # not useful in CI and bloats cache

jobs:
  lint:
    runs-on: ubuntu-24.04
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@nightly
        with:
          toolchain: nightly-2026-03-15
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
        with: { shared-key: lint }
      - run: cargo fmt --all --check
      - run: cargo clippy --all-targets --workspace -- -D warnings

  build:
    strategy:
      fail-fast: false
      matrix:
        include:
          - { name: linux-x86_64,   os: ubuntu-24.04,  target: x86_64-unknown-linux-gnu,  cross: '' }
          - { name: macos-arm64,    os: macos-15,      target: aarch64-apple-darwin,      cross: '' }
          - { name: macos-x86_64,   os: macos-15,      target: x86_64-apple-darwin,       cross: 'x86_64-apple-darwin' }
          - { name: windows-x86_64, os: windows-2025,  target: x86_64-pc-windows-msvc,    cross: '' }
    name: build (${{ matrix.name }})
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4

      - name: Install Linux deps
        if: startsWith(matrix.os, 'ubuntu')
        run: |
          sudo apt-get update
          sudo apt-get install -y \
            libasound2-dev libgl-dev libjack-jackd2-dev \
            libx11-xcb-dev libxcb1-dev libxcb-dri2-0-dev \
            libxcb-icccm4-dev libxcursor-dev libxkbcommon-dev \
            libxcb-shape0-dev libxcb-xfixes0-dev \
            libfreetype-dev libfontconfig1-dev xvfb

      - uses: dtolnay/rust-toolchain@nightly
        with:
          toolchain: nightly-2026-03-15
          targets: ${{ matrix.target }}

      - uses: Swatinem/rust-cache@v2
        with:
          shared-key: build-${{ matrix.target }}
          cache-on-failure: true
          cache-targets: ${{ !startsWith(matrix.os, 'windows') }}
          save-if: ${{ github.ref == 'refs/heads/main' }}

      - name: Bundle plugins
        shell: bash
        run: |
          pkgs=$(cargo xtask known-packages)
          args=()
          for p in $pkgs; do args+=("$p"); done
          if [[ "${{ matrix.name }}" == "macos-x86_64" ]]; then
            cargo xtask bundle-universal "${args[@]}" --release
          elif [[ -n "${{ matrix.cross }}" ]]; then
            cargo xtask bundle "${args[@]}" --release --target "${{ matrix.cross }}"
          else
            cargo xtask bundle "${args[@]}" --release
          fi

      - uses: actions/upload-artifact@v4
        with:
          name: bundles-${{ matrix.name }}
          path: target/bundled/

  validate:
    needs: build
    strategy:
      fail-fast: false
      matrix:
        include:
          - { name: linux-x86_64,   os: ubuntu-24.04 }
          - { name: macos-arm64,    os: macos-15 }
          - { name: windows-x86_64, os: windows-2025 }
    name: validate (${{ matrix.name }})
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: actions/download-artifact@v4
        with:
          name: bundles-${{ matrix.name }}
          path: target/bundled/

      - name: Install Linux runtime deps
        if: startsWith(matrix.os, 'ubuntu')
        run: |
          sudo apt-get update
          sudo apt-get install -y xvfb libasound2t64 \
            libfreetype6 libfontconfig1 libgl1 libx11-6 libxcursor1 libxcb1

      # --- pluginval ---
      - name: Install pluginval (Linux)
        if: startsWith(matrix.os, 'ubuntu')
        run: |
          curl -L -o pv.zip https://github.com/Tracktion/pluginval/releases/latest/download/pluginval_Linux.zip
          unzip -q pv.zip && echo "$PWD" >> $GITHUB_PATH

      - name: Install pluginval (macOS)
        if: startsWith(matrix.os, 'macos')
        run: |
          curl -L -o pv.zip https://github.com/Tracktion/pluginval/releases/latest/download/pluginval_macOS.zip
          unzip -q pv.zip
          echo "$PWD/pluginval.app/Contents/MacOS" >> $GITHUB_PATH

      - name: Install pluginval (Windows)
        if: startsWith(matrix.os, 'windows')
        shell: pwsh
        run: |
          Invoke-WebRequest https://github.com/Tracktion/pluginval/releases/latest/download/pluginval_Windows.zip -OutFile pv.zip
          Expand-Archive pv.zip -DestinationPath .
          echo "$PWD" >> $env:GITHUB_PATH

      - name: Run pluginval on every VST3
        shell: bash
        run: |
          set -eo pipefail
          shopt -s nullglob
          for bundle in target/bundled/*.vst3; do
            echo "::group::pluginval $bundle"
            if [[ "$RUNNER_OS" == "Linux" ]]; then
              xvfb-run -a pluginval --strictness-level 10 \
                --validate-in-process --skip-gui-tests --verbose \
                --validate "$bundle"
            else
              pluginval --strictness-level 10 \
                --validate-in-process --verbose \
                --validate "$bundle"
            fi
            echo "::endgroup::"
          done

      # --- Steinberg validator ---
      - name: Cache Steinberg validator
        id: vs
        uses: actions/cache@v4
        with:
          path: tools/validator*
          key: validator-${{ runner.os }}-${{ runner.arch }}-vst3sdk-3.7.12

      - name: Build Steinberg validator
        if: steps.vs.outputs.cache-hit != 'true'
        shell: bash
        run: |
          git clone --recursive --depth 1 --branch v3.7.12_build_20 \
            https://github.com/steinbergmedia/vst3sdk.git
          cmake -S vst3sdk -B vst3sdk/build -DCMAKE_BUILD_TYPE=Release
          cmake --build vst3sdk/build --target validator --config Release -j
          mkdir -p tools
          cp vst3sdk/build/bin/Release/validator* tools/ 2>/dev/null || \
            cp vst3sdk/build/bin/validator* tools/

      - name: Run Steinberg validator
        shell: bash
        run: |
          for bundle in target/bundled/*.vst3; do
            ./tools/validator -e "$bundle"
          done

  golden:
    needs: build
    runs-on: ubuntu-24.04
    steps:
      - uses: actions/checkout@v4
      - uses: actions/download-artifact@v4
        with: { name: bundles-linux-x86_64, path: target/bundled/ }
      - uses: astral-sh/setup-uv@v5
      - run: |
          for p in $(cargo xtask known-packages); do
            ./scripts/smoke_test.py "target/bundled/${p}.vst3" \
              --reference "tests/fixtures/${p}.ref.wav"
          done

  semver:
    if: github.event_name == 'pull_request'
    runs-on: ubuntu-24.04
    steps:
      - uses: actions/checkout@v4
      - uses: obi1kenobi/cargo-semver-checks-action@v2
        with: { package: plugin-common }
```

### Auto-merge workflow

`.github/workflows/automerge.yml`:

```yaml
name: automerge

on:
  pull_request:
    types: [opened, ready_for_review, labeled]

jobs:
  arm:
    if: contains(github.event.pull_request.labels.*.name, 'orchestrator')
    runs-on: ubuntu-24.04
    permissions: { pull-requests: write, contents: write }
    steps:
      - run: gh pr merge ${{ github.event.pull_request.number }} --auto --squash --delete-branch
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```

### Nightly heavier checks

`.github/workflows/nightly.yml`:

```yaml
name: nightly
on:
  schedule: [ { cron: '0 7 * * *' } ]
  workflow_dispatch:

jobs:
  bench:
    runs-on: ubuntu-24.04
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@nightly
      - uses: Swatinem/rust-cache@v2
      - run: cargo bench --bench process -- --save-baseline nightly

  reaper-smoke:
    runs-on: ubuntu-24.04
    steps:
      - uses: actions/checkout@v4
      - run: |
          sudo apt-get update && sudo apt-get install -y xvfb reaper || \
            curl -L https://www.reaper.fm/files/7.x/reaper712_linux_x86_64.tar.xz | tar -xJ
          xvfb-run -a ./reaper/reaper -renderproject tests/smoke.rpp
```

## Gotchas Checklist

- **nih-plug requires nightly.** Pin the date; update in its own PR.
- **Windows `target/` caching is flaky** — disable it in `rust-cache`.
- **Linux pluginval** segfaults without `--skip-gui-tests` or Xvfb.
- **Apple Silicon runners are the default now**; Intel macOS is paid tier.
- **`altool` is dead** — use `xcrun notarytool`.
- **`gh pr merge --auto`** requires branch protection with required checks,
  or it errors.
- **`gh run view --log`** truncates huge logs; prefer `--log-failed`.
- **`gemini-code-assist[bot]`** comments split across
  `/pulls/:n/comments` (inline) and `/issues/:n/comments` (summary).
- **VST3 on macOS** is a bundle dir, not a file — use `ditto` not `zip` for
  notarization archives.
- **DawDreamer is GPLv3** — fine for internal testing; don't ship it.
- **cargo-workspaces `--since`** requires git tags to exist; bootstrap with an
  initial `v0.0.0` tag.

## Key References

- nih-plug repo & CI: https://github.com/robbert-vdh/nih-plug
- nih_plug_xtask docs: https://nih-plug.robbertvanderhelm.nl/nih_plug_xtask/
- pluginval: https://github.com/Tracktion/pluginval
- vst3sdk / validator: https://github.com/steinbergmedia/vst3sdk
- Swatinem/rust-cache: https://github.com/Swatinem/rust-cache
- cargo-semver-checks: https://github.com/obi1kenobi/cargo-semver-checks
- cargo-workspaces: https://crates.io/crates/cargo-workspaces
- cargo-release: https://github.com/crate-ci/cargo-release
- assert_no_alloc: https://github.com/Windfisch/rust-assert-no-alloc
- Gemini Code Assist docs: https://developers.google.com/gemini-code-assist/docs/use-code-assist-github
- DawDreamer: https://github.com/DBraun/DawDreamer
- Carla: https://github.com/falkTX/Carla
- REAPER CLI doc: https://github.com/ReaTeam/Doc/blob/master/REAPER-CLI.md
- Melatonin macOS signing: https://melatonin.dev/blog/how-to-code-sign-and-notarize-macos-audio-plugins-in-ci/
- uv scripts / PEP 723: https://docs.astral.sh/uv/guides/scripts/
- gh pr merge: https://cli.github.com/manual/gh_pr_merge
