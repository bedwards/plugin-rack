# issue #18 — clap-validator integration

Journal for wiring `free-audio/clap-validator` into CI + the `pluginrack` CLI
as the CLAP-side counterpart to pluginval (which is VST3-only).

## Version pinned

**`0.3.2`** — latest stable release at time of writing (2026-04-17). Source:
`gh release list --repo free-audio/clap-validator`. The tool has not cut a new
release since 2023-03-25, which is fine: the CLAP spec itself has been stable
and the validator tracks that spec. Pin is explicit (not `latest`) for CI
reproducibility per CLAUDE.md conventions.

## Release asset URLs (all platforms)

```
macOS universal: https://github.com/free-audio/clap-validator/releases/download/0.3.2/clap-validator-0.3.2-macos-universal.tar.gz
Linux:           https://github.com/free-audio/clap-validator/releases/download/0.3.2/clap-validator-0.3.2-ubuntu-18.04.tar.gz
Windows:         https://github.com/free-audio/clap-validator/releases/download/0.3.2/clap-validator-0.3.2-windows.zip
```

macOS + Linux are `tar.gz` (single top-level directory → `--strip-components=1`
when extracting). Windows is `.zip`. The extracted binary is at
`./clap-validator/clap-validator[.exe]`.

## Flags chosen

CLI shape: `clap-validator validate [--only-failed] <path-to-.clap>`.

- `--only-failed`: hide successful + skipped tests, keep logs concise. Used in
  PR/push CI.
- Nightly drops `--only-failed` so the full report lands in logs.
- No GUI/process test skip required. clap-validator is pure-Rust, headless, and
  does not try to open a window — unlike pluginval, whose editor test aborts on
  headless Linux. (The issue briefing mentioned `--skip-test "process|gui"` as
  a possible need; in practice it is not required — `--help` shows the tool
  runs pure-API conformance checks and does not poke a window. If a future
  version adds a GUI test we can add `--test-filter '!gui'` or equivalent.)

## Exit behavior

Non-zero exit on any test failure (spec compliance, param round-trip, etc.) or
missing binary. CI fails loudly with the runner-surfaced exit code.

## Where the steps live

- `.github/workflows/ci.yml`: `clap_validator_url` + `clap_validator_archive` +
  `clap_validator_bin` per-runner matrix entries; "Install clap-validator"
  step; "clap-validator (CLAP)" step after the existing pluginval (VST3) step.
  Sits side-by-side with the unchanged pluginval step.
- `.github/workflows/nightly.yml`: downloads macOS universal tarball; runs the
  full suite (no `--only-failed`).

## CLI addition

`pluginrack verify clap-validator` mirrors the CI step. Discovery order:
`$PATH` → `tools/clap-validator[.exe]` → `./clap-validator/clap-validator[.exe]`.
Unknown extra args pass through via `wrapper_args` after `--`, matching the
project's CLI shape convention.

Bare `pluginrack verify` now runs: `lint` → `unit` → `bundle` → `pluginval`
(VST3) → `clap-validator` (CLAP). pluginval defaults to VST3 format now that
we have a proper CLAP path.

## Local smoke tests

Ran on macOS (darwin 25.2.0, aarch64) from the feature branch:

- `cargo fmt --all --check`: pass.
- `cargo clippy --workspace --all-targets -- -D warnings`: pass.
- `cd pluginrack && uv sync && uv run ruff check src`: pass.
- `uv run pluginrack --help`: pass, shows new `verify` group.
- `uv run pluginrack verify --help`: pass, shows `clap-validator` subcommand.
- `uv run pluginrack verify clap-validator --help`: pass, documents
  `--only-failed/--all-output` and the wrapper-args pass-through.

No local `clap-validator` binary was installed, so end-to-end smoke of the
CLI-driven validation relies on CI to exercise the download + run path.

## Version bump

`pluginrack/pyproject.toml` and `pluginrack/src/pluginrack/__init__.py`:
`0.1.0` → `0.2.0` (minor bump per the new subcommand).
