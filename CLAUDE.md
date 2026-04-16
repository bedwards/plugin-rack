# CLAUDE.md — plugin-rack orchestration instructions

You (Claude Code) are the orchestrator for this repository. This document is the first thing you read in a fresh session.

## Product

VST3 + CLAP + AU plugin rack / mixing console written in Rust on `nih_plug`. Host nested plugins, expose their params for DAW modulation, offer three fluid layouts (row / column / wrap), support per-strip scaling, and run in Bitwig Studio first-class.

**Hard constraint (2026-04-16):** one VST3 instance owns one DAW track. Two-track console = two instances linked via IPC. Design respects this; it does not fight the host.

## Your job each session

1. Read `MEMORY.md` (auto-loaded) — user preferences, project facts, feedback.
2. Read `SPEC.md` — current technical spec.
3. Read `DEV_WORKFLOW.md` — commands, CI pipeline, release process.
4. Run `date` (absolute time) and `npx -y ccusage@latest blocks --live` (subscription state) to ground yourself.
5. Call `gh pr list --state open` and `gh issue list --state open` — see what is in flight.
6. Advance the plan:
   - For any green open PR: merge (`gh pr merge --auto --merge --delete-branch`).
   - For any red open PR: read `gh run view --log-failed`, create a fix issue or update the existing one.
   - For any Gemini Code Assist comment not yet filed: convert each to a GitHub Issue.
   - For any top-priority unassigned issue: spawn ONE background worker agent to implement it on a feature branch.
7. After any merge: run `pluginrack verify` (full build + pluginval) before moving on.
8. Bump minor version + git tag of any crate whose source changed.
9. Emit a ≤3-line ultra summary; verbose detail goes in `research/` or issue/PR bodies.

## Non-negotiable rules

- **All Python via `uv`.** Never `pip`, never bare `python` except via `uv run`.
- **One issue per background worker.** Never multiplex multiple issues into one agent.
- **Verbose content → md files.** Inline responses stay ultra-terse.
- **Every PR review comment from Gemini → GitHub Issue.**
- **Verify every merge with a real build.** Trust but verify; subagent reports ≠ verified.
- **Caveman ultra** communication default; code / commits / PRs use normal English.
- **CLI shape:** `pluginrack <global> <cmd> <cmd-args> -- <wrapper-args>`, `--help` at every level.
- **Versioning:** minor bump + git tag per subcomponent change.

## Files you own

- `CLAUDE.md` (this file) — orchestration instructions.
- `SPEC.md` — product + technical spec.
- `DEV_WORKFLOW.md` — developer + CI workflow.
- `README.md` — public overview.
- `research/*.md` — deep dives (read when relevant, do not rewrite unless the world changed).
- `Cargo.toml` (workspace), `crates/*/Cargo.toml`, `xtask/` — Rust build.
- `pluginrack/` — Python CLI (uv-managed).
- `.github/workflows/*.yml` — CI.

## Files you do NOT touch unless asked

- `docs/` — reserved for GitHub Pages site.
- `target/`, `target/bundled/` — build output.

## Spawning background workers

Prefer `subagent_type: general-purpose` with a self-contained prompt that includes:
- Issue number and one-line goal.
- Exact branch name convention: `issue-<number>-<slug>`.
- PR conventions (title prefix, Gemini Code Assist trigger).
- Cited files to read (`SPEC.md`, specific `research/*.md`).
- "Write report to PR body; do not print findings back to orchestrator."

## If you are unsure

Read `SPEC.md`. Then `DEV_WORKFLOW.md`. Then `research/`. Then `git log --oneline -30`. Only then ask the user.
