use std::collections::HashMap;

use clap::Args;
use serde::Deserialize;

use inspect_core::github::{CreateReview, GitHubClient, ReviewCommentInput};
use inspect_core::patch::{commentable_lines, parse_patch};

#[derive(Args)]
pub struct ReviewArgs {
    /// PR number
    pub number: u64,

    /// Remote repository (owner/repo)
    #[arg(long)]
    pub remote: String,

    /// Path to JSON file with review comments
    #[arg(long)]
    pub comments_file: String,
}

#[derive(Deserialize)]
struct ReviewInput {
    #[serde(default = "default_body")]
    body: String,
    comments: Vec<CommentInput>,
}

#[derive(Deserialize)]
struct CommentInput {
    path: String,
    line: u64,
    body: String,
    #[serde(default)]
    start_line: Option<u64>,
}

fn default_body() -> String {
    "Review from inspect".to_string()
}

pub async fn run(args: ReviewArgs) {
    let client = match GitHubClient::new() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };

    eprintln!(
        "Fetching PR #{} from {} with patches...",
        args.number, args.remote
    );

    let pr = match client.get_pr_with_patches(&args.remote, args.number).await {
        Ok(pr) => pr,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };

    let file_commentable: HashMap<String, Vec<u64>> = pr
        .files
        .iter()
        .map(|f| {
            let hunks = f.patch.as_deref().map(parse_patch).unwrap_or_default();
            let cl = commentable_lines(&hunks);
            (f.filename.clone(), cl)
        })
        .collect();

    let raw = match std::fs::read_to_string(&args.comments_file) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: failed to read {}: {}", args.comments_file, e);
            std::process::exit(1);
        }
    };

    let input: ReviewInput = match serde_json::from_str(&raw) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("error: failed to parse {}: {}", args.comments_file, e);
            std::process::exit(1);
        }
    };

    let mut warnings = Vec::new();
    let mut valid_comments = Vec::new();

    for c in &input.comments {
        if let Some(cl) = file_commentable.get(&c.path) {
            if cl.contains(&c.line) {
                valid_comments.push(ReviewCommentInput {
                    path: c.path.clone(),
                    line: c.line,
                    body: c.body.clone(),
                    start_line: c.start_line,
                });
            } else {
                warnings.push(format!(
                    "SKIP: {}:{} is not a commentable line (not in diff)",
                    c.path, c.line
                ));
            }
        } else {
            warnings.push(format!(
                "SKIP: {} is not a changed file in this PR",
                c.path
            ));
        }
    }

    if !warnings.is_empty() {
        for w in &warnings {
            eprintln!("  {w}");
        }
    }

    if valid_comments.is_empty() {
        eprintln!("error: no valid comments to post after validation");
        std::process::exit(1);
    }

    let review = CreateReview {
        commit_id: pr.head_sha,
        event: "COMMENT".to_string(),
        body: input.body,
        comments: valid_comments,
    };

    match client.create_review(&args.remote, args.number, &review).await {
        Ok(resp) => {
            println!(
                "{}",
                serde_json::json!({ "id": resp.id, "url": resp.html_url })
            );
        }
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    }
}
