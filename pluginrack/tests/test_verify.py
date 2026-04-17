"""Tests for the ``pluginrack verify`` subcommand group."""

from __future__ import annotations

from pluginrack.cli import main


def test_verify_group_help_mentions_tiers(runner) -> None:
    result = runner.invoke(main, ["verify", "--help"])
    assert result.exit_code == 0, result.output
    assert "verification" in result.output.lower()


def test_verify_lint_help(runner) -> None:
    result = runner.invoke(main, ["verify", "lint", "--help"])
    assert result.exit_code == 0, result.output
    assert "fmt" in result.output.lower() or "clippy" in result.output.lower()


def test_verify_unit_help(runner) -> None:
    result = runner.invoke(main, ["verify", "unit", "--help"])
    assert result.exit_code == 0, result.output
    assert "cargo test" in result.output.lower() or "workspace" in result.output.lower()


def test_verify_bundle_help(runner) -> None:
    result = runner.invoke(main, ["verify", "bundle", "--help"])
    assert result.exit_code == 0, result.output
    assert "bundle" in result.output.lower()


def test_verify_pluginval_help_mentions_vst3(runner) -> None:
    result = runner.invoke(main, ["verify", "pluginval", "--help"])
    assert result.exit_code == 0, result.output
    assert "vst3" in result.output.lower() or "pluginval" in result.output.lower()


def test_verify_clap_validator_help_mentions_clap(runner) -> None:
    result = runner.invoke(main, ["verify", "clap-validator", "--help"])
    assert result.exit_code == 0, result.output
    assert "clap" in result.output.lower()


def test_verify_rt_safety_help(runner) -> None:
    result = runner.invoke(main, ["verify", "rt-safety", "--help"])
    assert result.exit_code == 0, result.output


def test_verify_bitwig_mod_help(runner) -> None:
    result = runner.invoke(main, ["verify", "bitwig-mod", "--help"])
    assert result.exit_code == 0, result.output
    assert "block" in result.output.lower() or "macro" in result.output.lower()


def test_verify_lint_invokes_cargo(runner, no_subprocess) -> None:
    """`verify lint` shells out to cargo fmt + clippy — mocked here."""
    result = runner.invoke(main, ["verify", "lint"])
    assert result.exit_code == 0, result.output
    # Two run() calls expected: fmt --check, then clippy.
    argvs = [c[1] for c in no_subprocess if c[0] == "sh.run"]
    joined = [" ".join(a) for a in argvs]
    assert any("cargo" in s and "fmt" in s for s in joined), joined
    assert any("cargo" in s and "clippy" in s for s in joined), joined


def test_verify_unit_invokes_cargo_test(runner, no_subprocess) -> None:
    result = runner.invoke(main, ["verify", "unit"])
    assert result.exit_code == 0, result.output
    argvs = [c[1] for c in no_subprocess if c[0] == "sh.run"]
    assert any("test" in a and "cargo" in a for a in argvs), argvs


def test_verify_bundle_invokes_xtask(runner, no_subprocess) -> None:
    result = runner.invoke(main, ["verify", "bundle"])
    assert result.exit_code == 0, result.output
    argvs = [c[1] for c in no_subprocess if c[0] == "sh.run"]
    assert any("xtask" in a and "bundle" in a for a in argvs), argvs


def test_verify_pluginval_errors_without_binary(runner, monkeypatch, tmp_path) -> None:
    """When pluginval is not on PATH and ./tools/pluginval is absent, fail cleanly."""
    monkeypatch.chdir(tmp_path)
    import pluginrack.commands.verify as vmod

    monkeypatch.setattr(vmod, "which", lambda _: None)
    result = runner.invoke(main, ["verify", "pluginval"])
    assert result.exit_code != 0
    assert "pluginval not found" in result.output.lower() or "not found" in result.output.lower()
