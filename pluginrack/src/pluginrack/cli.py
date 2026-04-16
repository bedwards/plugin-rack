"""pluginrack CLI entry point."""

from __future__ import annotations

import click

from pluginrack import __version__
from pluginrack.commands import build, ci, issue, pr, release, status, usage, verify


@click.group(
    context_settings={"help_option_names": ["-h", "--help"]},
    help="plugin-rack orchestration CLI.\n\nShape: pluginrack [global] <cmd> [args] [-- wrapper-args]",
)
@click.version_option(__version__, prog_name="pluginrack")
@click.option("--repo-root", default=".", type=click.Path(file_okay=False), help="Repo root.")
@click.pass_context
def main(ctx: click.Context, repo_root: str) -> None:
    ctx.ensure_object(dict)
    ctx.obj["repo_root"] = repo_root


main.add_command(build.build)
main.add_command(verify.verify)
main.add_command(status.status)
main.add_command(issue.issue)
main.add_command(pr.pr)
main.add_command(ci.ci)
main.add_command(release.release)
main.add_command(usage.usage)


if __name__ == "__main__":
    main()
