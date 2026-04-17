"""Tests for the ``pluginrack pr`` subcommand group."""

from __future__ import annotations

import json
import subprocess as real_subprocess

from pluginrack.cli import main


def test_pr_group_help(runner) -> None:
    result = runner.invoke(main, ["pr", "--help"])
    assert result.exit_code == 0, result.output
    assert "pr" in result.output.lower()


def test_pr_list_help(runner) -> None:
    result = runner.invoke(main, ["pr", "list-", "--help"])
    assert result.exit_code == 0, result.output


def test_pr_automerge_help(runner) -> None:
    result = runner.invoke(main, ["pr", "automerge", "--help"])
    assert result.exit_code == 0, result.output
    assert "dry-run" in result.output.lower() or "green" in result.output.lower()


def test_pr_automerge_dry_run_with_green_pr(runner, monkeypatch) -> None:
    """Dry-run must not call ``gh pr merge`` even when a PR appears green."""
    calls: list[list[str]] = []

    class _Fake:
        def __init__(self, stdout: str = "", returncode: int = 0) -> None:
            self.stdout = stdout
            self.stderr = ""
            self.returncode = returncode

    def fake_run(cmd, *args, **kwargs):
        calls.append(list(cmd))
        if "list" in cmd:
            data = [
                {
                    "number": 42,
                    "isDraft": False,
                    "mergeStateStatus": "CLEAN",
                    "reviewDecision": "APPROVED",
                    "statusCheckRollup": [{"conclusion": "SUCCESS"}],
                }
            ]
            return _Fake(stdout=json.dumps(data), returncode=0)
        return _Fake(stdout="", returncode=0)

    monkeypatch.setattr(real_subprocess, "run", fake_run)

    result = runner.invoke(main, ["pr", "automerge", "--dry-run"])
    assert result.exit_code == 0, result.output
    # Exactly the listing call; no `gh pr merge` under dry-run.
    merge_calls = [c for c in calls if len(c) >= 3 and c[0] == "gh" and c[1] == "pr" and c[2] == "merge"]
    assert merge_calls == [], f"dry-run must not merge, got {merge_calls}"
    assert "#42" in result.output
