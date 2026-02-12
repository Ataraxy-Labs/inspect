use std::collections::HashMap;

use crate::types::{ChangeGroup, EntityReview};

/// Union-Find data structure for grouping related entities.
struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
            rank: vec![0; n],
        }
    }

    fn find(&mut self, x: usize) -> usize {
        if self.parent[x] != x {
            self.parent[x] = self.find(self.parent[x]);
        }
        self.parent[x]
    }

    fn union(&mut self, a: usize, b: usize) {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra == rb {
            return;
        }
        if self.rank[ra] < self.rank[rb] {
            self.parent[ra] = rb;
        } else if self.rank[ra] > self.rank[rb] {
            self.parent[rb] = ra;
        } else {
            self.parent[rb] = ra;
            self.rank[ra] += 1;
        }
    }
}

/// Group entity reviews into logical change groups using dependency edges.
///
/// Two entities are in the same group if one depends on the other
/// (directly or transitively through other changed entities).
pub fn untangle(
    reviews: &[EntityReview],
    dependency_edges: &[(String, String)],
) -> Vec<ChangeGroup> {
    if reviews.is_empty() {
        return vec![];
    }

    // Map entity_id -> index
    let id_to_idx: HashMap<&str, usize> = reviews
        .iter()
        .enumerate()
        .map(|(i, r)| (r.entity_id.as_str(), i))
        .collect();

    let mut uf = UnionFind::new(reviews.len());

    // Union entities that share a dependency edge
    for (from, to) in dependency_edges {
        if let (Some(&a), Some(&b)) = (id_to_idx.get(from.as_str()), id_to_idx.get(to.as_str())) {
            uf.union(a, b);
        }
    }

    // Collect groups by root
    let mut groups_map: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..reviews.len() {
        let root = uf.find(i);
        groups_map.entry(root).or_default().push(i);
    }

    // Build ChangeGroup objects
    let mut groups: Vec<ChangeGroup> = groups_map
        .into_values()
        .enumerate()
        .map(|(group_id, indices)| {
            let entity_ids: Vec<String> = indices
                .iter()
                .map(|&i| reviews[i].entity_id.clone())
                .collect();

            // Label from the first entity's file path
            let label = if entity_ids.len() == 1 {
                reviews[indices[0]].entity_name.clone()
            } else {
                // Use common file path prefix or first entity name
                let files: Vec<&str> = indices.iter().map(|&i| reviews[i].file_path.as_str()).collect();
                let common = common_prefix(&files);
                if common.is_empty() {
                    format!("{} entities", entity_ids.len())
                } else {
                    common
                }
            };

            ChangeGroup {
                id: group_id,
                label,
                entity_ids,
            }
        })
        .collect();

    // Sort by group size (largest first)
    groups.sort_by(|a, b| b.entity_ids.len().cmp(&a.entity_ids.len()));

    // Re-number group IDs
    for (i, group) in groups.iter_mut().enumerate() {
        group.id = i;
    }

    groups
}

fn common_prefix(strings: &[&str]) -> String {
    if strings.is_empty() {
        return String::new();
    }
    let first = strings[0];
    let mut len = first.len();
    for s in &strings[1..] {
        len = len.min(s.len());
        for (i, (a, b)) in first.bytes().zip(s.bytes()).enumerate() {
            if a != b {
                len = len.min(i);
                break;
            }
        }
    }
    // Trim to last '/' for a clean path prefix
    let prefix = &first[..len];
    if let Some(pos) = prefix.rfind('/') {
        first[..=pos].to_string()
    } else {
        prefix.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ChangeClassification, EntityReview, RiskLevel};
    use sem_core::model::change::ChangeType;

    fn make_review(id: &str, name: &str, file: &str) -> EntityReview {
        EntityReview {
            entity_id: id.into(),
            entity_name: name.into(),
            entity_type: "function".into(),
            file_path: file.into(),
            change_type: ChangeType::Modified,
            classification: ChangeClassification::Functional,
            risk_score: 0.5,
            risk_level: RiskLevel::Medium,
            blast_radius: 0,
            dependent_count: 0,
            dependency_count: 0,
            is_public_api: false,
            structural_change: Some(true),
            group_id: 0,
            start_line: 1,
            end_line: 10,
        }
    }

    #[test]
    fn independent_entities_separate_groups() {
        let reviews = vec![
            make_review("a", "foo", "src/a.rs"),
            make_review("b", "bar", "src/b.rs"),
        ];
        let edges: Vec<(String, String)> = vec![];
        let groups = untangle(&reviews, &edges);
        assert_eq!(groups.len(), 2);
    }

    #[test]
    fn connected_entities_one_group() {
        let reviews = vec![
            make_review("a", "foo", "src/a.rs"),
            make_review("b", "bar", "src/a.rs"),
            make_review("c", "baz", "src/a.rs"),
        ];
        let edges = vec![
            ("a".to_string(), "b".to_string()),
            ("b".to_string(), "c".to_string()),
        ];
        let groups = untangle(&reviews, &edges);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].entity_ids.len(), 3);
    }
}
