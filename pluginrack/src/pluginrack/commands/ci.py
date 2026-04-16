"""`pluginrack ci` — inspect recent workflow runs, read failed logs."""

from __future__ import annotations

import subprocess

import click


@click.group(help="CI operations.")
def ci() -> None: ...


@ci.command(help="List recent workflow runs.")
@click.option("--limit", default=10, type=int)
def runs(limit: int) -> None:
    subprocess.run(["gh", "run", "list", "--limit", str(limit)], check=True)


@ci.command(help="Show the log of the most recent failed run (--log-failed).")
@click.option("--run-id", default=None, help="Specific run id; defaults to most recent failure.")
def fail_log(run_id: str | None) -> None:
    if not run_id:
        res = subprocess.run(
            ["gh", "run", "list", "--status", "failure", "--limit", "1", "--json", "databaseId"],
            capture_output=True, text=True, check=True,
        )
        import json
        rows = json.loads(res.stdout or "[]")
        if not rows:
            click.echo("no failed runs")
            return
        run_id = str(rows[0]["databaseId"])
    subprocess.run(["gh", "run", "view", run_id, "--log-failed"], check=True)
