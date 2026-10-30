use crate::expansion::{ExpandedEdge, ExpandedSubgraph, SeedRelationship};
use embedag_core::dag::MemoryDag;
use embedag_core::traits::NodePayload;
use std::collections::HashSet;
use std::fmt::Display;

/// Synthesizes and packages grounded evidence contexts with citation anchors for LLM consumption.
#[derive(Debug, Default, Clone)]
pub struct GroundedEvidencePackager;

/// Lists the linked nodes of `edges` under `heading`; `linked` picks the node on the other side.
/// Nothing is written when there are no edges.
fn write_links<'a, K: Display + 'a>(
    out: &mut String,
    heading: &str,
    edges: impl Iterator<Item = &'a ExpandedEdge<K>>,
    linked: impl Fn(&ExpandedEdge<K>) -> &K,
) {
    let edges: Vec<_> = edges.collect();
    if edges.is_empty() {
        return;
    }
    out.push_str(heading);
    for edge in edges {
        let prov_tag = if edge.provenance.is_deterministic() && edge.confidence >= 1.0 {
            "EXTRACTED (1.0)"
        } else {
            "INFERRED (< 1.0)"
        };
        out.push_str(&format!(
            "  * [Node: {}] via {} [{}]\n",
            linked(edge),
            edge.relationship,
            prov_tag
        ));
    }
}

impl GroundedEvidencePackager {
    pub fn new() -> Self {
        Self
    }

    /// Formats an expanded subgraph into a Markdown grounded evidence block.
    pub fn package<T: NodePayload>(
        &self,
        dag: &MemoryDag<T>,
        subgraph: &ExpandedSubgraph<T::Key>,
    ) -> String
    where
        T::Key: std::fmt::Display + std::fmt::Debug + std::hash::Hash + Eq + Clone,
    {
        let mut out = String::new();
        out.push_str("### Grounded Evidence Context\n");

        // Sort nodes: seeds first, then by hop distance ascending, then weight descending
        let mut sorted_nodes: Vec<_> = subgraph.nodes.values().collect();
        sorted_nodes.sort_by(|a, b| {
            let rel_order = match (a.relationship, b.relationship) {
                (SeedRelationship::Seed, SeedRelationship::Seed) => std::cmp::Ordering::Equal,
                (SeedRelationship::Seed, _) => std::cmp::Ordering::Less,
                (_, SeedRelationship::Seed) => std::cmp::Ordering::Greater,
                _ => std::cmp::Ordering::Equal,
            };
            rel_order
                .then_with(|| a.hop_distance.cmp(&b.hop_distance))
                .then_with(|| {
                    b.weight
                        .partial_cmp(&a.weight)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });

        for node in sorted_nodes {
            if let Some(payload) = dag.node(&node.key) {
                let rel_tag = match node.relationship {
                    SeedRelationship::Seed => "SEED",
                    SeedRelationship::Ancestor => "ANCESTOR",
                    SeedRelationship::Descendant => "DESCENDANT",
                    SeedRelationship::Sibling => "SIBLING",
                };

                out.push_str(&format!(
                    "\n[Node: {}] (Role: {}, Hop: {}, Weight: {:.2}, Level: {})\n",
                    payload.key(),
                    rel_tag,
                    node.hop_distance,
                    node.weight,
                    payload.entity_level()
                ));
                out.push_str(&format!("- Name: {}\n", payload.name()));

                // Connected incoming edges (parents), then outgoing edges (children)
                let parents = subgraph.edges.iter().filter(|e| e.child_key == node.key);
                write_links(&mut out, "- Parents / Ancestors:\n", parents, |e| {
                    &e.parent_key
                });
                let children = subgraph.edges.iter().filter(|e| e.parent_key == node.key);
                write_links(&mut out, "- Children / Descendants:\n", children, |e| {
                    &e.child_key
                });
            }
        }

        out.push_str("\n### Citation & Grounding Guardrails\n");
        out.push_str("Citation Requirement: When answering the user prompt, every factual assertion MUST cite its supporting node ID (e.g., `[Node: KEY]`).\n");
        out.push_str(
            "Answer strictly and exclusively using the Grounded Evidence Context above.\n",
        );
        out.push_str("If an attribute or relationship is not stated, report that the entity lacks that attribute.\n");

        out
    }

    /// Extracts all cited node keys from a generated LLM answer text (e.g. `[Node: XYZ]`).
    pub fn extract_citations(&self, answer: &str) -> HashSet<String> {
        let mut citations = HashSet::new();
        let needle = "[Node: ";
        let mut rest = answer;

        while let Some(start) = rest.find(needle) {
            let after = &rest[start + needle.len()..];
            if let Some(end) = after.find(']') {
                let key = after[..end].trim();
                if !key.is_empty() {
                    citations.insert(key.to_string());
                }
                rest = &after[end + 1..];
            } else {
                break;
            }
        }

        citations
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expansion::SubgraphExpander;
    use embedag_core::model::{EdgePayload, ProvenanceKind};
    use embedag_core::testing::TestNode;

    #[test]
    fn test_evidence_packaging_format() {
        let mut dag = MemoryDag::new();

        let brand = TestNode::new("BRAND-01", "Carlos V", "Brand");
        let sku = TestNode::new("SKU-100", "Carlos V Dark Chocolate 50g", "SKU");

        dag.upsert_node(brand);
        dag.upsert_node(sku);

        let edge_rule =
            EdgePayload::new("PARENT_OF", 1.0, ProvenanceKind::DeterministicRule).unwrap();
        dag.add_edge_by_keys(&"BRAND-01".into(), &"SKU-100".into(), edge_rule)
            .unwrap();

        let expander = SubgraphExpander::default();
        let subgraph = expander.expand(&dag, &[("SKU-100".to_string(), 1.0)]);

        let packager = GroundedEvidencePackager::new();
        let prompt_context = packager.package(&dag, &subgraph);

        // Verify structure
        assert!(prompt_context.contains("### Grounded Evidence Context"));
        assert!(prompt_context.contains("[Node: SKU-100]"));
        assert!(prompt_context.contains("[Node: BRAND-01]"));
        assert!(prompt_context.contains("EXTRACTED (1.0)"));
        assert!(prompt_context.contains("Citation Requirement"));

        // Verify citation extraction
        let answer = "The product [Node: SKU-100] belongs to brand [Node: BRAND-01].";
        let cited = packager.extract_citations(answer);
        assert_eq!(cited.len(), 2);
        assert!(cited.contains("SKU-100"));
        assert!(cited.contains("BRAND-01"));
    }
}
