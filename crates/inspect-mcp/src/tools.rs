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
