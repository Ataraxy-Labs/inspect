use std::path::Path;
use std::process::Command;

use clap::ValueEnum;
use sem_core::git::types::DiffScope;
use serde::Deserialize;

use inspect_core::analyze::{analyze, analyze_remote};
use inspect_core::github::GitHubClient;
use inspect_core::noise::is_noise_file;
use inspect_core::types::ReviewResult;

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum FetchSource {
    /// Read changes from local git refs.
    Local,
    /// Fetch PR files through the GitHub API.
    Github,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhPrView {
    base_ref_name: Option<String>,
    head_ref_name: Option<String>,
    base_ref_oid: Option<String>,
    head_ref_oid: Option<String>,
}

pub fn analyze_explicit_range(
    repo: &Path,
    base: Option<&str>,
    head: Option<&str>,
) -> Result<Option<ReviewResult>, String> {
    let Some(base) = base else {
        if head.is_some() {
            return Err("--head requires --base".to_string());
        }
        return Ok(None);
    };

    let Some(head) = head else {
        return Err("--base requires --head".to_string());
    };

    analyze(
        repo,
        DiffScope::Range {
            from: base.to_string(),
            to: head.to_string(),
        },
    )
    .map(Some)
    .map_err(|e| e.to_string())
}

pub fn analyze_local_pr(repo: &Path, number: u64) -> Result<ReviewResult, String> {
    let pr = load_gh_pr_view(repo, number)?;
    let base = pr
        .base_ref_oid
        .as_deref()
        .or(pr.base_ref_name.as_deref())
        .unwrap_or("main");
    let head = pr
        .head_ref_oid
        .as_deref()
        .or(pr.head_ref_name.as_deref())
        .unwrap_or("HEAD");

    ensure_local_pr_commits(repo, number, &pr, base, head)?;

    analyze(
        repo,
        DiffScope::Range {
            from: base.to_string(),
            to: head.to_string(),
        },
    )
    .map_err(|e| {
        format!(
            "{}\nTry `inspect pr {} --fetch github --remote owner/repo` to fetch PR files through the GitHub API.",
            e, number
        )
    })
}

pub async fn analyze_github_pr(remote_repo: &str, number: u64) -> Result<ReviewResult, String> {
    let client = GitHubClient::new().map_err(|e| e.to_string())?;

    eprintln!(
        "Fetching PR #{} from {} via GitHub API...",
        number, remote_repo
    );

    let pr = client
        .get_pr(remote_repo, number)
        .await
        .map_err(|e| e.to_string())?;

    let visible_files: Vec<_> = pr
        .files
        .iter()
        .filter(|f| !is_noise_file(&f.filename))
        .cloned()
        .collect();

    let noise_count = pr.files.len() - visible_files.len();
    if noise_count > 0 {
        eprintln!("({} noise files hidden)", noise_count);
    }

    eprintln!("Fetching {} file contents...", visible_files.len());

    // Use commit SHAs instead of branch names so fork PRs work even when the
    // head branch does not exist on the base repository.
    let file_pairs = client
        .get_file_pairs(remote_repo, &visible_files, &pr.base_sha, &pr.head_sha)
        .await;

    analyze_remote(&file_pairs).map_err(|e| e.to_string())
}

fn load_gh_pr_view(repo: &Path, number: u64) -> Result<GhPrView, String> {
    let output = Command::new("gh")
        .args([
            "pr",
            "view",
            &number.to_string(),
            "--json",
            "baseRefName,headRefName,baseRefOid,headRefOid",
        ])
        .current_dir(repo)
        .output()
        .map_err(|e| format!("could not run gh CLI: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "gh pr view failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    serde_json::from_slice(&output.stdout).map_err(|e| format!("invalid gh output: {e}"))
}

fn ensure_local_pr_commits(
    repo: &Path,
    number: u64,
    pr: &GhPrView,
    base: &str,
    head: &str,
) -> Result<(), String> {
    if commit_exists(repo, base) && commit_exists(repo, head) {
        return Ok(());
    }

    let mut fetch_errors = Vec::new();

    if !commit_exists(repo, base) {
        if let Some(base_ref) = pr.base_ref_name.as_deref() {
            if let Err(e) = fetch_ref(repo, base_ref) {
                fetch_errors.push(e);
            }
        }
    }

    let pr_fetch_ref = pr_head_refspec(number);
    if !commit_exists(repo, head) {
        if let Err(e) = fetch_ref(repo, &pr_fetch_ref) {
            fetch_errors.push(e);
        }
    }

    if commit_exists(repo, base) && commit_exists(repo, head) {
        return Ok(());
    }

    let mut commands = Vec::new();
    if !commit_exists(repo, base) {
        if let Some(base_ref) = pr.base_ref_name.as_deref() {
            commands.push(format!("git fetch origin {base_ref}"));
        }
    }
    if !commit_exists(repo, head) {
        commands.push(format!("git fetch origin {pr_fetch_ref}"));
    }

    Err(missing_commits_error(number, &commands, &fetch_errors))
}

fn pr_head_refspec(number: u64) -> String {
    format!("+pull/{number}/head:refs/remotes/inspect/pr-{number}")
}

fn missing_commits_error(number: u64, commands: &[String], fetch_errors: &[String]) -> String {
    let command_hint = if commands.is_empty() {
        "fetch the missing PR commits locally".to_string()
    } else {
        commands.join("\n  ")
    };

    let fetch_error_hint = if fetch_errors.is_empty() {
        String::new()
    } else {
        format!("\nFetch attempts failed:\n  {}", fetch_errors.join("\n  "))
    };

    format!(
        "PR #{number} commits are not available locally.\nRun:\n  {command_hint}{fetch_error_hint}\nOr use `--fetch github --remote owner/repo` to fetch file contents through GitHub."
    )
}

fn commit_exists(repo: &Path, rev: &str) -> bool {
    Command::new("git")
        .args(["cat-file", "-e", &format!("{rev}^{{commit}}")])
        .current_dir(repo)
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pr_head_refspec_forces_updates() {
        assert_eq!(
            pr_head_refspec(42),
            "+pull/42/head:refs/remotes/inspect/pr-42"
        );
    }

    #[test]
    fn missing_commits_error_includes_fetch_failures() {
        let message = missing_commits_error(
            42,
            &["git fetch origin +pull/42/head:refs/remotes/inspect/pr-42".to_string()],
            &["git fetch origin failed: permission denied".to_string()],
        );

        assert!(message.contains("git fetch origin +pull/42/head:refs/remotes/inspect/pr-42"));
        assert!(message.contains("Fetch attempts failed"));
        assert!(message.contains("permission denied"));
        assert!(message.contains("--fetch github --remote owner/repo"));
    }
}

fn fetch_ref(repo: &Path, refspec: &str) -> Result<(), String> {
    let output = Command::new("git")
        .args(["fetch", "origin", refspec])
        .current_dir(repo)
        .output()
        .map_err(|e| format!("could not run git fetch: {e}"))?;

    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "git fetch origin {refspec} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}
