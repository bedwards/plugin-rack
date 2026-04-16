"""`pluginrack status` — snapshot: PRs, issues, CI, subscription."""

from __future__ import annotations

import json
import subprocess

import click
from rich.console import Console
from rich.table import Table

console = Console()


@click.command(help="Snapshot of open PRs, open issues, CI state, and subscription usage.")
def status() -> None:
    _section("Open PRs", ["gh", "pr", "list", "--state", "open", "--json", "number,title,isDraft,reviewDecision,statusCheckRollup,author"])
    _section("Open issues (top 10)", ["gh", "issue", "list", "--state", "open", "--limit", "10", "--json", "number,title,labels,assignees"])
    _section("Recent workflow runs", ["gh", "run", "list", "--limit", "5", "--json", "databaseId,name,status,conclusion,headBranch,createdAt"])
    _ccusage_block()


def _section(title: str, cmd: list[str]) -> None:
    console.rule(title)
    result = subprocess.run(cmd, capture_output=True, text=True)
    if result.returncode != 0:
        console.print(f"[red]{result.stderr.strip()}[/red]")
        return
    try:
        data = json.loads(result.stdout or "[]")
    except json.JSONDecodeError:
        console.print(result.stdout)
        return
    if not data:
        console.print("  (none)")
        return
    table = Table(show_lines=False)
    if isinstance(data, list) and data:
        for key in data[0]:
            table.add_column(key)
        for row in data:
            table.add_row(*[_fmt(row[k]) for k in data[0]])
        console.print(table)


def _fmt(value: object) -> str:
    if isinstance(value, (list, dict)):
        return json.dumps(value, separators=(",", ":"))[:60]
    return str(value)


def _ccusage_block() -> None:
    console.rule("Claude subscription (ccusage session, current project)")
    result = subprocess.run(
        ["npx", "-y", "ccusage@latest", "session", "--json"],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        console.print("[yellow]ccusage unavailable (install node or check network)[/yellow]")
        return
    try:
        data = json.loads(result.stdout)
    except json.JSONDecodeError:
        console.print(result.stdout[-500:])
        return
    sessions = [s for s in data.get("sessions", []) if "plugin-rack" in s.get("sessionId", "")]
    if not sessions:
        console.print("  (no plugin-rack sessions in ccusage yet)")
        return
    for s in sessions:
        console.print(
            f"  {s['sessionId']}  cost=${s['totalCost']:.2f}  tokens={s['totalTokens']:,}  "
            f"last={s.get('lastActivity','?')}"
        )
