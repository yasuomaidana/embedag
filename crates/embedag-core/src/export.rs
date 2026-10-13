//! Visual graph exporters and interchange format serializers for [`MemoryDag`].
//!
//! Supports:
//! - Mermaid flowcharts (`graph TD`)
//! - Graphviz DOT (`digraph G`)
//! - GraphML (XML)
//! - W3C JSON-LD
//! - W3C RDF-Star Turtle (aligned with `cool-japan/oxirs`)

use crate::dag::MemoryDag;
use crate::traits::NodePayload;
use petgraph::visit::EdgeRef;
use serde_json::json;
use std::collections::HashMap;

/// Visual and interchange export options for [`MemoryDag`].
#[derive(Debug, Clone)]
pub struct GraphExporter;

impl GraphExporter {
    /// Serializes the DAG into a valid Mermaid flowchart string (`graph TD`).
    pub fn to_mermaid<T: NodePayload>(dag: &MemoryDag<T>) -> String {
        let mut out = String::from("graph TD\n");
        let inner = dag.inner();

        // Node definitions
        let mut idx_to_mermaid_id: HashMap<usize, String> = HashMap::new();
        for idx in inner.node_indices() {
            if let Some(payload) = inner.node_weight(idx) {
                let m_id = format!("n{}", idx.index());
                let raw_label = format!(
                    "{}: {} ({})",
                    payload.key(),
                    payload.name(),
                    payload.entity_level()
                );
                let escaped_label = raw_label.replace('"', "#quot;");
                out.push_str(&format!("    {}[\"{}\"]\n", m_id, escaped_label));
                idx_to_mermaid_id.insert(idx.index(), m_id);
            }
        }

        // Edge definitions
        for edge in inner.edge_references() {
            let src_id = &idx_to_mermaid_id[&edge.source().index()];
            let tgt_id = &idx_to_mermaid_id[&edge.target().index()];
            let weight = edge.weight();
            let label = format!("{} ({:.2})", weight.relationship, weight.confidence);
            out.push_str(&format!("    {} -->|\"{}\"| {}\n", src_id, label, tgt_id));
        }

        out
    }

    /// Serializes the DAG into a valid Graphviz DOT language string (`digraph G`).
    pub fn to_dot<T: NodePayload>(dag: &MemoryDag<T>) -> String {
        let mut out =
            String::from("digraph G {\n    rankdir=TB;\n    node [shape=box, style=rounded];\n");
        let inner = dag.inner();

        let mut idx_to_dot_id: HashMap<usize, String> = HashMap::new();
        for idx in inner.node_indices() {
            if let Some(payload) = inner.node_weight(idx) {
                let d_id = format!("node_{}", idx.index());
                let raw_label = format!(
                    "{}\\{}\\[{}]",
                    payload.key(),
                    payload.name(),
                    payload.entity_level()
                );
                let escaped = raw_label.replace('"', "\\\"");
                out.push_str(&format!("    {} [label=\"{}\"];\n", d_id, escaped));
                idx_to_dot_id.insert(idx.index(), d_id);
            }
        }

        for edge in inner.edge_references() {
            let src = &idx_to_dot_id[&edge.source().index()];
            let tgt = &idx_to_dot_id[&edge.target().index()];
            let weight = edge.weight();
            let label = format!("{} ({:.2})", weight.relationship, weight.confidence);
            out.push_str(&format!("    {} -> {} [label=\"{}\"];\n", src, tgt, label));
        }

        out.push_str("}\n");
        out
    }

    /// Serializes the DAG into a valid GraphML XML format string.
    pub fn to_graphml<T: NodePayload>(dag: &MemoryDag<T>) -> String {
        let mut out = String::from(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<graphml xmlns="http://graphml.graphdrawing.org/xmlns"
         xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
         xsi:schemaLocation="http://graphml.graphdrawing.org/xmlns
         http://graphml.graphdrawing.org/xmlns/1.0/graphml.xsd">
  <key id="key" for="node" attr.name="key" attr.type="string"/>
  <key id="name" for="node" attr.name="name" attr.type="string"/>
  <key id="level" for="node" attr.name="level" attr.type="string"/>
  <key id="relationship" for="edge" attr.name="relationship" attr.type="string"/>
  <key id="confidence" for="edge" attr.name="confidence" attr.type="float"/>
  <graph id="G" edgedefault="directed">
"#,
        );
        let inner = dag.inner();

        for idx in inner.node_indices() {
            if let Some(payload) = inner.node_weight(idx) {
                out.push_str(&format!(
                    "    <node id=\"n{}\">\n      <data key=\"key\">{}</data>\n      <data key=\"name\">{}</data>\n      <data key=\"level\">{}</data>\n    </node>\n",
                    idx.index(),
                    quick_xml_escape(&payload.key().to_string()),
                    quick_xml_escape(payload.name()),
                    quick_xml_escape(payload.entity_level())
                ));
            }
        }

        for (edge_id, edge) in inner.edge_references().enumerate() {
            let src = edge.source().index();
            let tgt = edge.target().index();
            let weight = edge.weight();
            out.push_str(&format!(
                "    <edge id=\"e{}\" source=\"n{}\" target=\"n{}\">\n      <data key=\"relationship\">{}</data>\n      <data key=\"confidence\">{:.4}</data>\n    </edge>\n",
                edge_id,
                src,
                tgt,
                quick_xml_escape(&weight.relationship),
                weight.confidence
            ));
        }

        out.push_str("  </graph>\n</graphml>\n");
        out
    }

    /// Serializes the DAG into a W3C JSON-LD graph structure.
    pub fn to_json_ld<T: NodePayload>(dag: &MemoryDag<T>) -> serde_json::Value {
        let inner = dag.inner();
        let mut graph_items = Vec::new();

        for idx in inner.node_indices() {
            if let Some(payload) = inner.node_weight(idx) {
                let key_str = payload.key().to_string();

                // Find connected child relations
                let mut outgoing = Vec::new();
                for edge in inner.edges(idx) {
                    if let Some(child_payload) = inner.node_weight(edge.target()) {
                        outgoing.push(json!({
                            "@type": edge.weight().relationship,
                            "target": format!("urn:embedag:node:{}", child_payload.key()),
                            "confidence": edge.weight().confidence,
                        }));
                    }
                }

                graph_items.push(json!({
                    "@id": format!("urn:embedag:node:{}", key_str),
                    "@type": payload.entity_level(),
                    "name": payload.name(),
                    "business_key": key_str,
                    "attributes": payload.attributes(),
                    "relationships": outgoing,
                }));
            }
        }

        json!({
            "@context": {
                "@vocab": "https://schema.org/",
                "embedag": "https://embedag.dev/ns#",
                "business_key": "embedag:businessKey",
                "relationships": "embedag:relationships"
            },
            "@graph": graph_items
        })
    }

    /// Serializes the DAG into W3C RDF-Star Turtle syntax compatible with `cool-japan/oxirs`.
    ///
    /// Uses RDF-Star quoted triples `<< :subject :predicate :object >>` with confidence annotation.
    pub fn to_rdf_star_turtle<T: NodePayload>(dag: &MemoryDag<T>) -> String {
        let mut out = String::from(
            "@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\
             @prefix embedag: <https://embedag.dev/ns#> .\n\
             @prefix node: <urn:embedag:node:> .\n\n",
        );

        let inner = dag.inner();
        for idx in inner.node_indices() {
            if let Some(payload) = inner.node_weight(idx) {
                let key = payload.key().to_string();
                out.push_str(&format!(
                    "node:{} a embedag:{} ;\n    rdfs:label \"{}\" ;\n    embedag:businessKey \"{}\" .\n\n",
                    key,
                    payload.entity_level(),
                    payload.name().replace('"', "\\\""),
                    key
                ));
            }
        }

        for edge in inner.edge_references() {
            if let (Some(src), Some(tgt)) = (
                inner.node_weight(edge.source()),
                inner.node_weight(edge.target()),
            ) {
                let weight = edge.weight();
                out.push_str(&format!(
                    "<< node:{} embedag:{} node:{} >> embedag:confidence \"{:.4}\"^^xsd:float ;\n    embedag:provenance \"{:?}\" .\n",
                    src.key(),
                    weight.relationship,
                    tgt.key(),
                    weight.confidence,
                    weight.provenance
                ));
            }
        }

        out
    }
}

fn quick_xml_escape(raw: &str) -> String {
    raw.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{EdgePayload, ProvenanceKind};

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct MockItem {
        id: String,
        label: String,
        tier: String,
    }

    impl MockItem {
        fn new(id: &str, label: &str, tier: &str) -> Self {
            Self {
                id: id.into(),
                label: label.into(),
                tier: tier.into(),
            }
        }
    }

    impl NodePayload for MockItem {
        type Key = String;
        fn key(&self) -> &Self::Key {
            &self.id
        }
        fn name(&self) -> &str {
            &self.label
        }
        fn entity_level(&self) -> &str {
            &self.tier
        }
    }

    fn sample_dag() -> MemoryDag<MockItem> {
        let mut dag = MemoryDag::new();
        let parent = MockItem::new("CAT-1", "Dairy", "Category");
        let child = MockItem::new("SKU-99", "Whole Milk 1L", "Product");

        dag.upsert_node(parent);
        dag.upsert_node(child);

        let edge = EdgePayload::new("PARENT_OF", 1.0, ProvenanceKind::DeterministicRule).unwrap();
        dag.add_edge_by_keys(&"CAT-1".into(), &"SKU-99".into(), edge)
            .unwrap();
        dag
    }

    #[test]
    fn test_mermaid_export_syntax() {
        let dag = sample_dag();
        let mermaid = GraphExporter::to_mermaid(&dag);

        assert!(mermaid.starts_with("graph TD\n"));
        assert!(mermaid.contains("CAT-1: Dairy (Category)"));
        assert!(mermaid.contains("SKU-99: Whole Milk 1L (Product)"));
        assert!(mermaid.contains("-->|\"PARENT_OF (1.00)\"|"));
    }

    #[test]
    fn test_dot_export_syntax() {
        let dag = sample_dag();
        let dot = GraphExporter::to_dot(&dag);

        assert!(dot.starts_with("digraph G {\n"));
        assert!(dot.contains("CAT-1\\Dairy\\[Category]"));
        assert!(dot.contains("SKU-99\\Whole Milk 1L\\[Product]"));
        assert!(dot.contains("label=\"PARENT_OF (1.00)\""));
        assert!(dot.ends_with("}\n"));
    }

    #[test]
    fn test_graphml_serialization() {
        let dag = sample_dag();
        let xml = GraphExporter::to_graphml(&dag);

        assert!(xml.contains("<graphml xmlns="));
        assert!(xml.contains("<data key=\"key\">CAT-1</data>"));
        assert!(xml.contains("<data key=\"name\">Whole Milk 1L</data>"));
        assert!(xml.contains("<data key=\"relationship\">PARENT_OF</data>"));
        assert!(xml.contains("</graphml>"));
    }

    #[test]
    fn test_json_ld_serialization() {
        let dag = sample_dag();
        let json_ld = GraphExporter::to_json_ld(&dag);

        assert_eq!(json_ld["@context"]["@vocab"], "https://schema.org/");
        let graph = json_ld["@graph"].as_array().unwrap();
        assert_eq!(graph.len(), 2);

        let cat_item = graph.iter().find(|i| i["business_key"] == "CAT-1").unwrap();
        assert_eq!(cat_item["name"], "Dairy");
        assert_eq!(cat_item["@type"], "Category");
        assert_eq!(cat_item["relationships"][0]["@type"], "PARENT_OF");
    }

    #[test]
    fn test_rdf_star_turtle_serialization() {
        let dag = sample_dag();
        let ttl = GraphExporter::to_rdf_star_turtle(&dag);

        assert!(ttl.contains("@prefix embedag: <https://embedag.dev/ns#> ."));
        assert!(ttl.contains("node:CAT-1 a embedag:Category ;"));
        assert!(ttl.contains("rdfs:label \"Dairy\" ;"));
        assert!(ttl.contains("<< node:CAT-1 embedag:PARENT_OF node:SKU-99 >> embedag:confidence \"1.0000\"^^xsd:float"));
    }
}
