mod commands;
mod formatters;

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(name = "inspect", about = "Entity-level code review")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Review entity-level changes between commits
    Diff(commands::diff::DiffArgs),
    /// Review changes in a GitHub pull request
    Pr(commands::pr::PrArgs),
    /// Review uncommitted changes in a file
    File(commands::file::FileArgs),
    /// Benchmark entity-level review across a repo's history
    Bench(commands::bench::BenchArgs),
    /// Post review comments on a GitHub PR
    Review(commands::review::ReviewArgs),
    /// Search PR files (and optionally the codebase) for a pattern
    Grep(commands::grep::GrepArgs),
}

#[derive(Clone, Copy, ValueEnum)]
pub enum OutputFormat {
    Terminal,
    Json,
    Markdown,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Diff(args) => commands::diff::run(args),
        Commands::Pr(args) => commands::pr::run(args).await,
        Commands::File(args) => commands::file::run(args),
        Commands::Bench(args) => commands::bench::run(args),
        Commands::Review(args) => commands::review::run(args).await,
        Commands::Grep(args) => commands::grep::run(args).await,
    }
}
