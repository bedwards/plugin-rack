"""Tests for the ``pluginrack issue`` subcommand group."""

from __future__ import annotations

import json
import subprocess as real_subprocess

from pluginrack.cli import main


def test_issue_group_help(runner) -> None:
    result = runner.invoke(main, ["issue", "--help"])
    assert result.exit_code == 0, result.output


def test_issue_list_help(runner) -> None:
    result = runner.invoke(main, ["issue", "list-", "--help"])
    assert result.exit_code == 0, result.output


def test_issue_mirror_help(runner) -> None:
    result = runner.invoke(main, ["issue", "mirror", "--help"])
    assert result.exit_code == 0, result.output
    assert "gemini" in result.output.lower() or "mirror" in result.output.lower()


def test_issue_mirror_dry_run_with_fixture_gemini_comment(runner, monkeypatch) -> None:
    """Feed a fixture Gemini comment payload; dry-run must not create issues."""
    fixture_comments = [
        {
            "body": "**High severity:** this is a test Gemini comment.\nMore details here.",
            "html_url": "https://example.invalid/pr/1#comment-1",
            "user": {"login": "gemini-code-assist[bot]"},
        }
    ]
    calls: list[list[str]] = []

    class _Fake:
        def __init__(self, stdout: str = "", returncode: int = 0) -> None:
            self.stdout = stdout
            self.stderr = ""
            self.returncode = returncode

    def fake_run(cmd, *args, **kwargs):
        calls.append(list(cmd))
        if len(cmd) >= 2 and cmd[0] == "gh" and cmd[1] == "api":
            # The mirror command makes two endpoint probes; return the
            # fixture on the first and an empty list on the second.
            calls_to_api = sum(1 for c in calls if len(c) >= 2 and c[0] == "gh" and c[1] == "api")
            if calls_to_api == 1:
                return _Fake(stdout=json.dumps(fixture_comments), returncode=0)
            return _Fake(stdout="[]", returncode=0)
        if len(cmd) >= 2 and cmd[0] == "gh" and cmd[1] == "issue" and cmd[2] == "create":
            return _Fake(stdout="", returncode=0)
        return _Fake(stdout="", returncode=0)

    monkeypatch.setattr(real_subprocess, "run", fake_run)

    result = runner.invoke(main, ["issue", "mirror", "42", "--dry-run"])
    assert result.exit_code == 0, result.output

    # No `gh issue create` must have been invoked under dry-run.
    created = [c for c in calls if len(c) >= 3 and c[0] == "gh" and c[1] == "issue" and c[2] == "create"]
    assert created == [], f"dry-run must not create issues, got {created}"

    # The first line of the fixture comment must appear in the summary output.
    assert "test Gemini comment" in result.output or "Gemini" in result.output
