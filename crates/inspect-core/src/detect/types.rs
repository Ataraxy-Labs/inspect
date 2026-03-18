use serde::{Deserialize, Serialize};

/// A deterministic finding from static analysis.
/// These are candidates for LLM validation — NOT final bugs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectorFinding {
    /// Which rule fired (e.g., "foreach-async", "ssrf-url-concat", "missing-null-check")
    pub rule_id: String,
    /// Human-readable one-line description of the potential bug
    pub message: String,
    /// The detector engine that produced this ("pattern", "contract", "diff-heuristic")
    pub detector: DetectorKind,
    /// Confidence: 0.0 (speculative) to 1.0 (certain)
    pub confidence: f64,
    /// Severity hint for the LLM
    pub severity: Severity,
    /// The entity that triggered this finding
    pub entity_id: String,
    pub entity_name: String,
    pub file_path: String,
    /// The specific code evidence (the snippet that triggered the rule)
    pub evidence: String,
    /// Line range in the after-content where the issue is
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectorKind {
    Pattern,
    Contract,
    DiffHeuristic,
}

impl std::fmt::Display for DetectorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pattern => write!(f, "pattern"),
            Self::Contract => write!(f, "contract"),
            Self::DiffHeuristic => write!(f, "diff_heuristic"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Critical,
    High,
    Medium,
    Low,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Critical => write!(f, "critical"),
            Self::High => write!(f, "high"),
            Self::Medium => write!(f, "medium"),
            Self::Low => write!(f, "low"),
        }
    }
}
