"""Tests for the ``pluginrack status`` command."""

from __future__ import annotations

from pluginrack.cli import main


def test_status_help(runner) -> None:
    result = runner.invoke(main, ["status", "--help"])
    assert result.exit_code == 0, result.output
    assert "snapshot" in result.output.lower() or "status" in result.output.lower()


def test_status_runs_without_network(runner, no_subprocess) -> None:
    """Status shells out to ``gh`` and ``npx``; with those mocked it should not raise."""
    result = runner.invoke(main, ["status"])
    # status may print "no plugin-rack sessions" or Rich tables; the
    # contract here is: exit code 0, and we recorded the gh calls.
    assert result.exit_code == 0, result.output
    recorded = [c[1] for c in no_subprocess if c[0] == "subprocess.run"]
    joined = [" ".join(a) if isinstance(a, list) else str(a) for a in recorded]
    # Expect at least one `gh pr list` and one `gh issue list`.
    assert any("gh" in s and "pr" in s for s in joined), joined
    assert any("gh" in s and "issue" in s for s in joined), joined
