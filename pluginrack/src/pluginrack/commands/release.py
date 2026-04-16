"""`pluginrack release` — minor version bump + git tag per subcomponent change."""

from __future__ import annotations

import re
import subprocess
from pathlib import Path

import click


@click.group(help="Release operations.")
def release() -> None: ...


@release.command(help="Bump minor version of a crate and git-tag it.")
@click.argument("crate")
@click.option("--dry-run/--no-dry-run", default=False)
def bump(crate: str, dry_run: bool) -> None:
    cargo_toml = Path(f"crates/{crate}/Cargo.toml")
    if not cargo_toml.exists() and crate == "xtask":
        cargo_toml = Path("xtask/Cargo.toml")
    if not cargo_toml.exists():
        raise click.ClickException(f"{cargo_toml} not found")

    text = cargo_toml.read_text()
    m = re.search(r'^version\s*=\s*"(\d+)\.(\d+)\.(\d+)"', text, re.MULTILINE)
    if not m:
        raise click.ClickException(f"no version field in {cargo_toml}")
    major, minor, patch = (int(x) for x in m.groups())
    new_version = f"{major}.{minor + 1}.0"
    new_text = re.sub(
        r'^version\s*=\s*"\d+\.\d+\.\d+"',
        f'version = "{new_version}"',
        text,
        count=1,
        flags=re.MULTILINE,
    )

    tag = f"{crate}-v{new_version}"
    click.echo(f"{crate}: {major}.{minor}.{patch} → {new_version}  (tag {tag})")
    if dry_run:
        return
    cargo_toml.write_text(new_text)
    subprocess.run(["git", "add", str(cargo_toml)], check=True)
    subprocess.run(["git", "commit", "-m", f"bump {crate} to {new_version}"], check=True)
    subprocess.run(["git", "tag", tag], check=True)
    click.echo(f"Tagged {tag}. Push with:  git push --follow-tags")
