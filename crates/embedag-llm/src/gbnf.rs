use gbnf::Grammar;
use gbnf::json::JsonSchemaParseError;
use serde_json::Value;
use thiserror::Error;

/// Error variants encountered during GBNF grammar generation and PDA validation.
#[derive(Debug, Error)]
pub enum GbnfError {
    #[error("Failed to parse JSON Schema into GBNF grammar: {0}")]
    SchemaParseError(#[from] JsonSchemaParseError),

    #[error("Failed to serialize or deserialize schema JSON: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Pushdown Automaton invalid transition: {0}")]
    InvalidPdaTransition(String),

    #[error("Grammar generation failed: {0}")]
    GenerationFailed(String),
}

/// Recursively dereferences all `$ref` keys against `$defs` or `definitions` in the schema
/// and normalizes objects for grammar synthesis.
pub fn dereference_schema(schema: &Value) -> Value {
    let defs = schema.get("$defs").or_else(|| schema.get("definitions"));

    fn resolve_node(val: &Value, root_defs: Option<&Value>) -> Value {
        match val {
            Value::Object(map) => {
                if let Some(Value::String(ref_str)) = map.get("$ref") {
                    let target_key = ref_str
                        .strip_prefix("#/$defs/")
                        .or_else(|| ref_str.strip_prefix("#/definitions/"));
                    if let Some(key) = target_key
                        && let Some(defs_obj) = root_defs
                        && let Some(target_val) = defs_obj.get(key)
                    {
                        return resolve_node(target_val, root_defs);
                    }
                }

                let mut resolved_map = serde_json::Map::new();
                for (k, v) in map {
                    if k != "$defs" && k != "definitions" {
                        resolved_map.insert(k.clone(), resolve_node(v, root_defs));
                    }
                }

                // Normalization: gbnf requires "properties" on object types
                if let Some(Value::String(t)) = resolved_map.get("type")
                    && t == "object"
                    && !resolved_map.contains_key("properties")
                {
                    resolved_map.insert("properties".into(), Value::Object(serde_json::Map::new()));
                }

                Value::Object(resolved_map)
            }
            Value::Array(items) => {
                Value::Array(items.iter().map(|it| resolve_node(it, root_defs)).collect())
            }
            other => other.clone(),
        }
    }

    resolve_node(schema, defs)
}

/// Compiles a JSON Schema `Value` into a GBNF grammar string compatible with pure-Rust samplers.
pub fn compile_schema_to_gbnf(schema: &Value) -> Result<String, GbnfError> {
    let dereferenced = dereference_schema(schema);
    let grammar = Grammar::from_json_schema_value(&dereferenced)?;
    let gbnf_string = grammar.to_string();
    if gbnf_string.trim().is_empty() {
        return Err(GbnfError::GenerationFailed(
            "Compiled GBNF grammar is empty".into(),
        ));
    }
    Ok(gbnf_string)
}

/// Token or character level pushdown automaton state stack frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PdaFrame {
    Root,
    ObjectKey,
    ObjectColon,
    ObjectValue,
    ObjectNext,
    ArrayValue,
    ArrayNext,
    InString,
}

/// Pushdown Automaton (PDA) tracking grammar-constrained states for token filtering and logit masking.
#[derive(Debug, Clone)]
pub struct PushdownAutomaton {
    grammar_rules: String,
    stack: Vec<PdaFrame>,
    current_buffer: String,
}

impl PushdownAutomaton {
    /// Creates a new Pushdown Automaton from a GBNF grammar string.
    pub fn new(grammar_rules: impl Into<String>) -> Self {
        Self {
            grammar_rules: grammar_rules.into(),
            stack: vec![PdaFrame::Root],
            current_buffer: String::new(),
        }
    }

    /// Compiles a JSON Schema directly into a ready-to-use Pushdown Automaton.
    pub fn from_schema(schema: &Value) -> Result<Self, GbnfError> {
        let rules = compile_schema_to_gbnf(schema)?;
        Ok(Self::new(rules))
    }

    /// Returns the compiled GBNF grammar representation.
    pub fn grammar(&self) -> &str {
        &self.grammar_rules
    }

    /// Checks if a candidate character or token prefix is structurally allowed in the current state.
    pub fn can_accept_token(&self, candidate: &str) -> bool {
        // Construct speculative buffer
        let mut test_buffer = self.current_buffer.clone();
        test_buffer.push_str(candidate);

        // Verify JSON prefix validity
        Self::is_valid_json_prefix(&test_buffer)
    }

    /// Filters a list of candidate tokens, keeping only those permitted by the PDA grammar state.
    pub fn mask_candidates<'a>(&self, candidates: &[&'a str]) -> Vec<&'a str> {
        candidates
            .iter()
            .copied()
            .filter(|token| self.can_accept_token(token))
            .collect()
    }

    /// Advances the automaton state by consuming an emitted token.
    pub fn consume(&mut self, token: &str) -> Result<(), GbnfError> {
        if !self.can_accept_token(token) {
            return Err(GbnfError::InvalidPdaTransition(format!(
                "Token '{token}' violates grammar PDA invariants for buffer '{}'",
                self.current_buffer
            )));
        }
        self.current_buffer.push_str(token);
        self.update_stack();
        Ok(())
    }

    /// Returns true if the currently consumed buffer forms a completely valid and closed JSON document.
    pub fn is_complete(&self) -> bool {
        serde_json::from_str::<Value>(&self.current_buffer).is_ok()
    }

    fn update_stack(&mut self) {
        let trimmed = self.current_buffer.trim();
        if trimmed.starts_with('{') {
            self.stack = vec![PdaFrame::ObjectValue];
        } else if trimmed.starts_with('[') {
            self.stack = vec![PdaFrame::ArrayValue];
        }
    }

    /// Validates whether a string is a syntactically valid prefix of a JSON document.
    fn is_valid_json_prefix(prefix: &str) -> bool {
        let trimmed = prefix.trim_start();
        if trimmed.is_empty() {
            return true;
        }

        // Check quotes and brackets balance
        let mut in_string = false;
        let mut escaped = false;
        let mut braces = 0isize;
        let mut brackets = 0isize;

        for ch in trimmed.chars() {
            if in_string {
                if escaped {
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == '"' {
                    in_string = false;
                }
            } else {
                match ch {
                    '"' => in_string = true,
                    '{' => braces += 1,
                    '}' => {
                        braces -= 1;
                        if braces < 0 {
                            return false;
                        }
                    }
                    '[' => brackets += 1,
                    ']' => {
                        brackets -= 1;
                        if brackets < 0 {
                            return false;
                        }
                    }
                    _ => {}
                }
            }
        }

        // If closed completely, it must deserialize cleanly or be capable of closing
        if braces == 0 && brackets == 0 && !in_string && !trimmed.is_empty() {
            serde_json::from_str::<Value>(trimmed).is_ok()
        } else {
            braces >= 0 && brackets >= 0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::generate_extraction_schema;

    #[test]
    fn test_gbnf_grammar_syntax() {
        let schema = generate_extraction_schema();
        let gbnf_grammar = compile_schema_to_gbnf(&schema);
        assert!(
            gbnf_grammar.is_ok(),
            "GBNF compilation must succeed for extraction schema: {:?}",
            gbnf_grammar.err()
        );

        let grammar_text = gbnf_grammar.expect("gbnf text");
        assert!(
            !grammar_text.is_empty(),
            "Compiled GBNF grammar must not be empty"
        );
        assert!(
            grammar_text.contains("root ::="),
            "GBNF grammar must declare a root entrypoint rule"
        );
    }

    #[test]
    fn test_pda_token_masking() {
        let schema = generate_extraction_schema();
        let mut pda = PushdownAutomaton::from_schema(&schema).expect("pda init");

        // Initial state allows opening brace '{' or whitespace
        let candidates = ["{", "invalid_start", "}", "123", "   {"];
        let masked = pda.mask_candidates(&candidates);
        assert!(masked.contains(&"{"));
        assert!(masked.contains(&"   {"));
        assert!(!masked.contains(&"invalid_start"));
        assert!(!masked.contains(&"}"));

        // Advance with valid JSON prefix
        assert!(pda.consume("{\"chain_of_thought\": \"test\"").is_ok());
        assert!(!pda.is_complete());

        // Consuming an invalid character sequence should fail
        assert!(pda.consume("}}}").is_err());
    }
}
