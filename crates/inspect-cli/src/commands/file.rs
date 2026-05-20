use std::path::{Component, Path, PathBuf};

use clap::Args;
use sem_core::git::types::DiffScope;

use crate::formatters;
use crate::OutputFormat;
use inspect_core::analyze::{analyze, retain_entity_reviews};
use inspect_core::types::RiskLevel;

#[derive(Args)]
pub struct FileArgs {
    /// File path to inspect
    pub path: String,

    /// Output format
    #[arg(long, value_enum, default_value = "terminal")]
    pub format: OutputFormat,

    /// Minimum risk level to show
    #[arg(long)]
    pub min_risk: Option<String>,

    /// Show dependency context
    #[arg(long)]
    pub context: bool,

    /// Repository path
    #[arg(short = 'C', long, default_value = ".")]
    pub repo: PathBuf,
}

pub fn run(args: FileArgs) {
    let repo = args.repo.canonicalize().unwrap_or(args.repo.clone());

    // Use working tree diff (uncommitted changes)
    let scope = DiffScope::Working;

    match analyze(&repo, scope) {
        Ok(mut result) => {
            let target_path = repo_relative_path(&repo, &args.path);

            // Filter to only the specified file
            retain_entity_reviews(&mut result, |r| {
                normalize_path(Path::new(&r.file_path)) == target_path
            });

            if let Some(ref min) = args.min_risk {
                let min_level = match min.to_lowercase().as_str() {
                    "critical" => RiskLevel::Critical,
                    "high" => RiskLevel::High,
                    "medium" => RiskLevel::Medium,
                    _ => RiskLevel::Low,
                };
                retain_entity_reviews(&mut result, |r| r.risk_level >= min_level);
            }

            match args.format {
                OutputFormat::Terminal => formatters::terminal::print(&result, args.context),
                OutputFormat::Json => formatters::json::print(&result),
                OutputFormat::Markdown => formatters::markdown::print(&result, args.context),
            }
        }
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    }
}

fn repo_relative_path(repo: &Path, input: &str) -> String {
    let path = Path::new(input);
    let absolute_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        repo.join(path)
    };
    let canonical_path = canonicalize_existing_path(&absolute_path);

    if let Ok(relative_path) = canonical_path.strip_prefix(repo) {
        normalize_path(relative_path)
    } else if let Ok(relative_path) = absolute_path.strip_prefix(repo) {
        normalize_path(relative_path)
    } else {
        normalize_path(path)
    }
}

fn normalize_path(path: &Path) -> String {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if parts.last().is_some_and(|part| part != "..") {
                    parts.pop();
                } else {
                    parts.push("..".into());
                }
            }
            Component::Normal(part) => {
                parts.push(part.to_string_lossy().into_owned());
            }
            Component::Prefix(_) | Component::RootDir => {}
        }
    }
    parts.join("/")
}

fn canonicalize_existing_path(path: &Path) -> PathBuf {
    if let Ok(canonical_path) = path.canonicalize() {
        return canonical_path;
    }

    let mut suffix = PathBuf::new();
    let mut cursor = path;
    while let (Some(parent), Some(name)) = (cursor.parent(), cursor.file_name()) {
        suffix = Path::new(name).join(suffix);
        if let Ok(canonical_parent) = parent.canonicalize() {
            return canonical_parent.join(suffix);
        }
        cursor = parent;
    }

    path.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_relative_path_accepts_relative_paths() {
        let repo = Path::new("/tmp/repo");

        assert_eq!(repo_relative_path(repo, "src/../app.ts"), "app.ts");
    }

    #[test]
    fn repo_relative_path_accepts_absolute_paths_under_repo() {
        let repo = Path::new("/tmp/repo");

        assert_eq!(
            repo_relative_path(repo, "/tmp/repo/src/app.ts"),
            "src/app.ts"
        );
    }

    #[test]
    fn normalize_path_preserves_leading_parent_dirs() {
        assert_eq!(normalize_path(Path::new("../app.ts")), "../app.ts");
    }

    #[test]
    fn normalize_path_collapses_nested_parent_dirs() {
        assert_eq!(normalize_path(Path::new("src/../lib/../app.ts")), "app.ts");
        assert_eq!(normalize_path(Path::new("src/../../app.ts")), "../app.ts");
    }
}
