# CLI parity harness

This directory contains integration-test fixtures for the CLI parity harness.

- `lint/` covers `gdscript-lsp lint`
- `format-check/` covers `gdscript-lsp format --check`
- `check/` covers `gdscript-lsp check`
- `rules/` covers `gdscript-lsp rules`

Fixture format per case:

- `args.txt`: one CLI token per line
- `input.gd`: fixture source under test (when required by the command)
- `expect/exit.txt`: expected exit code
- `expect/stdout.golden`: expected `stdout`
- `expect/stderr.golden`: expected `stderr` (optional; empty by default)

Token `${fixture}` in `args.txt` is replaced at runtime with the absolute
path to `input.gd`.

Token `${fixture_dir}` in `args.txt` is replaced at runtime with the absolute
path to the fixture case directory.

To add a new case:

1. Create `tests/fixtures/<suite>/<case>/`.
2. Add `args.txt`, optional `input.gd`, and expected files above.
3. Add a matching test in `tests/cli_parity.rs`.

When writing new cases for command-specific CLI flags, keep the subcommand token before its options (for example: `lint --disallow-tabs --max-line-length 80 <file>`). Global flags (`--project`, `--mode`, etc.) remain global and may appear before the subcommand.
