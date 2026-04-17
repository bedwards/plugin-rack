# DEV_WORKFLOW.md — plugin-rack

Pinned tooling + concrete commands. Verified on `darwin 25.2.0` (Apple Silicon), 2026-04-16.

## Toolchain

| Tool | Version | Source |
|------|---------|--------|
| rustc | 1.94.0 stable | rust-toolchain.toml pins it |
| cargo | 1.94.0 | ships with rustc |
| nih_plug | pinned to `BillyDM/nih-plug@3e0c4ac0430bffd5526b7325b8362369986d24af` | workspace Cargo.toml |
| uv | ≥ 0.10 | already on user's PATH |
| gh | ≥ 2.89 | already on user's PATH |
| pluginval | latest release (v1.0.x) | downloaded to `tools/` |
| Steinberg `validator` | built from `vst3sdk` 3.8.0 | fetched by CI |

## Core commands (memorize)

```
cargo check --workspace                           # 10-15s clean
cargo xtask bundle rack-plugin --release          # → target/bundled/rack-plugin.{vst3,clap}
cargo xtask bundle-universal rack-plugin --release  # macOS universal
cargo xtask known-packages                        # list bundlable crates

# Install locally (macOS)
ln -sfn "$(pwd)/target/bundled/rack-plugin.vst3" ~/Library/Audio/Plug-Ins/VST3/rack-plugin.vst3
ln -sfn "$(pwd)/target/bundled/rack-plugin.clap" ~/Library/Audio/Plug-Ins/CLAP/rack-plugin.clap

# Python CLI
cd pluginrack && uv sync
uv run pluginrack --help
uv run pluginrack status
uv run pluginrack verify
uv run pluginrack verify pluginval --format vst3
uv run pluginrack verify clap-validator
uv run pluginrack pr automerge
```

## Verify tiers (each a subcommand)

| Tier | Subcommand | What it does | Blocking |
|------|------------|--------------|----------|
| 1 | `verify lint` | `cargo fmt --check` + `cargo clippy -D warnings` | yes |
| 2 | `verify unit` | `cargo test --workspace` | yes |
| 3 | `verify bundle` | `cargo xtask bundle rack-plugin --release` | yes |
| 4a | `verify pluginval` | `pluginval --strictness-level 10 --validate-in-process` on the VST3 bundle | yes |
| 4b | `verify clap-validator` | `clap-validator validate --only-failed` on the CLAP bundle (pluginval is VST3-only) | yes |
| 5a | `verify render` | offline bit-identical passthrough render via dawdreamer — `scripts/verify_render.py` asserts INPUT == OUTPUT for the 2 s 48 kHz stereo test signal; loosen the assertion when rack-plugin gains real DSP | nightly |
| 5c | `verify bitwig-mod` | offline Bitwig-style harness: renders VST3 at block sizes {64, 511, 1024} and ramps macros 0/63/127 to prove variable-block tolerance + modulatable params (dawdreamer; requires `--extra verify`) | nightly |
| 6 | `verify rt-safety` | `cargo test --features assert_process_allocs` on audio path | nightly |

Bare `pluginrack verify` runs tiers 1–4 (lint, unit, bundle, pluginval, clap-validator).

## CI (`.github/workflows/`)

- `ci.yml` — PR / push: matrix over `macos-15`, `ubuntu-24.04`, `windows-2025`. Fmt, clippy, test, bundle, pluginval (VST3, strictness 5), clap-validator (CLAP, pinned 0.3.2, `--only-failed`). Plus a Python job that lints + help-tests the CLI.
- `automerge.yml` — on PR ready / check_suite complete: `gh pr merge --auto --squash --delete-branch`.
- `nightly.yml` — 07:17 UTC: bundle-universal on macOS + pluginval strictness 10 (VST3) + clap-validator full-suite (CLAP) + `verify-render` (bit-identical passthrough) + `verify-bitwig-mod` (variable-block + macro ramp).

Branch protection (set via `gh api`):
```
gh api -X PUT /repos/:owner/:repo/branches/main/protection --input protection.json
```
Requires: `rust / macos-15`, `rust / ubuntu-24.04`, `rust / windows-2025`, `python / pluginrack CLI` + 1 review.

## Gemini Code Assist

- GitHub App. Comments as `gemini-code-assist[bot]`.
- Mirror its review comments to issues:
  ```
  uv run pluginrack issue mirror <PR#>
  ```

## Release

```
uv run pluginrack release bump rack-core         # writes Cargo.toml, commits, tags
git push --follow-tags origin main               # tag push triggers release.yml (TODO)
```

Tag format: `<crate>-v<major.minor.patch>`. Minor bump per change until 1.0.

## Daily orchestrator loop

```
date
uv run pluginrack status                         # PRs, issues, CI, subscription
uv run pluginrack pr automerge --dry-run         # preview
uv run pluginrack pr automerge                   # actually merge green PRs
uv run pluginrack verify                         # post-merge sanity
uv run pluginrack issue mirror <PR#>             # for any PR with Gemini comments not yet mirrored
```

## Fresh session pickup checklist

1. Read `CLAUDE.md` (instructions).
2. Read `MEMORY.md` (auto-loaded).
3. Read `SPEC.md` (current decisions).
4. Run `date`, `uv run pluginrack status`.
5. Work top-priority unblocked GH issue.
