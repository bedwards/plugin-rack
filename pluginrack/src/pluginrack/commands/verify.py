"""`pluginrack verify` — tiered verification: lint, unit, bundle, pluginval, render, rt-safety."""

from __future__ import annotations

import os
from pathlib import Path

import click

from pluginrack.sh import run, which


@click.group(invoke_without_command=True, help="Tiered plugin verification.")
@click.pass_context
def verify(ctx: click.Context) -> None:
    if ctx.invoked_subcommand is None:
        ctx.invoke(lint)
        ctx.invoke(unit)
        ctx.invoke(bundle)
        ctx.invoke(pluginval_)


@verify.command(help="Tier 1: fmt + clippy (deny warnings).")
def lint() -> None:
    run(["cargo", "fmt", "--all", "--check"])
    run(["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"])


@verify.command(help="Tier 2: cargo test --workspace.")
def unit() -> None:
    run(["cargo", "test", "--workspace"])


@verify.command(help="Tier 3: bundle the rack-plugin for release.")
def bundle() -> None:
    run(["cargo", "xtask", "bundle", "rack-plugin", "--release"])


def _find_pluginval() -> str | None:
    for candidate in (
        which("pluginval"),
        "tools/pluginval.app/Contents/MacOS/pluginval",
        "tools/pluginval",
        "tools/pluginval.exe",
    ):
        if candidate and Path(candidate).exists():
            return candidate
    return None


@verify.command(name="pluginval", help="Tier 4: run Tracktion pluginval --strictness-level 10.")
@click.option("--strictness", default=10, type=int)
@click.option("--format", "fmt", default="vst3", type=click.Choice(["vst3", "clap"]))
def pluginval_(strictness: int, fmt: str) -> None:
    pv = _find_pluginval()
    if not pv:
        raise click.ClickException(
            "pluginval not found. Download from "
            "https://github.com/Tracktion/pluginval/releases/latest into ./tools/ "
            "or put it on PATH."
        )
    bundled = Path(f"target/bundled/rack-plugin.{fmt}")
    if not bundled.exists():
        raise click.ClickException(f"bundle not found: {bundled} (run pluginrack verify bundle first)")
    logs = Path("pluginval-logs")
    logs.mkdir(exist_ok=True)
    args = [
        pv,
        "--strictness-level",
        str(strictness),
        "--validate-in-process",
        "--output-dir",
        str(logs),
    ]
    if os.environ.get("CI") and os.name != "nt" and not os.environ.get("DISPLAY"):
        args.append("--skip-gui-tests")
    args.extend(["--validate", str(bundled)])
    run(args)


@verify.command(help="Tier 5: offline render smoke test via dawdreamer (uv sync --extra verify first).")
def render() -> None:
    run(["uv", "run", "--extra", "verify", "python", "scripts/verify_render.py"])


@verify.command(help="Tier 6: RT-safety — run the process path under assert_no_alloc.")
def rt_safety() -> None:
    run(["cargo", "test", "--workspace", "--features", "assert_process_allocs"])
