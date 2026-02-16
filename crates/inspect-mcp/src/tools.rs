use serde::Deserialize;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TriageParams {
    #[schemars(description = "Absolute path to the git repository")]
    pub repo_path: String,
    #[schemars(description = "What to analyze: a commit ref (e.g. 'HEAD~1'), a range ('main..feature'), or 'working' for uncommitted changes")]
    pub target: String,
    #[schemars(description = "Minimum risk level to include: 'low', 'medium', 'high', or 'critical'")]
    pub min_risk: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct EntityParams {
    #[schemars(description = "Absolute path to the git repository")]
    pub repo_path: String,
    #[schemars(description = "What to analyze: commit ref, range, or 'working'")]
    pub target: String,
    #[schemars(description = "Name of the entity to inspect")]
    pub entity_name: String,
    #[schemars(description = "File path to disambiguate entities with the same name")]
    pub file_path: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GroupParams {
    #[schemars(description = "Absolute path to the git repository")]
    pub repo_path: String,
    #[schemars(description = "What to analyze: commit ref, range, or 'working'")]
    pub target: String,
    #[schemars(description = "Group ID to inspect")]
    pub group_id: usize,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FileParams {
    #[schemars(description = "Absolute path to the git repository")]
    pub repo_path: String,
    #[schemars(description = "What to analyze: commit ref, range, or 'working'")]
    pub target: String,
    #[schemars(description = "File path to scope the review to")]
    pub file_path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct StatsParams {
    #[schemars(description = "Absolute path to the git repository")]
    pub repo_path: String,
    #[schemars(description = "What to analyze: commit ref, range, or 'working'")]
    pub target: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RiskMapParams {
    #[schemars(description = "Absolute path to the git repository")]
    pub repo_path: String,
    #[schemars(description = "What to analyze: commit ref, range, or 'working'")]
    pub target: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RemoteTriageParams {
    #[schemars(description = "GitHub repository in owner/repo format (e.g. 'facebook/react')")]
    pub repo: String,
    #[schemars(description = "PR number to analyze")]
    pub pr_number: u64,
    #[schemars(description = "Minimum risk level to include: 'low', 'medium', 'high', or 'critical'")]
    pub min_risk: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PostReviewParams {
    #[schemars(description = "GitHub repository in owner/repo format")]
    pub repo: String,
    #[schemars(description = "PR number to post review on")]
    pub pr_number: u64,
    #[schemars(description = "Overall review body text")]
    pub body: Option<String>,
    #[schemars(description = "Review comments to post. Each has: path (file), line (number), body (text), start_line (optional, for multi-line)")]
    pub comments: Vec<ReviewComment>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ReviewComment {
    #[schemars(description = "File path relative to repo root")]
    pub path: String,
    #[schemars(description = "Line number in the new file (must be in the diff)")]
    pub line: u64,
    #[schemars(description = "Comment body text")]
    pub body: String,
    #[schemars(description = "Start line for multi-line comments")]
    pub start_line: Option<u64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchParams {
    #[schemars(description = "GitHub repository in owner/repo format")]
    pub repo: String,
    #[schemars(description = "PR number whose files to search")]
    pub pr_number: u64,
    #[schemars(description = "Text pattern to search for")]
    pub pattern: String,
    #[schemars(description = "Also search the broader codebase via GitHub Code Search")]
    pub repo_wide: Option<bool>,
    #[schemars(description = "Case-sensitive search (default: false)")]
    pub case_sensitive: Option<bool>,
}
