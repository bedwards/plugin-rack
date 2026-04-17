# NEXT_SESSION_PROMPT.md

## Priority shift: ship user-visible features

The plugin loads in Bitwig and does nothing. That is the current state. Change it.

**New default:** every issue worked must produce something the human can load in Bitwig, hear, or see. No more scaffolding for its own sake.

## What "user-visible" means

- Brian can load the plugin in Bitwig and observe the change (audio, GUI, params)
- A feature is done when it has been tested end-to-end in the most realistic scenario possible: load the actual bundle, drive it with real audio, assert real output. Use dawdreamer, pluginval, clap-validator, scripted host loops, or any other automated harness before asking Brian to touch it. CI green is a floor, not a ceiling.

## Concrete priority order

**Brian has VST/VST3 plugins, not CLAP. Prioritize VST3 hosting.**

1. **Issue #9 — VST3 guest hosting** (`rack-host-vst3`)
   - Load a `.vst3` bundle → audio passes through the guest → Brian hears it
   - This is the core product. Everything else is dressing.

2. **Issue #7 — 128 macro params** (`rack-core` + `rack-plugin`)
   - 128 parameters appear in Bitwig's modulator/automation lane
   - Brian can map a Bitwig modulator to a param and see it move

3. **Issue #8 — Layout engine GUI** (`rack-gui`)
   - vizia UI appears when plugin is opened in Bitwig
   - Shows strip slots (even if empty); row/col/wrap toggle works

4. **Issue #6 — Guest editor embed** (depends on #9 + #8)
   - Guest plugin's native GUI opens inside the rack window

5. **Issue #5 — CLAP guest hosting** (`rack-host-clap`)
   - Lower priority — Brian's plugins are VST/VST3

## What to deprioritize

- Issue #4 (vst3-sys/GPL) — closed, open source project, GPL fine
- Issue #15 (branch protection) — CI/admin, not user-visible
- Issue #16 (pytest CLI) — tooling, not user-visible
- Issue #17 (dawdreamer render) — nightly/CI, not user-visible
- Issue #18 (clap-validator) — CI, not user-visible

## Session startup rule (replaces §6 in CLAUDE.md for now)

After merging green PRs: spawn ONE worker on the next unfinished item from the priority list above. Do not touch infra/CI issues unless they are blocking a feature.
