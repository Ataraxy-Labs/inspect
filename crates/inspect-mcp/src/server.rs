use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, Content, ServerCapabilities, ServerInfo};
use rmcp::{tool, tool_handler, tool_router, ServerHandler};
use sem_core::git::types::DiffScope;
use tokio::sync::Mutex;

use inspect_core::analyze::{analyze, AnalyzeError};
use inspect_core::risk::suggest_verdict;
use inspect_core::types::{ReviewResult, RiskLevel};

use crate::tools::*;

/// Cached analysis result keyed by (repo_path, target).
struct CachedResult {
    key: (String, String),
    result: ReviewResult,
}

#[derive(Clone)]
pub struct InspectServer {
    cache: Arc<Mutex<Option<CachedResult>>>,
    tool_router: ToolRouter<Self>,
}

fn parse_scope(target: &str) -> DiffScope {
    if target == "working" {
        DiffScope::Working
    } else if target.contains("..") {
        let parts: Vec<&str> = target.split("..").collect();
        DiffScope::Range {
            from: parts[0].to_string(),
            to: parts[1].to_string(),
        }
    } else {
        DiffScope::Commit {
            sha: target.to_string(),
        }
    }
}

fn parse_risk_level(s: &str) -> RiskLevel {
    match s.to_lowercase().as_str() {
        "critical" => RiskLevel::Critical,
        "high" => RiskLevel::High,
        "medium" => RiskLevel::Medium,
        _ => RiskLevel::Low,
    }
}

fn internal_err(msg: impl ToString) -> rmcp::ErrorData {
    rmcp::ErrorData::internal_error(msg.to_string(), None)
}

impl InspectServer {
    /// Run analysis, using cache if the key matches.
    async fn get_result(
        &self,
        repo_path: &str,
        target: &str,
    ) -> Result<ReviewResult, AnalyzeError> {
        // Check cache
        {
            let cache = self.cache.lock().await;
            if let Some(cached) = cache.as_ref() {
                if cached.key.0 == repo_path && cached.key.1 == target {
                    return Ok(cached.result.clone());
                }
            }
        }

        // Run analysis in a blocking task (CPU-bound)
        let repo = PathBuf::from(repo_path);
        let scope = parse_scope(target);
        let result =
            tokio::task::spawn_blocking(move || analyze(&repo, scope))
                .await
                .map_err(|e| AnalyzeError::Git(format!("spawn_blocking failed: {}", e)))??;

        // Cache it
        {
            let mut cache = self.cache.lock().await;
            *cache = Some(CachedResult {
                key: (repo_path.to_string(), target.to_string()),
                result: result.clone(),
            });
        }

        Ok(result)
    }
}

#[tool_router]
impl InspectServer {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(Mutex::new(None)),
            tool_router: Self::tool_router(),
        }
    }

    #[tool(description = "Run entity-level code review triage. Returns a compact summary of all changed entities sorted by risk score, with classification, blast radius, and logical grouping. This is the primary entry point for understanding what changed and where to focus review effort.")]
    async fn inspect_triage(
        &self,
        Parameters(params): Parameters<TriageParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let result = self
            .get_result(&params.repo_path, &params.target)
            .await
            .map_err(internal_err)?;

        let verdict = suggest_verdict(&result);

        let entities: Vec<serde_json::Value> = result
            .entity_reviews
            .iter()
            .filter(|r| {
                if let Some(ref min) = params.min_risk {
                    r.risk_level >= parse_risk_level(min)
                } else {
                    true
                }
            })
            .map(|r| {
                serde_json::json!({
                    "name": r.entity_name,
                    "type": r.entity_type,
                    "file": r.file_path,
                    "risk": format!("{}", r.risk_level),
                    "score": format!("{:.2}", r.risk_score),
                    "classification": format!("{}", r.classification),
                    "blast_radius": r.blast_radius,
                    "change_type": format!("{:?}", r.change_type).to_lowercase(),
                    "public_api": r.is_public_api,
                    "cosmetic": r.structural_change == Some(false),
                    "group_id": r.group_id,
                })
            })
            .collect();

        let groups: Vec<serde_json::Value> = result
            .groups
            .iter()
            .map(|g| {
                serde_json::json!({
                    "id": g.id,
                    "label": g.label,
                    "entity_count": g.entity_ids.len(),
                })
            })
            .collect();

        let output = serde_json::json!({
            "verdict": format!("{}", verdict),
            "stats": {
                "total_entities": result.stats.total_entities,
                "critical": result.stats.by_risk.critical,
                "high": result.stats.by_risk.high,
                "medium": result.stats.by_risk.medium,
                "low": result.stats.by_risk.low,
            },
            "entities": entities,
            "groups": groups,
            "timing_ms": result.timing.total_ms,
        });

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&output).unwrap_or_default(),
        )]))
    }

    #[tool(description = "Drill into a single entity to see full details including before/after content, dependents, and dependencies. Use after inspect_triage to understand a specific high-risk entity.")]
    async fn inspect_entity(
        &self,
        Parameters(params): Parameters<EntityParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let result = self
            .get_result(&params.repo_path, &params.target)
            .await
            .map_err(internal_err)?;

        let review = result
            .entity_reviews
            .iter()
            .find(|r| {
                r.entity_name == params.entity_name
                    && params
                        .file_path
                        .as_ref()
                        .map(|fp| r.file_path.ends_with(fp))
                        .unwrap_or(true)
            })
            .ok_or_else(|| {
                internal_err(format!("Entity '{}' not found in changes", params.entity_name))
            })?;

        let output = serde_json::json!({
            "entity_id": review.entity_id,
            "name": review.entity_name,
            "type": review.entity_type,
            "file": review.file_path,
            "lines": format!("{}-{}", review.start_line, review.end_line),
            "change_type": format!("{:?}", review.change_type).to_lowercase(),
            "classification": format!("{}", review.classification),
            "risk": format!("{}", review.risk_level),
            "score": format!("{:.2}", review.risk_score),
            "blast_radius": review.blast_radius,
            "public_api": review.is_public_api,
            "cosmetic": review.structural_change == Some(false),
            "group_id": review.group_id,
            "before_content": review.before_content,
            "after_content": review.after_content,
            "dependents": review.dependent_names.iter().map(|(name, file)| {
                serde_json::json!({"name": name, "file": file})
            }).collect::<Vec<_>>(),
            "dependencies": review.dependency_names.iter().map(|(name, file)| {
                serde_json::json!({"name": name, "file": file})
            }).collect::<Vec<_>>(),
        });

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&output).unwrap_or_default(),
        )]))
    }

    #[tool(description = "Get all entities in a logical change group. Groups are formed by dependency edges between changed entities. Use after inspect_triage to understand related changes.")]
    async fn inspect_group(
        &self,
        Parameters(params): Parameters<GroupParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let result = self
            .get_result(&params.repo_path, &params.target)
            .await
            .map_err(internal_err)?;

        let group = result
            .groups
            .iter()
            .find(|g| g.id == params.group_id)
            .ok_or_else(|| internal_err(format!("Group {} not found", params.group_id)))?;

        let entities: Vec<serde_json::Value> = result
            .entity_reviews
            .iter()
            .filter(|r| group.entity_ids.contains(&r.entity_id))
            .map(|r| {
                serde_json::json!({
                    "name": r.entity_name,
                    "type": r.entity_type,
                    "file": r.file_path,
                    "risk": format!("{}", r.risk_level),
                    "score": format!("{:.2}", r.risk_score),
                    "classification": format!("{}", r.classification),
                    "change_type": format!("{:?}", r.change_type).to_lowercase(),
                })
            })
            .collect();

        let output = serde_json::json!({
            "group_id": group.id,
            "label": group.label,
            "entity_count": group.entity_ids.len(),
            "entities": entities,
        });

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&output).unwrap_or_default(),
        )]))
    }

    #[tool(description = "Scope review to a single file. Returns entity reviews for only the specified file path.")]
    async fn inspect_file(
        &self,
        Parameters(params): Parameters<FileParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let result = self
            .get_result(&params.repo_path, &params.target)
            .await
            .map_err(internal_err)?;

        let entities: Vec<serde_json::Value> = result
            .entity_reviews
            .iter()
            .filter(|r| r.file_path.ends_with(&params.file_path))
            .map(|r| {
                serde_json::json!({
                    "name": r.entity_name,
                    "type": r.entity_type,
                    "file": r.file_path,
                    "lines": format!("{}-{}", r.start_line, r.end_line),
                    "risk": format!("{}", r.risk_level),
                    "score": format!("{:.2}", r.risk_score),
                    "classification": format!("{}", r.classification),
                    "blast_radius": r.blast_radius,
                    "change_type": format!("{:?}", r.change_type).to_lowercase(),
                    "public_api": r.is_public_api,
                    "cosmetic": r.structural_change == Some(false),
                })
            })
            .collect();

        let output = serde_json::json!({
            "file": params.file_path,
            "entity_count": entities.len(),
            "entities": entities,
        });

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&output).unwrap_or_default(),
        )]))
    }

    #[tool(description = "Lightweight summary with no entity details. Returns stats, group count, verdict, and timing.")]
    async fn inspect_stats(
        &self,
        Parameters(params): Parameters<StatsParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let result = self
            .get_result(&params.repo_path, &params.target)
            .await
            .map_err(internal_err)?;

        let verdict = suggest_verdict(&result);

        let output = serde_json::json!({
            "verdict": format!("{}", verdict),
            "total_entities": result.stats.total_entities,
            "risk": {
                "critical": result.stats.by_risk.critical,
                "high": result.stats.by_risk.high,
                "medium": result.stats.by_risk.medium,
                "low": result.stats.by_risk.low,
            },
            "classification": {
                "text": result.stats.by_classification.text,
                "syntax": result.stats.by_classification.syntax,
                "functional": result.stats.by_classification.functional,
                "mixed": result.stats.by_classification.mixed,
            },
            "change_types": {
                "added": result.stats.by_change_type.added,
                "modified": result.stats.by_change_type.modified,
                "deleted": result.stats.by_change_type.deleted,
                "moved": result.stats.by_change_type.moved,
                "renamed": result.stats.by_change_type.renamed,
            },
            "groups": result.groups.len(),
            "timing_ms": result.timing.total_ms,
        });

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&output).unwrap_or_default(),
        )]))
    }

    #[tool(description = "File-level risk heatmap. Returns per-file aggregate risk showing max risk, entity count, critical/high counts, and public API changes. Sorted by max risk descending.")]
    async fn inspect_risk_map(
        &self,
        Parameters(params): Parameters<RiskMapParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let result = self
            .get_result(&params.repo_path, &params.target)
            .await
            .map_err(internal_err)?;

        // Aggregate per file
        let mut file_map: HashMap<String, FileRisk> = HashMap::new();
        for r in &result.entity_reviews {
            let entry = file_map.entry(r.file_path.clone()).or_insert(FileRisk {
                max_score: 0.0,
                max_risk: RiskLevel::Low,
                entity_count: 0,
                critical: 0,
                high: 0,
                public_api_changes: 0,
            });
            entry.entity_count += 1;
            if r.risk_score > entry.max_score {
                entry.max_score = r.risk_score;
                entry.max_risk = r.risk_level;
            }
            if r.risk_level == RiskLevel::Critical {
                entry.critical += 1;
            }
            if r.risk_level == RiskLevel::High {
                entry.high += 1;
            }
            if r.is_public_api {
                entry.public_api_changes += 1;
            }
        }

        let mut files: Vec<(String, &FileRisk)> = file_map.iter().map(|(k, v)| (k.clone(), v)).collect();
        files.sort_by(|a, b| b.1.max_score.partial_cmp(&a.1.max_score).unwrap());

        let output: Vec<serde_json::Value> = files
            .iter()
            .map(|(path, risk)| {
                serde_json::json!({
                    "file": path,
                    "max_risk": format!("{}", risk.max_risk),
                    "max_score": format!("{:.2}", risk.max_score),
                    "entity_count": risk.entity_count,
                    "critical": risk.critical,
                    "high": risk.high,
                    "public_api_changes": risk.public_api_changes,
                })
            })
            .collect();

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&output).unwrap_or_default(),
        )]))
    }
}

struct FileRisk {
    max_score: f64,
    max_risk: RiskLevel,
    entity_count: usize,
    critical: usize,
    high: usize,
    public_api_changes: usize,
}

#[tool_handler]
impl ServerHandler for InspectServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Entity-level code review triage server. Use inspect_triage as the primary entry \
                 point to understand what changed and where to focus review. Drill down with \
                 inspect_entity, inspect_group, or inspect_file for details."
                    .into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}
