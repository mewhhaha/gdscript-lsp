use clap::Parser;
use std::process;

use gdscript_lsp::{
    Cli, Commands, check_command, format_command, lint_command, lsp, parity_report_command,
    resolve_engine_config, rules_command,
};

fn main() {
    let cli = Cli::parse();
    let project = cli.global.project.clone();
    let engine = resolve_engine_config(
        project.as_deref(),
        cli.global.godot_version,
        cli.global.mode,
    );

    let result = match cli.command {
        Commands::Lsp => lsp::run_stdio_with_config(engine),
        Commands::Lint { files, overrides } => lint_command(
            files,
            project,
            overrides.into(),
            cli.global.godot_version,
            cli.global.mode,
        ),
        Commands::Format {
            files,
            write,
            check,
        } => format_command(files, write, check),
        Commands::Check { files, overrides } => check_command(
            files,
            project,
            overrides.into(),
            cli.global.godot_version,
            cli.global.mode,
        ),
        Commands::Rules => rules_command(),
        Commands::ParityReport {
            json,
            strict,
            limit,
        } => parity_report_command(json, strict, limit),
    };

    if let Err(err) = result {
        eprintln!("{err}");
        process::exit(1);
    }
}
