#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""File v1 issues for plugin-rack. Idempotent on title match."""
from __future__ import annotations

import json
import subprocess
import sys

ISSUES: list[tuple[str, str, str]] = [
    (
        "VST3 guest hosting via vst3 crate",
        "v1,p1,host,feature",
        """Implement rack-host-vst3 using the `vst3` crate (coupler-rs/vst3-rs). Cover: plugin discovery (scanning the user's VST3 dir), bundle load, class enumeration, IComponent/IAudioProcessor/IEditController instantiation, setupProcessing, process(), state get/set.

Pair with `free-audio/clap-wrapper` for cross-format reach.

**Acceptance:**
- [ ] can load a .vst3 bundle, see bus configuration
- [ ] process() passes audio unchanged
- [ ] state round-trips through getState/setState
- [ ] unit tests against a known VST3 plugin

Source: research/hosting.md §VST3 hosting in Rust; research/vst3_spec.md §Audio buses.
""",
    ),
    (
        "Per-strip scaling of nested plugin GUIs",
        "v1,p2,gui,feature",
        """Each strip exposes a scale slider (0.5x-2x) that scales the embedded guest plugin editor independently of the rack window scale.

Uses IPlugViewContentScaleSupport (VST3) / CLAP gui scale extension. On guests that do not support scale, fall back to a software proxy (separate issue).

**Acceptance:**
- [ ] scale change takes effect without editor reinstantiation where possible
- [ ] layout reflows around new strip size
- [ ] scale persists per-strip in plugin state

Source: research/gui.md §Per-plugin-strip scaling.
""",
    ),
    (
        "State persistence: rack + all guest states",
        "v1,p1,feature",
        """Save and restore rack state (layout mode, macro bindings, strip order) plus every guest plugin's own state blob via its own getState/setState.

All data flows through nih_plug's Persist / serialize hooks. Store guest chunks as Vec<u8> fields.

**Acceptance:**
- [ ] save, close DAW, reopen — rack returns to exact same state
- [ ] guest state blobs round-trip byte-for-byte
- [ ] unit test simulating save/load cycle with a mock guest

Source: research/vst3_spec.md §Preset (state) management.
""",
    ),
    (
        "Inter-instance IPC via shared memory",
        "v1,p1,ipc,feature",
        """Implement rack-ipc: instances with the same user-set `link_tag` discover each other via a POSIX/Win32 shared-memory segment keyed by that tag. Each instance publishes its strip state at 30 Hz; console view reads all siblings.

**Design pillars (research/ipc.md §Recommended architecture):**

- memmap2 + rtrb SPSC ring for GUI-rate state sync
- PID registry + 2s heartbeat TTL for sibling liveness
- link_tag persisted in plugin state so siblings rediscover across DAW reloads
- no audio-rate cross-instance data (sub-block latency is impossible, documented)

**Acceptance:**
- [ ] two rack instances with the same link_tag discover each other
- [ ] one dying (PID gone) is dropped from registry within 4 seconds
- [ ] macOS App Sandbox naming rules respected
- [ ] no allocations on the publish path

Source: research/ipc.md.
""",
    ),
    (
        "Console-view: render sibling strips inside any instance",
        "v1,p2,gui,ipc,feature",
        """When an instance has linked siblings, its GUI can switch to Console view: render its own strip alongside sibling strips (read-only for guest editors of siblings; sibling audio path stays on its own track).

**Acceptance:**
- [ ] toggle Local / Console view
- [ ] sibling meters, fader positions, mute/solo-like flags visible in real time
- [ ] clicking a sibling slot opens the sibling's editor if that plugin is the owner (deep-link across OS window z-order)

Source: research/ipc.md §GUI linking; research/prior_art.md §UAD Console / Console 1.
""",
    ),
    (
        "Bitwig buffer / modulation verification harness",
        "v1,p1,ci,audio",
        """Prove the rack handles Bitwig's actual runtime behavior:

- variable block sizes (sample-accurate modulator splits)
- 4-channel cap (stereo main + stereo sidechain)
- modulator to VST3 param round-trip
- freewheeling / render mode

**Deliverable:** `scripts/verify_bitwig.py` (uv-run) that scripts Bitwig in render mode or, lacking that, a DawDreamer-based VST3 harness that exercises variable block sizes and sidechain. Documents any Bitwig-only quirks in research/bitwig.md additions.

**Acceptance:**
- [ ] passes on CI (nightly.yml render job)
- [ ] a manual Bitwig smoke checklist added to DEV_WORKFLOW.md

Source: research/bitwig.md; research/ci_verification.md §Offline rendering.
""",
    ),
    (
        "Configure branch protection on main",
        "v1,p2,ci,docs",
        """Set branch protection on `main` requiring:

- rust / macos-15 green
- rust / ubuntu-24.04 green
- rust / windows-2025 green
- python / pluginrack CLI green
- 1 review (Gemini Code Assist + orchestrator count)
- linear history; squash-merge only

**Command sketch** (tracked in DEV_WORKFLOW.md §CI):

```
gh api -X PUT /repos/bedwards/plugin-rack/branches/main/protection --input protection.json
```

**Acceptance:**
- [ ] settings reflect the required checks after first green CI pass
- [ ] attempting to push direct to main is blocked (for non-owners)
""",
    ),
    (
        "pytest suite for pluginrack CLI",
        "v1,p2,ci,docs",
        """Add unit tests under `pluginrack/tests/` covering:

- `cli --help` returns 0 and lists all commands
- `verify lint` shells out to cargo fmt + clippy
- `issue mirror` parses a fixture Gemini comment payload

Use pytest. Mock subprocess via `unittest.mock.patch`.

**Acceptance:**
- [ ] `uv run pytest -q` passes locally
- [ ] CI python job runs the suite
""",
    ),
    (
        "Offline render verification via dawdreamer",
        "v1,p2,ci,audio",
        """Implement `scripts/verify_render.py` (uv-run, PEP-723 inline deps) that:

1. Loads the built `target/bundled/rack-plugin.vst3`
2. Pipes a deterministic test waveform through it
3. Compares RMS of output vs input — passthrough must be bit-identical (currently the plugin is passthrough)
4. Exits 0 on match

Used by `pluginrack verify render` and by nightly.yml.

**Acceptance:**
- [ ] runs locally on macOS
- [ ] exit code 0 on current passthrough plugin
- [ ] fails visibly if output silent / not-bit-identical

Source: research/ci_verification.md §Offline audio rendering.
""",
    ),
]


def existing_titles() -> set[str]:
    r = subprocess.run(
        ["gh", "issue", "list", "--state", "all", "--limit", "200", "--json", "title"],
        capture_output=True, text=True, check=True,
    )
    return {row["title"] for row in json.loads(r.stdout or "[]")}


def main() -> int:
    existing = existing_titles()
    created = 0
    for title, labels, body in ISSUES:
        if title in existing:
            print(f"SKIP  {title}")
            continue
        r = subprocess.run(
            ["gh", "issue", "create", "--title", title, "--label", labels, "--body", body],
            capture_output=True, text=True,
        )
        if r.returncode != 0:
            print(f"FAIL  {title}: {r.stderr.strip()}", file=sys.stderr)
            continue
        url = r.stdout.strip().splitlines()[-1]
        print(f"  +   {title}  ({url})")
        created += 1
    print(f"\nCreated {created}, skipped {len(ISSUES) - created}")
    subprocess.run(["gh", "issue", "list", "--state", "open", "--limit", "30"])
    return 0


if __name__ == "__main__":
    sys.exit(main())
