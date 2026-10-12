use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// High-level typed graph extraction directive emitted by the constrained LLM.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ExtractionDirective {
    /// Connect record as a child of an existing parent node
    InsertNode {
        parent_key: String,
        entity_name: String,
        entity_level: String,
        relationship: String,
        confidence: f32,
    },
    /// Connect record to multiple parents simultaneously
    InsertMultiParentNode {
        parent_keys: Vec<String>,
        entity_name: String,
        entity_level: String,
        relationship: String,
        confidence: f32,
    },
    /// Record refers to an existing node; update its attributes
    UpdateAttributes {
        target_key: String,
        updated_fields: HashMap<String, serde_json::Value>,
        confidence: f32,
    },
    /// Move or retarget an existing relationship edge
    RetargetEdge {
        child_key: String,
        old_parent_key: String,
        new_parent_key: String,
        relationship: String,
        confidence: f32,
    },
    /// Establish a brand new root node
    CreateRootNode {
        entity_name: String,
        entity_level: String,
        confidence: f32,
    },
    /// Ambiguous match requiring human resolution
    MarkAmbiguous {
        candidate_keys: Vec<String>,
        reason: String,
    },
}

/// Draft-then-decode envelope containing a chain-of-thought scratchpad ahead of payload fields.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct ExtractionEnvelope<T = ExtractionDirective> {
    /// Unconstrained chain-of-thought scratchpad where the model reasons freely
    #[serde(alias = "reasoning_scratchpad")]
    pub chain_of_thought: String,

    /// Final deterministic extraction directive
    pub directive: T,
}

/// Generates a valid JSON Schema draft definition for `ExtractionEnvelope<ExtractionDirective>`.
pub fn generate_extraction_schema() -> serde_json::Value {
    generate_schema_for::<ExtractionEnvelope<ExtractionDirective>>()
}

/// Generates a JSON Schema value for any type implementing `JsonSchema`.
pub fn generate_schema_for<T: JsonSchema>() -> serde_json::Value {
    let schema = schemars::schema_for!(T);
    serde_json::to_value(&schema).expect("Failed to serialize generated JSON schema")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_schema_validation() {
        let schema_val = generate_extraction_schema();
        println!(
            "SCHEMA:\n{}",
            serde_json::to_string_pretty(&schema_val).unwrap()
        );
        assert!(
            schema_val.is_object(),
            "Generated schema must be a JSON object"
        );

        // Validate that schema can compile in a JSON Schema validator
        let compiled = jsonschema::validator_for(&schema_val);
        assert!(
            compiled.is_ok(),
            "Generated schema must be valid JSON Schema: {:?}",
            compiled.err()
        );
        let validator = compiled.expect("validator");

        // Verify valid instance validates successfully
        let valid_instance = serde_json::json!({
            "chain_of_thought": "The entity matches parent X directly based on SKU.",
            "directive": {
                "action": "insert_node",
                "parent_key": "cat_10",
                "entity_name": "Almond Milk",
                "entity_level": "sku",
                "relationship": "CHILD_OF",
                "confidence": 0.95
            }
        });

        let validation_res = validator.validate(&valid_instance);
        assert!(
            validation_res.is_ok(),
            "Valid envelope instance must pass schema validation"
        );
    }

    #[test]
    fn test_directive_roundtrip_serialization() {
        let directive = ExtractionDirective::InsertNode {
            parent_key: "root_1".into(),
            entity_name: "Espresso".into(),
            entity_level: "product".into(),
            relationship: "BELONGS_TO".into(),
            confidence: 0.98,
        };

        let json = serde_json::to_string(&directive).expect("serialize");
        let parsed: ExtractionDirective = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(directive, parsed);
    }
}
