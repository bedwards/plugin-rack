"""Thin shell wrapper. All external process calls in the CLI go through this."""

from __future__ import annotations

import shlex
import subprocess
import sys
from pathlib import Path


def run(cmd: list[str] | str, *, cwd: str | Path | None = None, check: bool = True, capture: bool = False) -> subprocess.CompletedProcess[str]:
    if isinstance(cmd, str):
        display = cmd
        argv = shlex.split(cmd)
    else:
        display = " ".join(shlex.quote(c) for c in cmd)
        argv = cmd
    print(f"$ {display}", file=sys.stderr)
    return subprocess.run(
        argv,
        cwd=str(cwd) if cwd else None,
        check=check,
        text=True,
        capture_output=capture,
    )


def which(cmd: str) -> str | None:
    from shutil import which as _which
    return _which(cmd)
