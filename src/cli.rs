use crate::engine::{BehaviorMode, Version};
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Clone, Args, Default)]
pub struct GlobalOptions {
    /// Target Godot behavior version.
    #[arg(long, global = true)]
    pub godot_version: Option<Version>,
    /// Optional explicit project.godot path.
    #[arg(long, global = true)]
    pub project: Option<PathBuf>,
    /// Behavior mode for feature parity vs enhanced UX.
    #[arg(long, global = true)]
    pub mode: Option<BehaviorMode>,
    /// Reserved config path for future tool settings.
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,
}

#[derive(Debug, Clone, Args, Default)]
pub struct LintRuleOverrides {
    /// Override maximum allowed line length.
    #[arg(long)]
    pub max_line_length: Option<usize>,
    /// Allow tabs in source code.
    #[arg(long, conflicts_with = "disallow_tabs")]
    pub allow_tabs: bool,
    /// Force tab diagnostics regardless of project config.
    #[arg(long, conflicts_with = "allow_tabs")]
    pub disallow_tabs: bool,
    /// Enforce spaces around assignment operators.
    #[arg(long, conflicts_with = "allow_tight_operators")]
    pub require_spaces_around_operators: bool,
    /// Allow tight assignment operators like a=1.
    #[arg(long, conflicts_with = "require_spaces_around_operators")]
    pub allow_tight_operators: bool,
}

#[derive(Debug, Parser)]
#[command(name = "gdscript-lsp")]
#[command(about = "Standalone GDScript LSP, linter, and formatter")]
pub struct Cli {
    #[command(flatten)]
    pub global: GlobalOptions,
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Start a minimal LSP stdio loop.
    Lsp,
    /// Run lint rules on source files.
    Lint {
        #[command(flatten)]
        overrides: LintRuleOverrides,
        /// Input paths to lint.
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
    /// Format source files.
    Format {
        /// Input paths to format.
        #[arg(required = true)]
        files: Vec<PathBuf>,
        /// Only check formatting changes without writing.
        #[arg(long)]
        check: bool,
        /// Write formatted output back to input files.
        #[arg(short, long)]
        write: bool,
    },
    /// Parse and lint source files.
    Check {
        #[command(flatten)]
        overrides: LintRuleOverrides,
        /// Input paths to check.
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
    /// Print available lint rule names.
    Rules,
    /// Report fixture parity gaps versus upstream snapshots.
    ParityReport {
        /// Emit report as JSON.
        #[arg(long)]
        json: bool,
        /// Fail with non-zero exit if any gaps are found.
        #[arg(long)]
        strict: bool,
        /// Maximum entries per mismatch section in text output.
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
}

impl From<LintRuleOverrides> for crate::lint::LintOverrides {
    fn from(value: LintRuleOverrides) -> Self {
        let allow_tabs = if value.allow_tabs {
            Some(true)
        } else if value.disallow_tabs {
            Some(false)
        } else {
            None
        };

        let require_spaces_around_operators = if value.require_spaces_around_operators {
            Some(true)
        } else if value.allow_tight_operators {
            Some(false)
        } else {
            None
        };

        Self {
            max_line_length: value.max_line_length,
            allow_tabs,
            require_spaces_around_operators,
        }
    }
}
