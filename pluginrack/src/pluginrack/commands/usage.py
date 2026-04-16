"""`pluginrack usage` — real Claude subscription numbers via ccusage."""

from __future__ import annotations

import json
import subprocess

import click


@click.group(help="Claude subscription usage (ccusage).")
def usage() -> None: ...


@usage.command(help="Current session totals.")
def session() -> None:
    _run(["npx", "-y", "ccusage@latest", "session", "--json"])


@usage.command(help="Daily totals.")
def daily() -> None:
    _run(["npx", "-y", "ccusage@latest", "daily", "--json"])


@usage.command(help="Rate-limit block snapshot (5h window).")
def block() -> None:
    subprocess.run(["npx", "-y", "ccusage@latest", "blocks", "--live"])


@usage.command(help="Monthly totals.")
def monthly() -> None:
    _run(["npx", "-y", "ccusage@latest", "monthly", "--json"])


def _run(cmd: list[str]) -> None:
    r = subprocess.run(cmd, capture_output=True, text=True)
    if r.returncode != 0:
        raise click.ClickException(r.stderr.strip())
    try:
        data = json.loads(r.stdout)
        click.echo(json.dumps(data, indent=2)[:4000])
    except json.JSONDecodeError:
        click.echo(r.stdout)
