"""`pluginrack build` — wraps `cargo xtask bundle`."""

from __future__ import annotations

import click

from pluginrack.sh import run


@click.group(help="Build plugin bundles.")
def build() -> None: ...


@build.command(help="Bundle a single package. Example: pluginrack build bundle rack-plugin -- --release")
@click.argument("package", default="rack-plugin")
@click.option("--release/--debug", default=True, help="Release build (default).")
@click.option("--universal/--native", default=False, help="macOS universal binary (x86_64 + aarch64).")
@click.argument("wrapper_args", nargs=-1, type=click.UNPROCESSED)
def bundle(package: str, release: bool, universal: bool, wrapper_args: tuple[str, ...]) -> None:
    task = "bundle-universal" if universal else "bundle"
    cmd = ["cargo", "xtask", task, package]
    if release:
        cmd.append("--release")
    cmd.extend(wrapper_args)
    run(cmd)


@build.command(help="List bundlable packages (cargo xtask known-packages).")
def packages() -> None:
    run(["cargo", "xtask", "known-packages"])


@build.command(help="Bundle every known package (release).")
def all() -> None:  # noqa: A003
    result = run(["cargo", "xtask", "known-packages"], capture=True)
    pkgs = [p.strip() for p in result.stdout.splitlines() if p.strip()]
    if not pkgs:
        raise click.ClickException("cargo xtask known-packages returned nothing")
    run(["cargo", "xtask", "bundle", *pkgs, "--release"])
