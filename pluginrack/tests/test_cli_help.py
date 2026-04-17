"""Smoke test ``--help`` at every level of the pluginrack CLI.

No subprocesses, no network. Uses ``click.testing.CliRunner`` exclusively.
"""

from __future__ import annotations

import pytest

from pluginrack.cli import main


def _invoke_help(runner, args: list[str]):
    return runner.invoke(main, [*args, "--help"])


def test_top_level_help_lists_all_subcommands(runner) -> None:
    result = runner.invoke(main, ["--help"])
    assert result.exit_code == 0, result.output
    for cmd in ("build", "verify", "status", "issue", "pr", "ci", "release", "usage"):
        assert cmd in result.output, f"missing {cmd!r} in top-level help"


def test_version_flag(runner) -> None:
    result = runner.invoke(main, ["--version"])
    assert result.exit_code == 0, result.output
    assert "pluginrack" in result.output


@pytest.mark.parametrize(
    "group",
    ["build", "verify", "issue", "pr", "ci", "release", "usage"],
)
def test_group_help(runner, group: str) -> None:
    result = _invoke_help(runner, [group])
    assert result.exit_code == 0, result.output
    # Every group's help must include the group name itself in the usage line.
    assert "Usage:" in result.output
    assert group in result.output


def test_status_help(runner) -> None:
    # status is a leaf command, not a group — it still must have --help.
    result = _invoke_help(runner, ["status"])
    assert result.exit_code == 0, result.output
    assert "Usage:" in result.output


@pytest.mark.parametrize(
    ("group", "sub"),
    [
        ("build", "bundle"),
        ("build", "packages"),
        ("build", "all"),
        ("verify", "lint"),
        ("verify", "unit"),
        ("verify", "bundle"),
        ("verify", "pluginval"),
        ("verify", "clap-validator"),
        ("verify", "render"),
        ("verify", "bitwig-mod"),
        ("verify", "rt-safety"),
        ("issue", "list-"),
        ("issue", "mirror"),
        ("pr", "list-"),
        ("pr", "automerge"),
        ("ci", "runs"),
        ("ci", "fail-log"),
        ("release", "bump"),
        ("usage", "session"),
        ("usage", "daily"),
        ("usage", "block"),
        ("usage", "monthly"),
    ],
)
def test_subcommand_help(runner, group: str, sub: str) -> None:
    result = _invoke_help(runner, [group, sub])
    assert result.exit_code == 0, f"{group} {sub} --help failed: {result.output}"
    assert "Usage:" in result.output
