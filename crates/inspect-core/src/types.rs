use sem_core::model::change::{ChangeType, SemanticChange};
use serde::{Deserialize, Serialize};

/// ConGra change classification taxonomy.
/// Categorizes what dimension(s) of the code changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangeClassification {
    /// Only comments, whitespace, or documentation changed
    Text,
    /// Only signatures, types, or declarations changed (no logic)
    Syntax,
    /// Logic or behavior changed
    Functional,
    /// Comments + signature changes
    TextSyntax,
    /// Comments + logic changes
    TextFunctional,
    /// Signature + logic changes
    SyntaxFunctional,
    /// All three dimensions changed
    TextSyntaxFunctional,
}

impl std::fmt::Display for ChangeClassification {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Text => write!(f, "text"),
            Self::Syntax => write!(f, "syntax"),
            Self::Functional => write!(f, "functional"),
            Self::TextSyntax => write!(f, "text+syntax"),
            Self::TextFunctional => write!(f, "text+functional"),
            Self::SyntaxFunctional => write!(f, "syntax+functional"),
            Self::TextSyntaxFunctional => write!(f, "text+syntax+functional"),
        }
    }
}

/// Risk level for a changed entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

impl std::fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Low => write!(f, "low"),
            Self::Medium => write!(f, "medium"),
            Self::High => write!(f, "high"),
            Self::Critical => write!(f, "critical"),
        }
    }
}

/// Code of an entity that depends on a changed entity.
/// Provides precise function/method bodies from tree-sitter extraction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependentEntity {
    pub entity_name: String,
    pub entity_type: String,
    pub file_path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub content: String,
    pub own_dependent_count: usize,
    pub is_public_api: bool,
}

/// Review information for a single changed entity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityReview {
    pub entity_id: String,
    pub entity_name: String,
    pub entity_type: String,
    pub file_path: String,
    pub change_type: ChangeType,
    pub classification: ChangeClassification,
    pub risk_score: f64,
    pub risk_level: RiskLevel,
    pub blast_radius: usize,
    pub dependent_count: usize,
    pub dependency_count: usize,
    pub is_public_api: bool,
    pub structural_change: Option<bool>,
    pub group_id: usize,
    pub start_line: usize,
    pub end_line: usize,
    /// Source content before the change (None for Added entities)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before_content: Option<String>,
    /// Source content after the change (None for Deleted entities)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_content: Option<String>,
    /// Entities that depend on this entity: (name, file_path)
    pub dependent_names: Vec<(String, String)>,
    /// Entities this entity depends on: (name, file_path)
    pub dependency_names: Vec<(String, String)>,
    /// Full source code of top dependent entities (callers/consumers).
    /// Only populated when analyze_with_options is called with include_dependent_code.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependent_entities: Vec<DependentEntity>,
}

/// A logical group of related changes (from untangling).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeGroup {
    pub id: usize,
    pub label: String,
    pub entity_ids: Vec<String>,
}

/// Summary statistics for a review.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewStats {
    pub total_entities: usize,
    pub by_risk: RiskBreakdown,
    pub by_classification: ClassificationBreakdown,
    pub by_change_type: ChangeTypeBreakdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskBreakdown {
    pub critical: usize,
    pub high: usize,
    pub medium: usize,
    pub low: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassificationBreakdown {
    pub text: usize,
    pub syntax: usize,
    pub functional: usize,
    pub mixed: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeTypeBreakdown {
    pub added: usize,
    pub modified: usize,
    pub deleted: usize,
    pub moved: usize,
    pub renamed: usize,
}

/// Timing breakdown for the analysis pipeline.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Timing {
    /// Time to compute entity-level diff (ms)
    pub diff_ms: u64,
    /// Time to list source files (ms)
    pub list_files_ms: u64,
    /// Number of source files in the repo
    pub file_count: usize,
    /// Time to build the entity graph (ms)
    pub graph_build_ms: u64,
    /// Number of entities in the graph
    pub graph_entity_count: usize,
    /// Time for scoring, classification, untangling (ms)
    pub scoring_ms: u64,
    /// Total wall-clock time (ms)
    pub total_ms: u64,
}

/// Complete review result for a set of changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewResult {
    pub entity_reviews: Vec<EntityReview>,
    pub groups: Vec<ChangeGroup>,
    pub stats: ReviewStats,
    pub timing: Timing,
    /// Dependency edges between changed entities, used to recompute groups after filtering.
    #[serde(skip)]
    pub dependency_edges: Vec<(String, String)>,
    /// The underlying semantic changes (for formatters that want raw data)
    #[serde(skip)]
    pub changes: Vec<SemanticChange>,
}

/// An unchanged entity that is at risk of breaking due to a change in something it depends on.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtRiskEntity {
    pub entity_name: String,
    pub entity_type: String,
    pub file_path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub content: String,
    pub risk_level: RiskLevel,
    pub risk_score: f64,
    pub own_dependent_count: usize,
    pub is_public_api: bool,
    pub is_cross_file: bool,
}

/// A changed entity that threatens unchanged callers/consumers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreatSource {
    pub entity_name: String,
    pub entity_type: String,
    pub file_path: String,
    pub change_type: ChangeType,
    pub classification: ChangeClassification,
    pub at_risk: Vec<AtRiskEntity>,
}

/// Result of blast zone prediction: unchanged code at risk of breaking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictResult {
    pub threats: Vec<ThreatSource>,
    pub total_changes: usize,
    pub total_at_risk: usize,
    pub at_risk_by_level: RiskBreakdown,
    pub timing: Timing,
}
