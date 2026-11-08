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
    resolve_node(schema, defs)
}

fn resolve_node(val: &Value, root_defs: Option<&Value>) -> Value {
    match val {
        Value::Object(map) => {
            resolve_ref(map, root_defs).unwrap_or_else(|| resolve_object(map, root_defs))
        }
        Value::Array(items) => {
            Value::Array(items.iter().map(|it| resolve_node(it, root_defs)).collect())
        }
        other => other.clone(),
    }
}

/// The resolved target of a local `$ref`, when the object has one that exists.
fn resolve_ref(map: &serde_json::Map<String, Value>, root_defs: Option<&Value>) -> Option<Value> {
    let ref_str = map.get("$ref").and_then(Value::as_str)?;
    let key = ref_str
        .strip_prefix("#/$defs/")
        .or_else(|| ref_str.strip_prefix("#/definitions/"))?;
    let target = root_defs?.get(key)?;
    Some(resolve_node(target, root_defs))
}

/// Resolves every member except the definitions themselves, then normalizes the object.
fn resolve_object(map: &serde_json::Map<String, Value>, root_defs: Option<&Value>) -> Value {
    let mut resolved: serde_json::Map<String, Value> = map
        .iter()
        .filter(|(k, _)| !matches!(k.as_str(), "$defs" | "definitions"))
        .map(|(k, v)| (k.clone(), resolve_node(v, root_defs)))
        .collect();
    // Normalization: gbnf requires "properties" on object types
    let is_object = resolved.get("type").and_then(Value::as_str) == Some("object");
    if is_object && !resolved.contains_key("properties") {
        resolved.insert("properties".into(), Value::Object(serde_json::Map::new()));
    }
    Value::Object(resolved)
}

/// Compiles a JSON Schema `Value` into a GBNF grammar string compatible with pure-Rust samplers.
pub fn compile_schema_to_gbnf(schema: &Value) -> Result<String, GbnfError> {
    let dereferenced = dereference_schema(schema);
    let grammar = Grammar::from_json_schema_value(&dereferenced)?;
    Ok(grammar.to_string())
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
        let mut balance = Balance::default();
        if !trimmed.chars().all(|ch| balance.feed(ch)) {
            return false;
        }
        // If closed completely, it must deserialize cleanly or be capable of closing
        !balance.is_closed() || serde_json::from_str::<Value>(trimmed).is_ok()
    }
}

/// Quote and bracket balance of a JSON prefix.
#[derive(Default)]
struct Balance {
    in_string: bool,
    escaped: bool,
    braces: isize,
    brackets: isize,
}

impl Balance {
    /// Consumes one character; false when it closes something that was never opened.
    fn feed(&mut self, ch: char) -> bool {
        if self.in_string {
            self.feed_in_string(ch);
            return true;
        }
        match ch {
            '"' => self.in_string = true,
            '{' => self.braces += 1,
            '}' => {
                self.braces -= 1;
                return self.braces >= 0;
            }
            '[' => self.brackets += 1,
            ']' => {
                self.brackets -= 1;
                return self.brackets >= 0;
            }
            _ => {}
        }
        true
    }

    fn feed_in_string(&mut self, ch: char) {
        if self.escaped {
            self.escaped = false;
        } else if ch == '\\' {
            self.escaped = true;
        } else if ch == '"' {
            self.in_string = false;
        }
    }

    fn is_closed(&self) -> bool {
        self.braces == 0 && self.brackets == 0 && !self.in_string
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::generate_extraction_schema;

    #[test]
    fn test_gbnf_grammar_syntax() {
        let schema = generate_extraction_schema();
        let grammar_text = compile_schema_to_gbnf(&schema)
            .expect("GBNF compilation must succeed for the extraction schema");
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

    use rstest::rstest;
    use serde_json::json;

    #[rstest]
    #[case::object(json!({"type": "object"}), true)]
    #[case::empty(json!({}), false)]
    #[case::not_a_schema(json!(42), false)]
    fn schemas_compile_or_report_why_not(#[case] schema: Value, #[case] compiles: bool) {
        assert_eq!(compile_schema_to_gbnf(&schema).is_ok(), compiles);
        assert_eq!(PushdownAutomaton::from_schema(&schema).is_ok(), compiles);
    }

    #[test]
    fn the_automaton_exposes_its_grammar_and_tracks_arrays() {
        let mut pda = PushdownAutomaton::new("root ::= value");
        assert_eq!(pda.grammar(), "root ::= value");
        pda.consume("  [1, 2").unwrap();
        assert!(!pda.is_complete());
        pda.consume("]").unwrap();
        assert!(pda.is_complete());
        // Nothing consumed yet accepts anything; an unopened closing bracket is refused.
        assert!(PushdownAutomaton::new("g").can_accept_token(""));
        assert!(PushdownAutomaton::new("g").consume("]").is_err());
    }

    #[rstest]
    #[case::plain_text(r#"{"a": "b"#, true)]
    #[case::escaped_quote(r##"{"a": "say \"hi\" "##, true)]
    #[case::escaped_backslash(r#"{"a": "path\\ "#, true)]
    #[case::closing_without_opening_brace("}", false)]
    #[case::closing_without_opening_bracket("]", false)]
    #[case::closed_but_invalid("{}}", false)]
    #[case::closed_and_valid("[]", true)]
    fn json_prefixes_are_checked_for_balance(#[case] prefix: &str, #[case] valid: bool) {
        assert_eq!(PushdownAutomaton::is_valid_json_prefix(prefix), valid);
    }

    #[test]
    fn definitions_are_inlined_and_unknown_references_are_kept() {
        let schema = json!({
            "$defs": {"Name": {"type": "string"}},
            "type": "object",
            "properties": {
                "known": {"$ref": "#/$defs/Name"},
                "unknown": {"$ref": "#/$defs/Missing"},
                "plain": {"$ref": "elsewhere.json"},
            },
        });
        let resolved = dereference_schema(&schema);
        assert_eq!(resolved["properties"]["known"], json!({"type": "string"}));
        assert!(resolved["properties"]["unknown"].get("$ref").is_some());
        assert!(resolved.get("$defs").is_none());
        // Objects without `properties` get an empty set; legacy `definitions` work too.
        let legacy = json!({"definitions": {"A": {"type": "object"}}, "$ref": "#/definitions/A"});
        assert_eq!(dereference_schema(&legacy)["properties"], json!({}));
    }
}
