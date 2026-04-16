"""`pluginrack issue` — triage + mirror Gemini Code Assist comments to issues."""

from __future__ import annotations

import json
import subprocess
import sys

import click


@click.group(help="GitHub issue operations.")
def issue() -> None: ...


@issue.command(help="List open issues.")
@click.option("--limit", default=30, type=int)
def list_(limit: int) -> None:
    subprocess.run(["gh", "issue", "list", "--state", "open", "--limit", str(limit)], check=True)


@issue.command(
    name="mirror",
    help="Mirror Gemini Code Assist review comments from a PR into new issues.",
)
@click.argument("pr_number", type=int)
@click.option("--dry-run/--no-dry-run", default=False)
@click.option("--severity", default="any", type=click.Choice(["any", "critical", "high", "medium", "low"]))
def mirror(pr_number: int, dry_run: bool, severity: str) -> None:
    comments = _gemini_comments(pr_number)
    if severity != "any":
        comments = [c for c in comments if severity.lower() in (c.get("body") or "").lower()]
    if not comments:
        click.echo("No matching Gemini Code Assist comments.")
        return
    for c in comments:
        title = f"[from PR #{pr_number} review] {_first_line(c.get('body', ''))[:80]}"
        body = (
            f"Mirrored from Gemini Code Assist review on #{pr_number}.\n\n"
            f"Original comment: {c.get('html_url','')}\n\n"
            f"---\n\n{c.get('body','')}"
        )
        click.echo(f"→ {title}")
        if not dry_run:
            subprocess.run(
                ["gh", "issue", "create", "--title", title, "--body", body, "--label", "review-feedback"],
                check=True,
            )


def _gemini_comments(pr_number: int) -> list[dict]:
    out = []
    for endpoint in (
        f"/repos/{{owner}}/{{repo}}/pulls/{pr_number}/comments",
        f"/repos/{{owner}}/{{repo}}/issues/{pr_number}/comments",
    ):
        r = subprocess.run(
            [
                "gh",
                "api",
                endpoint,
                "--jq",
                '[ .[] | select(.user.login=="gemini-code-assist[bot]") ]',
            ],
            capture_output=True,
            text=True,
        )
        if r.returncode != 0:
            print(r.stderr, file=sys.stderr)
            continue
        try:
            out.extend(json.loads(r.stdout or "[]"))
        except json.JSONDecodeError:
            continue
    return out


def _first_line(s: str) -> str:
    return (s or "").splitlines()[0] if s else ""
