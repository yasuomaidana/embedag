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
        render(dag, String::from("graph TD\n"), &MERMAID)
    }

    /// Serializes the DAG into a valid Graphviz DOT language string (`digraph G`).
    pub fn to_dot<T: NodePayload>(dag: &MemoryDag<T>) -> String {
        let header = "digraph G {\n    rankdir=TB;\n    node [shape=box, style=rounded];\n";
        render(dag, String::from(header), &DOT) + "}\n"
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

/// How a line-per-node, line-per-edge notation spells a node id, label, node and edge.
struct Notation {
    /// Prefix of a node's exported id; the node index follows.
    id_prefix: &'static str,
    /// The unescaped label of a node from its key, name and level.
    label: fn(&str, &str, &str) -> String,
    /// The label with the notation's quote character escaped.
    escape: fn(&str) -> String,
    /// The line declaring a node from its id and escaped label.
    node: fn(&str, &str) -> String,
    /// The line for an edge from its source id, target id and label.
    edge: fn(&str, &str, &str) -> String,
}

const MERMAID: Notation = Notation {
    id_prefix: "n",
    label: mermaid_label,
    escape: mermaid_escape,
    node: mermaid_node,
    edge: mermaid_edge,
};

fn mermaid_label(key: &str, name: &str, level: &str) -> String {
    format!("{key}: {name} ({level})")
}

fn mermaid_escape(label: &str) -> String {
    label.replace('"', "#quot;")
}

fn mermaid_node(id: &str, label: &str) -> String {
    format!("    {id}[\"{label}\"]\n")
}

fn mermaid_edge(src: &str, tgt: &str, label: &str) -> String {
    format!("    {src} -->|\"{label}\"| {tgt}\n")
}

const DOT: Notation = Notation {
    id_prefix: "node_",
    label: dot_label,
    escape: dot_escape,
    node: dot_node,
    edge: dot_edge,
};

fn dot_label(key: &str, name: &str, level: &str) -> String {
    format!("{key}\\{name}\\[{level}]")
}

fn dot_escape(label: &str) -> String {
    label.replace('"', "\\\"")
}

fn dot_node(id: &str, label: &str) -> String {
    format!("    {id} [label=\"{label}\"];\n")
}

fn dot_edge(src: &str, tgt: &str, label: &str) -> String {
    format!("    {src} -> {tgt} [label=\"{label}\"];\n")
}

/// Appends every node and edge of `dag` to `out` in `notation`.
fn render<T: NodePayload>(dag: &MemoryDag<T>, mut out: String, notation: &Notation) -> String {
    let ids = write_nodes(dag, &mut out, notation.id_prefix, |id, payload| {
        let label = (notation.label)(
            &payload.key().to_string(),
            payload.name(),
            payload.entity_level(),
        );
        (notation.node)(id, &(notation.escape)(&label))
    });
    write_edges(dag, &ids, &mut out, notation.edge);
    out
}

/// Appends `line(id, payload)` for every node, naming each `{prefix}{index}`, and returns the
/// exported id of each node index.
fn write_nodes<T: NodePayload>(
    dag: &MemoryDag<T>,
    out: &mut String,
    prefix: &str,
    line: impl Fn(&str, &T) -> String,
) -> HashMap<usize, String> {
    let inner = dag.inner();
    let mut ids = HashMap::new();
    for idx in inner.node_indices() {
        if let Some(payload) = inner.node_weight(idx) {
            let id = format!("{prefix}{}", idx.index());
            out.push_str(&line(&id, payload));
            ids.insert(idx.index(), id);
        }
    }
    ids
}

/// Appends `line(source id, target id, "relationship (confidence)")` for every edge.
fn write_edges<T: NodePayload>(
    dag: &MemoryDag<T>,
    ids: &HashMap<usize, String>,
    out: &mut String,
    line: impl Fn(&str, &str, &str) -> String,
) {
    for edge in dag.inner().edge_references() {
        let weight = edge.weight();
        let label = format!("{} ({:.2})", weight.relationship, weight.confidence);
        out.push_str(&line(
            &ids[&edge.source().index()],
            &ids[&edge.target().index()],
            &label,
        ));
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
    use crate::testing::TestNode;
    use rstest::{fixture, rstest};

    #[fixture]
    fn sample_dag() -> MemoryDag<TestNode> {
        let mut dag = MemoryDag::new();
        let parent = TestNode::new("CAT-1", "Dairy", "Category");
        let child = TestNode::new("SKU-99", "Whole Milk 1L", "Product");

        dag.upsert_node(parent);
        dag.upsert_node(child);

        let edge = EdgePayload::new("PARENT_OF", 1.0, ProvenanceKind::DeterministicRule).unwrap();
        dag.add_edge_by_keys(&"CAT-1".into(), &"SKU-99".into(), edge)
            .unwrap();
        dag
    }

    #[rstest]
    #[case::mermaid(GraphExporter::to_mermaid, &[
        "graph TD\n",
        "CAT-1: Dairy (Category)",
        "SKU-99: Whole Milk 1L (Product)",
        "-->|\"PARENT_OF (1.00)\"|",
    ])]
    #[case::dot(GraphExporter::to_dot, &[
        "digraph G {\n",
        "CAT-1\\Dairy\\[Category]",
        "SKU-99\\Whole Milk 1L\\[Product]",
        "label=\"PARENT_OF (1.00)\"",
        "}\n",
    ])]
    #[case::graphml(GraphExporter::to_graphml, &[
        "<graphml xmlns=",
        "<data key=\"key\">CAT-1</data>",
        "<data key=\"name\">Whole Milk 1L</data>",
        "<data key=\"relationship\">PARENT_OF</data>",
        "</graphml>",
    ])]
    #[case::rdf_star_turtle(GraphExporter::to_rdf_star_turtle, &[
        "@prefix embedag: <https://embedag.dev/ns#> .",
        "node:CAT-1 a embedag:Category ;",
        "rdfs:label \"Dairy\" ;",
        "<< node:CAT-1 embedag:PARENT_OF node:SKU-99 >> embedag:confidence \"1.0000\"^^xsd:float",
    ])]
    fn text_exports_carry_nodes_and_edges(
        sample_dag: MemoryDag<TestNode>,
        #[case] export: fn(&MemoryDag<TestNode>) -> String,
        #[case] expected: &[&str],
    ) {
        let text = export(&sample_dag);
        for part in expected {
            assert!(text.contains(part), "missing {part:?} in:\n{text}");
        }
    }

    #[rstest]
    fn test_json_ld_serialization(sample_dag: MemoryDag<TestNode>) {
        let json_ld = GraphExporter::to_json_ld(&sample_dag);

        assert_eq!(json_ld["@context"]["@vocab"], "https://schema.org/");
        let graph = json_ld["@graph"].as_array().unwrap();
        assert_eq!(graph.len(), 2);

        let cat_item = graph.iter().find(|i| i["business_key"] == "CAT-1").unwrap();
        assert_eq!(cat_item["name"], "Dairy");
        assert_eq!(cat_item["@type"], "Category");
        assert_eq!(cat_item["relationships"][0]["@type"], "PARENT_OF");
    }
}
