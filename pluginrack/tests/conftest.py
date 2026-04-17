"""Shared fixtures for the pluginrack pytest suite.

The suite never invokes real tools (cargo, gh, uv, curl, dawdreamer). Any test
that exercises a command which would shell out must use the ``no_subprocess``
fixture below to stub out both :func:`pluginrack.sh.run` and
:func:`subprocess.run`.
"""

from __future__ import annotations

import subprocess
from types import SimpleNamespace
from typing import Any

import pytest
from click.testing import CliRunner


@pytest.fixture
def runner() -> CliRunner:
    """A fresh Click ``CliRunner`` per test (mix_stderr default)."""
    return CliRunner()


class _FakeCompleted(SimpleNamespace):
    """Stand-in for :class:`subprocess.CompletedProcess` — enough API for the CLI."""

    def __init__(self, returncode: int = 0, stdout: str = "", stderr: str = "") -> None:
        super().__init__(returncode=returncode, stdout=stdout, stderr=stderr)


@pytest.fixture
def no_subprocess(monkeypatch: pytest.MonkeyPatch) -> list[tuple[Any, ...]]:
    """Replace external process launchers with a recorder that never spawns.

    Returns a list of recorded calls; each entry is ``(source, argv)`` where
    ``source`` is ``"sh.run"`` or ``"subprocess.run"``.
    """
    calls: list[tuple[Any, ...]] = []

    def fake_sh_run(cmd, *args, **kwargs):  # noqa: ANN001, ANN002, ANN003
        calls.append(("sh.run", cmd))
        return _FakeCompleted(stdout="", stderr="")

    def fake_subprocess_run(cmd, *args, **kwargs):  # noqa: ANN001, ANN002, ANN003
        calls.append(("subprocess.run", cmd))
        # ccusage JSON endpoints return an object, not a list; the status
        # command calls ``.get("sessions", [])`` on the parsed payload. Hand
        # back an empty object for those calls and an empty list for gh.
        argv = list(cmd) if not isinstance(cmd, str) else cmd.split()
        if argv and argv[0] == "npx":
            return _FakeCompleted(stdout="{}", stderr="")
        return _FakeCompleted(stdout="[]", stderr="")

    import pluginrack.sh as sh_mod

    monkeypatch.setattr(sh_mod, "run", fake_sh_run)
    monkeypatch.setattr(subprocess, "run", fake_subprocess_run)

    # Also patch the already-imported `run` symbol in any command module that
    # did `from pluginrack.sh import run`.
    from pluginrack.commands import build as build_mod
    from pluginrack.commands import verify as verify_mod

    monkeypatch.setattr(build_mod, "run", fake_sh_run)
    monkeypatch.setattr(verify_mod, "run", fake_sh_run)

    return calls
