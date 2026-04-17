"""`pluginrack verify` — tiered verification: lint, unit, bundle, pluginval, clap-validator, render, rt-safety."""

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
        ctx.invoke(clap_validator)


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


def _find_clap_validator() -> str | None:
    for candidate in (
        which("clap-validator"),
        "tools/clap-validator",
        "tools/clap-validator.exe",
        "clap-validator/clap-validator",
        "clap-validator/clap-validator.exe",
    ):
        if candidate and Path(candidate).exists():
            return candidate
    return None


@verify.command(name="pluginval", help="Tier 4a: run Tracktion pluginval on the VST3 bundle (strictness 10).")
@click.option("--strictness", default=10, type=int)
@click.argument("wrapper_args", nargs=-1, type=click.UNPROCESSED)
def pluginval_(strictness: int, wrapper_args: tuple[str, ...]) -> None:
    # pluginval is VST3-only (JUCE-based; no CLAP support). CLAP validation
    # lives in `pluginrack verify clap-validator`. No `--format` option
    # here because offering `clap` would always fail.
    pv = _find_pluginval()
    if not pv:
        raise click.ClickException(
            "pluginval not found. Download from "
            "https://github.com/Tracktion/pluginval/releases/latest into ./tools/ "
            "or put it on PATH."
        )
    bundled = Path("target/bundled/rack-plugin.vst3")
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
    args.extend(wrapper_args)
    args.extend(["--validate", str(bundled)])
    run(args)


@verify.command(
    name="clap-validator",
    help=(
        "Tier 4b: run free-audio/clap-validator on the CLAP bundle. "
        "Mirrors the CI step. Pass extra flags after '--' to forward to clap-validator."
    ),
)
@click.option(
    "--only-failed/--all-output",
    default=True,
    help="--only-failed hides successful/skipped tests (default, matches CI).",
)
@click.argument("wrapper_args", nargs=-1, type=click.UNPROCESSED)
def clap_validator(only_failed: bool, wrapper_args: tuple[str, ...]) -> None:
    cv = _find_clap_validator()
    if not cv:
        raise click.ClickException(
            "clap-validator not found. Download from "
            "https://github.com/free-audio/clap-validator/releases/tag/0.3.2 "
            "into ./tools/ (or ./clap-validator/) or put it on PATH."
        )
    bundled = Path("target/bundled/rack-plugin.clap")
    if not bundled.exists():
        raise click.ClickException(f"bundle not found: {bundled} (run pluginrack verify bundle first)")
    args = [cv, "validate"]
    if only_failed:
        args.append("--only-failed")
    args.extend(wrapper_args)
    args.append(str(bundled))
    run(args)


@verify.command(help="Tier 5: offline render smoke test via dawdreamer (uv sync --extra verify first).")
def render() -> None:
    run(["uv", "run", "--extra", "verify", "python", "scripts/verify_render.py"])


@verify.command(
    name="bitwig-mod",
    help=(
        "Tier 5c: offline Bitwig-style verification — renders the VST3 at block sizes "
        "{64, 511, 1024} and ramps a sample of macro params (0, 63, 127) to prove the "
        "plugin handles variable block sizes and exposes modulatable parameters. "
        "Requires the `verify` optional-deps group (dawdreamer + numpy)."
    ),
)
def bitwig_mod() -> None:
    # Lazy import: keeps `pluginrack --help` working even when the `verify`
    # extras group (dawdreamer) is not installed on this interpreter.
    from pluginrack.commands.verify_bitwig_mod import run as run_bitwig_mod

    rc = run_bitwig_mod(verbose=True)
    if rc != 0:
        raise click.ClickException(f"bitwig-mod verify failed (rc={rc})")


@verify.command(help="Tier 6: RT-safety — run the process path under assert_no_alloc.")
def rt_safety() -> None:
    run(["cargo", "test", "--workspace", "--features", "assert_process_allocs"])
