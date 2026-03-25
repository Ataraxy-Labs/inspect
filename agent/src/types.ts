/** Mirrors inspect-core's DetectorFinding (Rust struct). */
export interface DetectorFinding {
  rule_id: string;
  message: string;
  detector: "pattern" | "contract" | "diff_heuristic";
  confidence: number;
  severity: "critical" | "high" | "medium" | "low";
  entity_id: string;
  entity_name: string;
  file_path: string;
  evidence: string;
  start_line: number;
  end_line: number;
}

/** Mirrors inspect-core's EntityReview (Rust struct). */
export interface EntityReview {
  entity_id: string;
  entity_name: string;
  entity_type: string;
  file_path: string;
  change_type: string;
  classification: string;
  risk_score: number;
  risk_level: string;
  blast_radius: number;
  dependent_count: number;
  dependency_count: number;
  is_public_api: boolean;
  structural_change?: boolean;
  group_id: number;
  start_line: number;
  end_line: number;
  before_content?: string;
  after_content?: string;
  dependent_names: [string, string][];
  dependency_names: [string, string][];
}

/** Input piped from Rust via stdin. */
export interface ValidateInput {
  pr_title: string;
  diff: string;
  triage_section: string;
  findings: DetectorFinding[];
  entity_reviews: EntityReview[];
  /** Working directory of the checked-out repo (for tool access). */
  repo_dir: string;
  /** LLM provider to use (e.g., "anthropic", "openai", "google"). */
  provider?: string;
  /** Model ID (e.g., "claude-sonnet-4-20250514", "gpt-4o"). */
  model?: string;
}

/** A single verdict for one finding. */
export interface Verdict {
  rule_id: string;
  entity_name: string;
  verdict: "true_positive" | "false_positive";
  explanation: string;
}

/** Output written to stdout as JSON. */
export interface ValidateOutput {
  verdicts: Verdict[];
  raw_review?: string;
}
