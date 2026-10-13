use crate::schema::ExtractionEnvelope;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use thiserror::Error;

/// Error variants encountered during prompt formatting and envelope extraction.
#[derive(Debug, Error)]
pub enum PromptError {
    #[error("Failed to deserialize extraction envelope: {0}")]
    DeserializationError(#[from] serde_json::Error),

    #[error("Missing envelope directive payload")]
    MissingPayload,
}

/// Provenance metadata extracted from the model's chain-of-thought scratchpad.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProvenanceNotes {
    pub chain_of_thought: String,
    pub character_length: usize,
}

/// Prompt formatter for Draft-Then-Decode structured extraction.
pub struct PromptFormatter;

impl PromptFormatter {
    /// Formats an extraction prompt instructing the model to reason first in `chain_of_thought`
    /// before producing the final deterministic `directive`.
    pub fn build_prompt(
        system_role: &str,
        entity_payload: &str,
        candidate_subgraphs: &[&str],
    ) -> String {
        Self::build_prompt_full(system_role, entity_payload, candidate_subgraphs, None, None)
    }

    /// Formats an extraction prompt including field descriptions and constraints extracted from a JSON schema.
    pub fn build_prompt_with_schema(
        system_role: &str,
        entity_payload: &str,
        candidate_subgraphs: &[&str],
        schema: Option<&serde_json::Value>,
    ) -> String {
        Self::build_prompt_full(
            system_role,
            entity_payload,
            candidate_subgraphs,
            schema,
            None,
        )
    }

    /// Formats an extraction prompt including candidate subgraphs, field-level constraints, and relational table context.
    pub fn build_prompt_full(
        system_role: &str,
        entity_payload: &str,
        candidate_subgraphs: &[&str],
        schema: Option<&serde_json::Value>,
        table_context: Option<&str>,
    ) -> String {
        let mut prompt = String::new();
        prompt.push_str("### SYSTEM ROLE\n");
        prompt.push_str(system_role);
        prompt.push_str("\n\n### INCOMING RECORD\n");
        prompt.push_str(entity_payload);
        prompt.push_str("\n\n### CANDIDATE PARENT / PEER SUBGRAPHS\n");

        if candidate_subgraphs.is_empty() {
            prompt.push_str("- None (Evaluate for new root creation)\n");
        } else {
            for (idx, candidate) in candidate_subgraphs.iter().enumerate() {
                prompt.push_str(&format!("{}. {}\n", idx + 1, candidate));
            }
        }

        if let Some(tbl_ctx) = table_context {
            prompt.push_str("\n### TARGET HIERARCHY TABLE CONTEXT\n");
            prompt.push_str(tbl_ctx);
            prompt.push('\n');
        }

        if let Some(schema_val) = schema {
            let descriptions = extract_field_descriptions(schema_val);
            if !descriptions.is_empty() {
                prompt.push_str("\n### FIELD CONSTRAINTS & SPECIFICATIONS\n");
                for (field, desc) in &descriptions {
                    prompt.push_str(&format!("- {}: \"{}\"\n", field, desc));
                }
            }
        }

        prompt.push_str("\n### OUTPUT CONTRACT\n");
        prompt
            .push_str("Respond with a JSON object conforming to the Draft-Then-Decode contract:\n");
        prompt.push_str("1. \"chain_of_thought\": Write thorough step-by-step reasoning explaining category matches, attribute diffs, or why an edge should be established.\n");
        prompt.push_str("2. \"directive\": The exact structured action.\n");

        prompt
    }

    /// Deserializes model output into typed `ExtractionEnvelope<T>` and extracts provenance telemetry.
    pub fn parse_envelope<T: for<'de> Deserialize<'de>>(
        raw_json: &str,
    ) -> Result<(T, ProvenanceNotes), PromptError> {
        let envelope: ExtractionEnvelope<T> = serde_json::from_str(raw_json)?;
        let notes = ProvenanceNotes {
            character_length: envelope.chain_of_thought.len(),
            chain_of_thought: envelope.chain_of_thought,
        };
        Ok((envelope.directive, notes))
    }
}

/// Recursively extracts `description` fields associated with struct properties from a JSON Schema.
pub fn extract_field_descriptions(val: &serde_json::Value) -> Vec<(String, String)> {
    let mut results = Vec::new();
    let mut seen = HashSet::new();

    fn walk(
        val: &serde_json::Value,
        results: &mut Vec<(String, String)>,
        seen: &mut HashSet<String>,
    ) {
        if let Some(props) = val.get("properties").and_then(|p| p.as_object()) {
            for (k, v) in props {
                if let Some(desc) = v.get("description").and_then(|d| d.as_str())
                    && seen.insert(k.clone())
                {
                    results.push((k.clone(), desc.to_string()));
                }
                walk(v, results, seen);
            }
        }
        if let Some(defs) = val
            .get("$defs")
            .or_else(|| val.get("definitions"))
            .and_then(|d| d.as_object())
        {
            for (_def_name, def_val) in defs {
                walk(def_val, results, seen);
            }
        }
    }

    walk(val, &mut results, &mut seen);
    results.sort_by(|a, b| a.0.cmp(&b.0));
    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::ExtractionDirective;
    use schemars::JsonSchema;

    #[derive(JsonSchema, Serialize, Deserialize)]
    struct MockAnnotatedNode {
        id: String,
        #[schemars(
            description = "single word category, categories with multiples word but single meaning are allowed too"
        )]
        category: String,
    }

    #[test]
    fn test_field_description_extraction_and_prompt_inclusion() {
        let schema = schemars::schema_for!(MockAnnotatedNode);
        let schema_val = serde_json::to_value(&schema).unwrap();

        let descs = extract_field_descriptions(&schema_val);
        assert_eq!(descs.len(), 1);
        assert_eq!(descs[0].0, "category");
        assert!(descs[0].1.contains("single word category"));

        let prompt = PromptFormatter::build_prompt_full(
            "You are a taxonomy builder",
            "{\"name\": \"Whole Milk\"}",
            &["Dairy"],
            Some(&schema_val),
            Some("Table: receipt_categories\n- Description: Categories hierarchy"),
        );

        assert!(prompt.contains("### TARGET HIERARCHY TABLE CONTEXT"));
        assert!(prompt.contains("receipt_categories"));
        assert!(prompt.contains("### FIELD CONSTRAINTS & SPECIFICATIONS"));
        assert!(prompt.contains("single word category"));
    }

    #[test]
    fn test_draft_then_decode_ordering() {
        let envelope = ExtractionEnvelope {
            chain_of_thought: "Step 1: SKU matched parent cat_42 with confidence 0.95.".into(),
            directive: ExtractionDirective::InsertNode {
                parent_key: "cat_42".into(),
                entity_name: "Cold Brew".into(),
                entity_level: "sku".into(),
                relationship: "VARIANT_OF".into(),
                confidence: 0.95,
            },
        };

        let serialized = serde_json::to_string(&envelope).expect("serialize envelope");

        // Assert chain_of_thought is the first key serialized
        let first_key_pos = serialized
            .find("\"chain_of_thought\"")
            .expect("must contain chain_of_thought");
        let directive_key_pos = serialized
            .find("\"directive\"")
            .expect("must contain directive");
        assert!(
            first_key_pos < directive_key_pos,
            "chain_of_thought must appear ahead of directive in serialized JSON"
        );

        // Deserialization with provenance tracking
        let (directive, notes) =
            PromptFormatter::parse_envelope::<ExtractionDirective>(&serialized)
                .expect("parse envelope");

        assert_eq!(directive, envelope.directive);
        assert_eq!(
            notes.chain_of_thought,
            "Step 1: SKU matched parent cat_42 with confidence 0.95."
        );
        assert_eq!(notes.character_length, notes.chain_of_thought.len());
    }

    #[test]
    fn test_prompt_formatter_layout() {
        let prompt = PromptFormatter::build_prompt(
            "You are a taxonomy classifier.",
            "{\"sku\": \"A-100\", \"title\": \"Organic Oat Milk\"}",
            &[
                "Parent: Beverage > Dairy Alternatives",
                "Parent: Food > Pantry",
            ],
        );

        assert!(prompt.contains("### SYSTEM ROLE"));
        assert!(prompt.contains("Organic Oat Milk"));
        assert!(prompt.contains("Dairy Alternatives"));
        assert!(prompt.contains("\"chain_of_thought\""));
    }
}
