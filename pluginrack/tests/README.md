# pluginrack tests

Fast, hermetic CLI smoke suite. No real subprocesses (cargo, gh, uv, npx,
dawdreamer) and no network — every external call is monkey-patched.

## Run locally

```bash
cd pluginrack
uv sync --all-extras --group dev
uv run pytest -q
```

Typical run is under one second.

## Layout

- `conftest.py` — shared fixtures: `runner` (Click `CliRunner`) and
  `no_subprocess` (records `sh.run` / `subprocess.run` calls instead of
  spawning them).
- `test_cli_help.py` — smokes `--help` on every group + subcommand.
- `test_verify.py` — `verify` subgroup: help text and (mocked) cargo wiring.
- `test_status.py` — `status` snapshot with mocked `gh` / `npx`.
- `test_pr.py` — `pr automerge` dry-run using a fixture payload.
- `test_issue.py` — `issue mirror` dry-run using a fixture Gemini comment.

## Adding tests

- Prefer `click.testing.CliRunner` over subprocess invocations.
- If a command shells out, use the `no_subprocess` fixture or stub
  `pluginrack.sh.run` / `subprocess.run` directly via `monkeypatch`.
- Keep each test under ~100 ms; the whole suite should stay well under a
  second so it fits into the CI python job without pain.
