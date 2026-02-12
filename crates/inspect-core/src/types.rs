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

/// Complete review result for a set of changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewResult {
    pub entity_reviews: Vec<EntityReview>,
    pub groups: Vec<ChangeGroup>,
    pub stats: ReviewStats,
    /// The underlying semantic changes (for formatters that want raw data)
    #[serde(skip)]
    pub changes: Vec<SemanticChange>,
}
