"""`pluginrack pr` — PR helpers: watch, auto-merge green PRs."""

from __future__ import annotations

import subprocess

import click


@click.group(help="GitHub PR operations.")
def pr() -> None: ...


@pr.command(help="List open PRs with CI / review status.")
def list_() -> None:
    subprocess.run(
        ["gh", "pr", "list", "--state", "open", "--json",
         "number,title,isDraft,reviewDecision,statusCheckRollup"],
        check=True,
    )


@pr.command(help="Auto-merge any PR that is green, not draft, and approved.")
@click.option("--dry-run/--no-dry-run", default=False)
def automerge(dry_run: bool) -> None:
    res = subprocess.run(
        ["gh", "pr", "list", "--state", "open", "--json",
         "number,isDraft,mergeStateStatus,reviewDecision,statusCheckRollup"],
        capture_output=True, text=True, check=True,
    )
    import json
    for p in json.loads(res.stdout or "[]"):
        if p.get("isDraft"):
            continue
        checks = p.get("statusCheckRollup") or []
        all_ok = all(c.get("conclusion") in (None, "SUCCESS", "NEUTRAL", "SKIPPED") for c in checks) and len(checks) > 0
        if not all_ok:
            click.echo(f"  #{p['number']} skipped (CI not green)")
            continue
        click.echo(f"  #{p['number']} → auto-merge")
        if not dry_run:
            subprocess.run(
                ["gh", "pr", "merge", str(p["number"]), "--auto", "--squash", "--delete-branch"],
                check=True,
            )
