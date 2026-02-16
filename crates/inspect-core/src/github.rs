use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, USER_AGENT};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::fmt;

// --- Error type ---

#[derive(Debug)]
pub enum GitHubError {
    Auth(String),
    Api(String),
    Parse(String),
}

impl fmt::Display for GitHubError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Auth(msg) => write!(f, "auth error: {}", msg),
            Self::Api(msg) => write!(f, "GitHub API error: {}", msg),
            Self::Parse(msg) => write!(f, "parse error: {}", msg),
        }
    }
}

impl std::error::Error for GitHubError {}

impl From<reqwest::Error> for GitHubError {
    fn from(e: reqwest::Error) -> Self {
        Self::Api(e.to_string())
    }
}

// --- GraphQL internal types ---

#[derive(Debug, Deserialize)]
struct GraphQLResponse<T> {
    data: Option<T>,
    errors: Option<Vec<GraphQLError>>,
}

#[derive(Debug, Deserialize)]
struct GraphQLError {
    message: String,
}

#[derive(Debug, Deserialize)]
struct RepositoryData {
    repository: RepositoryNode,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RepositoryNode {
    pull_request: GraphQLPullRequest,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GraphQLPullRequest {
    number: u64,
    title: String,
    body: Option<String>,
    state: String,
    additions: u64,
    deletions: u64,
    changed_files: u64,
    head_ref_name: String,
    base_ref_name: String,
    head_ref_oid: String,
    files: FileConnection,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileConnection {
    page_info: PageInfo,
    nodes: Vec<GraphQLPrFile>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageInfo {
    has_next_page: bool,
    end_cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GraphQLPrFile {
    path: String,
    additions: u64,
    deletions: u64,
    change_type: String,
}

#[derive(Debug, Deserialize)]
struct FilesPageData {
    repository: FilesPageRepository,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FilesPageRepository {
    pull_request: FilesPagePR,
}

#[derive(Debug, Deserialize)]
struct FilesPagePR {
    files: FileConnection,
}

#[derive(Debug, Deserialize)]
struct FileContent {
    content: Option<String>,
}

// --- Public types ---

#[derive(Debug, Clone)]
pub struct PullRequest {
    pub number: u64,
    pub title: String,
    pub body: Option<String>,
    pub state: String,
    pub additions: u64,
    pub deletions: u64,
    pub changed_files: u64,
    pub head_ref: String,
    pub base_ref: String,
    pub head_sha: String,
    pub files: Vec<PrFile>,
}

#[derive(Debug, Clone)]
pub struct PrFile {
    pub filename: String,
    pub status: String,
    pub additions: u64,
    pub deletions: u64,
    pub patch: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FilePair {
    pub filename: String,
    pub status: String,
    pub before_content: Option<String>,
    pub after_content: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateReview {
    pub commit_id: String,
    pub event: String,
    pub body: String,
    pub comments: Vec<ReviewCommentInput>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ReviewCommentInput {
    pub path: String,
    pub line: u64,
    pub body: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_line: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateReviewResponse {
    pub id: u64,
    pub html_url: String,
}

#[derive(Debug, Deserialize)]
pub struct CodeSearchResponse {
    pub total_count: u64,
    pub items: Vec<CodeSearchItem>,
}

#[derive(Debug, Deserialize)]
pub struct CodeSearchItem {
    pub name: String,
    pub path: String,
    pub repository: CodeSearchRepo,
    pub html_url: String,
    pub text_matches: Option<Vec<TextMatch>>,
}

#[derive(Debug, Deserialize)]
pub struct CodeSearchRepo {
    pub full_name: String,
}

#[derive(Debug, Deserialize)]
pub struct TextMatch {
    pub fragment: String,
    pub matches: Vec<TextMatchLocation>,
}

#[derive(Debug, Deserialize)]
pub struct TextMatchLocation {
    pub indices: Vec<u64>,
}

// --- Client ---

pub struct GitHubClient {
    http: reqwest::Client,
    base_url: String,
}

fn parse_raw_diff(raw: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    let mut current_file: Option<String> = None;
    let mut current_patch = String::new();

    for line in raw.lines() {
        if line.starts_with("diff --git ") {
            if let Some(file) = current_file.take() {
                if !current_patch.is_empty() {
                    map.insert(file, current_patch.trim_start_matches('\n').to_string());
                }
            }
            current_patch = String::new();
        } else if line.starts_with("+++ b/") {
            current_file = Some(line[6..].to_string());
        } else if line.starts_with("@@")
            || (current_file.is_some()
                && !line.starts_with("--- ")
                && !line.starts_with("+++ ")
                && !line.starts_with("index ")
                && !line.starts_with("new file")
                && !line.starts_with("deleted file")
                && !line.starts_with("old mode")
                && !line.starts_with("new mode")
                && !line.starts_with("similarity")
                && !line.starts_with("rename "))
        {
            if current_file.is_some() {
                if !current_patch.is_empty() {
                    current_patch.push('\n');
                }
                current_patch.push_str(line);
            }
        }
    }

    if let Some(file) = current_file {
        if !current_patch.is_empty() {
            map.insert(file, current_patch.trim_start_matches('\n').to_string());
        }
    }

    map
}

fn map_change_type(ct: &str) -> String {
    match ct {
        "ADDED" => "added".to_string(),
        "DELETED" | "REMOVED" => "removed".to_string(),
        "MODIFIED" | "CHANGED" => "modified".to_string(),
        "RENAMED" => "renamed".to_string(),
        "COPIED" => "copied".to_string(),
        other => other.to_lowercase(),
    }
}

fn split_repo(repo: &str) -> Result<(&str, &str), GitHubError> {
    repo.split_once('/')
        .ok_or_else(|| GitHubError::Parse(format!("Repository must be owner/repo, got: {repo}")))
}

impl GitHubClient {
    pub fn new() -> Result<Self, GitHubError> {
        let token = std::env::var("GITHUB_TOKEN")
            .or_else(|_| Self::token_from_gh_cli())
            .map_err(|e| GitHubError::Auth(format!("Set GITHUB_TOKEN or install/auth gh CLI: {e}")))?;

        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {token}"))
                .map_err(|e| GitHubError::Auth(e.to_string()))?,
        );
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/vnd.github+json"),
        );
        headers.insert(USER_AGENT, HeaderValue::from_static("inspect/0.1"));
        headers.insert(
            "X-GitHub-Api-Version",
            HeaderValue::from_static("2022-11-28"),
        );

        let http = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .map_err(|e| GitHubError::Api(e.to_string()))?;

        Ok(Self {
            http,
            base_url: "https://api.github.com".to_string(),
        })
    }

    fn token_from_gh_cli() -> Result<String, String> {
        let output = std::process::Command::new("gh")
            .args(["auth", "token"])
            .output()
            .map_err(|e| format!("Failed to run `gh auth token`: {e}"))?;
        if !output.status.success() {
            return Err("gh auth token failed".into());
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    async fn graphql<T: DeserializeOwned>(
        &self,
        query: &str,
        variables: &serde_json::Value,
    ) -> Result<T, GitHubError> {
        let body = serde_json::json!({
            "query": query,
            "variables": variables,
        });
        let url = format!("{}/graphql", self.base_url);
        let resp = self.http.post(&url).json(&body).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(GitHubError::Api(format!("{status}: {text}")));
        }
        let gql_resp: GraphQLResponse<T> = resp
            .json()
            .await
            .map_err(|e| GitHubError::Parse(e.to_string()))?;
        if let Some(errors) = gql_resp.errors {
            let msgs: Vec<String> = errors.into_iter().map(|e| e.message).collect();
            return Err(GitHubError::Api(format!("GraphQL: {}", msgs.join("; "))));
        }
        gql_resp
            .data
            .ok_or_else(|| GitHubError::Api("No data in GraphQL response".into()))
    }

    async fn rest_get<T: DeserializeOwned>(&self, path: &str) -> Result<T, GitHubError> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self.http.get(&url).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(GitHubError::Api(format!("{status}: {body}")));
        }
        resp.json()
            .await
            .map_err(|e| GitHubError::Parse(e.to_string()))
    }

    async fn rest_post<B: Serialize, R: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<R, GitHubError> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self.http.post(&url).json(body).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(GitHubError::Api(format!("{status}: {body}")));
        }
        resp.json()
            .await
            .map_err(|e| GitHubError::Parse(e.to_string()))
    }

    pub async fn get_pr(&self, repo: &str, number: u64) -> Result<PullRequest, GitHubError> {
        let (owner, name) = split_repo(repo)?;

        const QUERY: &str = r#"
query($owner: String!, $repo: String!, $number: Int!) {
  repository(owner: $owner, name: $repo) {
    pullRequest(number: $number) {
      number
      title
      body
      state
      additions
      deletions
      changedFiles
      headRefName
      baseRefName
      headRefOid
      files(first: 100) {
        pageInfo { hasNextPage endCursor }
        nodes {
          path
          additions
          deletions
          changeType
        }
      }
    }
  }
}
"#;

        let vars = serde_json::json!({
            "owner": owner,
            "repo": name,
            "number": number as i64,
        });

        let data: RepositoryData = self.graphql(QUERY, &vars).await?;
        let pr = data.repository.pull_request;

        let mut files: Vec<PrFile> = pr
            .files
            .nodes
            .iter()
            .map(|f| PrFile {
                filename: f.path.clone(),
                status: map_change_type(&f.change_type),
                additions: f.additions,
                deletions: f.deletions,
                patch: None,
            })
            .collect();

        let mut page_info = pr.files.page_info;
        while page_info.has_next_page {
            let cursor = page_info.end_cursor.as_deref().unwrap_or_default();
            let more = self
                .get_pr_files_page(owner, name, number, cursor)
                .await?;
            for f in &more.nodes {
                files.push(PrFile {
                    filename: f.path.clone(),
                    status: map_change_type(&f.change_type),
                    additions: f.additions,
                    deletions: f.deletions,
                    patch: None,
                });
            }
            page_info = more.page_info;
        }

        Ok(PullRequest {
            number: pr.number,
            title: pr.title,
            body: pr.body,
            state: pr.state,
            additions: pr.additions,
            deletions: pr.deletions,
            changed_files: pr.changed_files,
            head_ref: pr.head_ref_name,
            base_ref: pr.base_ref_name,
            head_sha: pr.head_ref_oid,
            files,
        })
    }

    async fn get_pr_files_page(
        &self,
        owner: &str,
        name: &str,
        number: u64,
        cursor: &str,
    ) -> Result<FileConnection, GitHubError> {
        const QUERY: &str = r#"
query($owner: String!, $repo: String!, $number: Int!, $cursor: String!) {
  repository(owner: $owner, name: $repo) {
    pullRequest(number: $number) {
      files(first: 100, after: $cursor) {
        pageInfo { hasNextPage endCursor }
        nodes {
          path
          additions
          deletions
          changeType
        }
      }
    }
  }
}
"#;
        let vars = serde_json::json!({
            "owner": owner,
            "repo": name,
            "number": number as i64,
            "cursor": cursor,
        });

        let data: FilesPageData = self.graphql(QUERY, &vars).await?;
        Ok(data.repository.pull_request.files)
    }

    async fn get_pr_raw_diff(&self, repo: &str, number: u64) -> Result<String, GitHubError> {
        let url = format!("{}/repos/{}/pulls/{}", self.base_url, repo, number);
        let resp = self
            .http
            .get(&url)
            .header(ACCEPT, "application/vnd.github.diff")
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(GitHubError::Api(format!("{status}: {body}")));
        }
        resp.text()
            .await
            .map_err(|e| GitHubError::Parse(e.to_string()))
    }

    pub async fn get_pr_with_patches(
        &self,
        repo: &str,
        number: u64,
    ) -> Result<PullRequest, GitHubError> {
        let (pr, raw_diff) = tokio::try_join!(
            self.get_pr(repo, number),
            self.get_pr_raw_diff(repo, number),
        )?;

        let patch_map = parse_raw_diff(&raw_diff);

        let files = pr
            .files
            .into_iter()
            .map(|mut f| {
                if let Some(patch) = patch_map.get(&f.filename) {
                    f.patch = Some(patch.clone());
                }
                f
            })
            .collect();

        Ok(PullRequest { files, ..pr })
    }

    async fn get_file_content(
        &self,
        repo: &str,
        path: &str,
        git_ref: &str,
    ) -> Result<String, GitHubError> {
        let encoded_path = urlencoding::encode(path);
        let fc: FileContent = self
            .rest_get(&format!(
                "/repos/{repo}/contents/{encoded_path}?ref={git_ref}"
            ))
            .await?;
        let encoded = fc.content.unwrap_or_default();
        let cleaned: String = encoded.chars().filter(|c| !c.is_whitespace()).collect();
        let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &cleaned)
            .map_err(|e| GitHubError::Parse(format!("base64 decode: {e}")))?;
        String::from_utf8(bytes).map_err(|e| GitHubError::Parse(format!("utf8: {e}")))
    }

    pub async fn get_file_pairs(
        &self,
        repo: &str,
        files: &[PrFile],
        base_ref: &str,
        head_ref: &str,
    ) -> Vec<FilePair> {
        let futs: Vec<_> = files
            .iter()
            .map(|f| {
                let filename = f.filename.clone();
                let status = f.status.clone();
                let repo = repo.to_string();
                let base = base_ref.to_string();
                let head = head_ref.to_string();

                async move {
                    let before = if status == "added" {
                        None
                    } else {
                        self.get_file_content(&repo, &filename, &base).await.ok()
                    };

                    let after = if status == "removed" {
                        None
                    } else {
                        self.get_file_content(&repo, &filename, &head).await.ok()
                    };

                    FilePair {
                        filename,
                        status,
                        before_content: before,
                        after_content: after,
                    }
                }
            })
            .collect();

        futures::future::join_all(futs).await
    }

    pub async fn search_code(
        &self,
        repo: &str,
        query: &str,
        path_prefix: Option<&str>,
    ) -> Result<CodeSearchResponse, GitHubError> {
        let mut q = format!("{} repo:{}", query, repo);
        if let Some(prefix) = path_prefix {
            q.push_str(&format!(" path:{}", prefix));
        }

        let encoded_q = urlencoding::encode(&q);
        let url = format!("{}/search/code?q={}&per_page=100", self.base_url, encoded_q);

        let resp = self
            .http
            .get(&url)
            .header(ACCEPT, "application/vnd.github.text-match+json")
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(GitHubError::Api(format!("Code Search {status}: {body}")));
        }

        resp.json()
            .await
            .map_err(|e| GitHubError::Parse(e.to_string()))
    }

    pub async fn create_review(
        &self,
        repo: &str,
        number: u64,
        review: &CreateReview,
    ) -> Result<CreateReviewResponse, GitHubError> {
        self.rest_post(
            &format!("/repos/{repo}/pulls/{number}/reviews"),
            review,
        )
        .await
    }

    pub async fn fetch_file_contents(
        &self,
        repo: &str,
        paths: &[String],
        git_ref: &str,
    ) -> Vec<(String, String)> {
        let futs: Vec<_> = paths
            .iter()
            .map(|path| {
                let path = path.clone();
                let repo = repo.to_string();
                let git_ref = git_ref.to_string();
                async move {
                    match self.get_file_content(&repo, &path, &git_ref).await {
                        Ok(content) => Some((path, content)),
                        Err(_) => None,
                    }
                }
            })
            .collect();

        futures::future::join_all(futs)
            .await
            .into_iter()
            .flatten()
            .collect()
    }
}
