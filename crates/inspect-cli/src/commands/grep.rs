use std::collections::HashSet;

use clap::Args;

use inspect_core::github::GitHubClient;
use inspect_core::noise::is_noise_file;
use inspect_core::search::{self, SearchMatch};

#[derive(Args)]
pub struct GrepArgs {
    /// PR number
    pub number: u64,

    /// Remote repository (owner/repo)
    #[arg(long)]
    pub remote: String,

    /// Search pattern
    #[arg(long)]
    pub pattern: String,

    /// Also search the broader codebase via GitHub Code Search
    #[arg(long)]
    pub repo_wide: bool,

    /// Case-sensitive search
    #[arg(long)]
    pub case_sensitive: bool,

    /// Context lines around matches
    #[arg(short = 'C', long, default_value = "0")]
    pub context: usize,
}

pub async fn run(args: GrepArgs) {
    let client = match GitHubClient::new() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };

    let pr = match client.get_pr(&args.remote, args.number).await {
        Ok(pr) => pr,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };

    let file_paths: Vec<String> = pr
        .files
        .iter()
        .filter(|f| !is_noise_file(&f.filename))
        .map(|f| f.filename.clone())
        .collect();

    eprintln!("Fetching {} PR files at {}...", file_paths.len(), pr.head_ref);
    let pr_files = client
        .fetch_file_contents(&args.remote, &file_paths, &pr.head_ref)
        .await;

    let mut all_matches =
        search::grep_files(&pr_files, &args.pattern, args.case_sensitive, args.context);

    if args.repo_wide {
        eprintln!("Searching codebase via GitHub Code Search...");
        match client
            .search_code(&args.remote, &args.pattern, None)
            .await
        {
            Ok(search_results) => {
                eprintln!(
                    "Code Search: {} results from default branch",
                    search_results.total_count
                );

                let pr_file_set: HashSet<&str> =
                    file_paths.iter().map(|s| s.as_str()).collect();

                for item in &search_results.items {
                    if pr_file_set.contains(item.path.as_str()) {
                        continue;
                    }
                    if is_noise_file(&item.path) {
                        continue;
                    }
                    if let Some(text_matches) = &item.text_matches {
                        for tm in text_matches {
                            for (line_idx, line) in tm.fragment.lines().enumerate() {
                                let haystack = if args.case_sensitive {
                                    line.to_string()
                                } else {
                                    line.to_lowercase()
                                };
                                let pat = if args.case_sensitive {
                                    args.pattern.clone()
                                } else {
                                    args.pattern.to_lowercase()
                                };
                                if haystack.contains(&pat) {
                                    all_matches.push(SearchMatch {
                                        file: item.path.clone(),
                                        line: line_idx + 1,
                                        column: haystack.find(&pat).unwrap_or(0) + 1,
                                        text: line.to_string(),
                                        context_before: vec![],
                                        context_after: vec![],
                                    });
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("Code Search failed: {}", e);
            }
        }
    }

    println!("{}", search::format_matches(&all_matches));
}
