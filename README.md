# gdscript-lsp

A standalone GDScript language server, linter, and formatter.
Yes, it is giving clean scripts and less chaos uwu.

It supports:

- LSP over stdio (`lsp`)
- linting (`lint`)
- formatting (`format`)
- parse + lint checks (`check`)
- rule listing (`rules`)
- parity reporting against fixtures (`parity-report`)

## Quick Start

You need Rust installed (`cargo`).

```bash
cargo install --path .
```

Or just run the project install flow:

```bash
just install
```

`just install` does two things:

1. installs `gdscript-lsp`
2. runs `scripts/setup_helix.sh` to wire Helix config

So if you are lazy (same), just do `just install` and move on with life.
Hit the command and pretend you planned everything perfectly.

## Helix Setup

If you already installed the binary and only want editor wiring:

```bash
scripts/setup_helix.sh
```

Or pass a specific binary path:

```bash
scripts/setup_helix.sh /abs/path/to/gdscript-lsp
```

The script updates `~/.config/helix/languages.toml` with a managed `gdscript-lsp` block.
It removes old legacy blocks too, so your config does not become cursed.
Please do not hand-edit five random server blocks at 2:00 AM.

If you want to configure Helix manually, this is the expected example:

```toml
[language-server.gdscript-lsp]
command = "gdscript-lsp"
args = ["lsp"]

[[language]]
name = "gdscript"
language-id = "gdscript"
file-types = ["gd", "gdscript"]
roots = ["project.godot", ".git"]
auto-format = true
language-servers = ["gdscript-lsp"]
```

## CLI Usage

```bash
gdscript-lsp <command> [options]
```

Think of it as:
- `lsp` for editor brain, instant IQ boost
- `lint` for judging your code choices (respectfully)
- `format` for making it look put together and not crusty
- `check` for parser + lint combo, aka no surprises
- `rules` when you want receipts
- `parity-report` when you want to close fixture gaps without guessing

Main commands:

```bash
gdscript-lsp lsp
gdscript-lsp lint path/to/file.gd
gdscript-lsp format --check path/to/file.gd
gdscript-lsp format --write path/to/file.gd
gdscript-lsp check path/to/file.gd
gdscript-lsp rules
gdscript-lsp parity-report --limit 20
```

Global options:

- `--godot-version <version>`
- `--project <path-to-project.godot>`
- `--mode <parity|enhanced>`
- `--config <path>`

Lint override flags:

- `--max-line-length <n>`
- `--allow-tabs` / `--disallow-tabs`
- `--require-spaces-around-operators` / `--allow-tight-operators`

## LSP Transport

`gdscript-lsp lsp` speaks JSON-RPC over stdio and supports:

- standard framed transport (`Content-Length` headers)
- line-delimited JSON messages (handy for local testing)

So yes, normal LSP clients can use it directly. No weird bridge needed, no extra drama.
Very slay, very standard, very no hacks.

## Dev Loop

```bash
cargo fmt
cargo test
cargo test --test lsp_protocol
```

If you are working parity gaps:

```bash
cargo run -- parity-report --limit 40
```

Then fix the top mismatch family first.
It's less pain, more signal, and honestly just feels better than random fixing.
Like literally do not yolo-fix 200 files blind.

## Tiny Survival Notes

- If Helix says the server is missing, run `just install` again and restart Helix.
- If config feels weird, check `~/.config/helix/languages.toml` for duplicate old blocks.
- If you want fast confidence, run `cargo test --test lsp_protocol` before full test suite.

You are now spiritually ready. Go slay your diagnostics.
XOXO and good luck.
