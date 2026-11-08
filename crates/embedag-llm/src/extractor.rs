//! Generalized entity extraction from prompt records and structured payloads.
//!
//! Provides domain-agnostic heuristics, configurable key priorities, and declarative
//! macros to eliminate application-specific hardcoding across inference pipelines.

use serde_json::Value;

/// Checks whether a JSON property key matches general identification naming conventions.
///
/// Matches standard keys ("name", "title", "label", "id", "key") and common affix patterns
/// (`_name`, `_title`, `_label`, `_id`, `_key`) case-insensitively when heuristics are enabled.
pub fn is_heuristic_entity_key(key: &str) -> bool {
    heuristic_rank(key).is_some()
}

/// How strongly a key names an entity: `Some(0)` for descriptive keys (`name`, `title`, `label` and
/// their affixed forms), `Some(1)` for identifiers (`id`, `key`, `*_id`, `*_key`), `None` otherwise.
/// Descriptive keys win over identifiers, so `{"category_id": 7, "name": "Milk"}` reads as `Milk`.
fn heuristic_rank(key: &str) -> Option<u8> {
    let lower = key.to_lowercase();
    let descriptive = matches!(lower.as_str(), "name" | "title" | "label")
        || lower.ends_with("_name")
        || lower.ends_with("_title")
        || lower.ends_with("_label")
        || lower.starts_with("name_")
        || lower.starts_with("title_")
        || lower.starts_with("label_");
    if descriptive {
        return Some(0);
    }
    let identifier =
        matches!(lower.as_str(), "id" | "key") || lower.ends_with("_id") || lower.ends_with("_key");
    identifier.then_some(1)
}

/// Extracts the raw record payload string from a prompt containing the `### INCOMING RECORD\n` section.
pub fn extract_record_slice(prompt: &str) -> Option<&str> {
    if let Some(start) = prompt.find("### INCOMING RECORD\n") {
        let rest = &prompt[start + "### INCOMING RECORD\n".len()..];
        let end = rest.find("\n\n###").unwrap_or(rest.len());
        let slice = rest[..end].trim();
        if !slice.is_empty() {
            return Some(slice);
        }
    }
    None
}

/// Contract for extracting human-readable entity labels from structured records and prompt envelopes.
pub trait EntityExtractor: Send + Sync {
    /// Extracts a representative entity text label from a parsed JSON value.
    fn extract_from_value(&self, value: &Value) -> Option<String>;

    /// Extracts a representative entity text label from a prompt string.
    fn extract_from_prompt(&self, prompt: &str) -> String {
        extract_record_slice(prompt)
            .and_then(|slice| {
                if let Ok(v) = serde_json::from_str::<Value>(slice) {
                    self.extract_from_value(&v)
                } else if !slice.is_empty() {
                    Some(slice.to_string())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| "Unidentified Entity".to_string())
    }
}

/// Developer-defined entity property contract inspired by Diesel schema declarations.
///
/// Implemented by developers or generated via [`entity_properties!`] to provide 100% control
/// over attribute mapping and primary label extraction.
pub trait EntityProperties: Send + Sync {
    /// Returns the slice of attribute keys configured by the developer to identify this entity.
    fn property_keys(&self) -> &[&'static str];

    /// Extracts the primary label for this entity from a JSON Value using developer-defined keys.
    fn extract_label(&self, value: &Value) -> Option<String> {
        let Value::Object(map) = value else {
            return None;
        };
        for key in self.property_keys() {
            if let Some(label) = map.get(*key).and_then(scalar_label) {
                return Some(label);
            }
        }
        None
    }
}

/// The label a JSON scalar carries: a non-empty trimmed string, or a number or boolean as text.
fn scalar_label(val: &Value) -> Option<String> {
    if let Some(s) = val.as_str() {
        let trimmed = s.trim();
        return (!trimmed.is_empty()).then(|| trimmed.to_string());
    }
    (val.is_number() || val.is_boolean()).then(|| val.to_string())
}

/// Configuration governing entity text extraction strategies, key priorities, and fallbacks.
#[derive(Debug, Clone, PartialEq)]
pub struct EntityExtractorConfig {
    /// Prioritized list of developer-configured keys to evaluate first.
    pub priority_keys: Vec<String>,
    /// Whether to evaluate suffix-based heuristics (`*_name`, `*_title`, `*_label`, `*_id`, `*_key`).
    pub enable_suffix_heuristics: bool,
    /// Whether to fallback to any non-empty string attribute present in the JSON object.
    pub fallback_to_any_string: bool,
    /// Whether to fallback to the raw text slice if JSON parsing fails or yields no label.
    pub fallback_to_slice: bool,
    /// Default label returned when no entity text can be identified.
    pub default_label: String,
}

impl Default for EntityExtractorConfig {
    fn default() -> Self {
        Self {
            priority_keys: Vec::new(),
            enable_suffix_heuristics: true,
            fallback_to_any_string: true,
            fallback_to_slice: true,
            default_label: "Unidentified Entity".to_string(),
        }
    }
}

impl EntityExtractorConfig {
    /// Creates a new `EntityExtractorConfig` initialized with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets application-specific priority keys.
    pub fn with_keys<I, S>(mut self, keys: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.priority_keys = keys.into_iter().map(Into::into).collect();
        self
    }

    /// Appends a single priority key.
    pub fn with_priority_key(mut self, key: impl Into<String>) -> Self {
        self.priority_keys.push(key.into());
        self
    }

    /// Controls whether suffix-based heuristics (`*_name`, `*_title`, etc.) are checked.
    pub fn enable_suffix_heuristics(mut self, enable: bool) -> Self {
        self.enable_suffix_heuristics = enable;
        self
    }

    /// Controls whether to fallback to the first non-empty string in a JSON object.
    pub fn fallback_to_any_string(mut self, fallback: bool) -> Self {
        self.fallback_to_any_string = fallback;
        self
    }

    /// Sets the fallback label returned if entity extraction fails entirely.
    pub fn with_default_label(mut self, label: impl Into<String>) -> Self {
        self.default_label = label.into();
        self
    }
}

impl EntityExtractorConfig {
    /// The label of a JSON object: a configured priority key, else (when enabled) a heuristic key with
    /// descriptive keys before identifiers, else (when enabled) any non-empty string.
    fn extract_from_object(&self, map: &serde_json::Map<String, Value>) -> Option<String> {
        let by_priority_key = self
            .priority_keys
            .iter()
            .find_map(|key| map.get(key).and_then(scalar_label));
        by_priority_key
            .or_else(|| {
                self.enable_suffix_heuristics
                    .then(|| heuristic_label(map))
                    .flatten()
            })
            .or_else(|| {
                self.fallback_to_any_string
                    .then(|| first_non_empty_string(map))
                    .flatten()
            })
    }
}

/// The label under the best-ranked heuristic key (see [`heuristic_rank`]): `name`-like keys first, then
/// `id`/`key`-like ones; within a rank, the first key in map order.
fn heuristic_label(map: &serde_json::Map<String, Value>) -> Option<String> {
    (0..=1).find_map(|rank| {
        map.iter()
            .filter(|(k, _)| heuristic_rank(k) == Some(rank))
            .find_map(|(_, val)| scalar_label(val))
    })
}

/// The first string member of `map` that is not blank, trimmed.
fn first_non_empty_string(map: &serde_json::Map<String, Value>) -> Option<String> {
    map.values()
        .filter_map(Value::as_str)
        .map(str::trim)
        .find(|s| !s.is_empty())
        .map(str::to_string)
}

impl EntityExtractor for EntityExtractorConfig {
    fn extract_from_value(&self, value: &Value) -> Option<String> {
        match value {
            Value::String(s) => Some(s.trim()).filter(|t| !t.is_empty()).map(str::to_string),
            Value::Object(map) => self.extract_from_object(map),
            Value::Number(n) => Some(n.to_string()),
            Value::Bool(b) => Some(b.to_string()),
            _ => None,
        }
    }

    fn extract_from_prompt(&self, prompt: &str) -> String {
        if let Some(slice) = extract_record_slice(prompt) {
            if let Ok(v) = serde_json::from_str::<Value>(slice)
                && let Some(extracted) = self.extract_from_value(&v)
            {
                return extracted;
            }
            if self.fallback_to_slice && !slice.is_empty() {
                return slice.to_string();
            }
        }
        self.default_label.clone()
    }
}

/// Extracts entity text using default, domain-agnostic heuristics.
pub fn extract_incoming_entity_text(prompt: &str) -> String {
    EntityExtractorConfig::default().extract_from_prompt(prompt)
}

/// Extracts entity text using application-specified priority keys followed by standard heuristics.
pub fn extract_incoming_entity_text_with_keys(prompt: &str, candidate_keys: &[&str]) -> String {
    let config = EntityExtractorConfig {
        priority_keys: candidate_keys.iter().map(|s| s.to_string()).collect(),
        ..Default::default()
    };
    config.extract_from_prompt(prompt)
}

/// Extracts entity text using an explicit configuration.
pub fn extract_incoming_entity_text_with_config(
    prompt: &str,
    config: &EntityExtractorConfig,
) -> String {
    config.extract_from_prompt(prompt)
}

/// Extracts entity text directly from a `serde_json::Value`.
pub fn extract_entity_text_from_value(
    value: &Value,
    preferred_keys: Option<&[&str]>,
) -> Option<String> {
    let mut config = EntityExtractorConfig::default();
    if let Some(keys) = preferred_keys {
        config.priority_keys = keys.iter().map(|s| s.to_string()).collect();
    }
    config.extract_from_value(value)
}

/// Extracts an entity label from a prompt or JSON string using general heuristics or custom keys.
///
/// # Examples
/// ```rust
/// use embedag_llm::extract_entity_text;
///
/// let prompt = "### INCOMING RECORD\n{\"sku\": \"SKU-001\"}\n\n### CANDIDATE PARENT";
///
/// // Application-specific priority keys:
/// let name = extract_entity_text!(prompt, ["sku", "barcode"]);
/// assert_eq!(name, "SKU-001");
/// ```
#[macro_export]
macro_rules! extract_entity_text {
    ($prompt:expr) => {
        $crate::extractor::extract_incoming_entity_text($prompt)
    };
    ($prompt:expr, [ $($key:expr),* $(,)? ]) => {
        $crate::extractor::extract_incoming_entity_text_with_keys($prompt, &[ $($key),* ])
    };
}

/// Declarative macro enabling developers to define their entity properties and schema mappings.
///
/// Follows the Diesel DSL concept, giving developers 100% compile-time control over
/// which attributes, keys, and default labels define their domain entities.
///
/// # Example
/// ```rust
/// use embedag_llm::{entity_properties, EntityExtractor};
///
/// entity_properties! {
///     ProductRecord {
///         keys: ["sku", "barcode", "product_title"],
///         default_label: "Unknown Product",
///     }
/// }
///
/// let record = ProductRecord;
/// let prompt = "### INCOMING RECORD\n{\"sku\": \"SKU-001\"}\n\n###";
/// assert_eq!(record.extract_from_prompt(prompt), "SKU-001");
/// ```
#[macro_export]
macro_rules! entity_properties {
    (
        $struct_name:ident {
            keys: [ $($key:expr),* $(,)? ] $(,)?
            $(default_label: $default_label:expr,)?
        }
    ) => {
        #[derive(Debug, Clone, Copy, Default)]
        pub struct $struct_name;

        impl $struct_name {
            /// Inherent helper to extract an entity label from a prompt string.
            pub fn extract_from_prompt(&self, prompt: &str) -> ::std::string::String {
                <Self as $crate::extractor::EntityExtractor>::extract_from_prompt(self, prompt)
            }
        }

        impl $crate::extractor::EntityProperties for $struct_name {
            fn property_keys(&self) -> &[&'static str] {
                &[ $($key),* ]
            }
        }

        impl $crate::extractor::EntityExtractor for $struct_name {
            fn extract_from_value(&self, value: &::serde_json::Value) -> ::std::option::Option<::std::string::String> {
                $crate::extractor::EntityProperties::extract_label(self, value)
            }

            fn extract_from_prompt(&self, prompt: &str) -> ::std::string::String {
                #[allow(unused_mut, unused_variables, unused_assignments)]
                let mut fallback = "Unidentified Entity";
                $(fallback = $default_label;)?

                $crate::extractor::extract_record_slice(prompt)
                    .and_then(|slice| {
                        if let Ok(v) = ::serde_json::from_str::<::serde_json::Value>(slice) {
                            $crate::extractor::EntityProperties::extract_label(self, &v)
                        } else if !slice.is_empty() {
                            Some(slice.to_string())
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(|| fallback.to_string())
            }
        }
    };
}

/// Defines an application-specific entity extractor with prioritized domain keys.
///
/// Generates a struct implementing [`EntityExtractor`] and an associated extraction function.
#[macro_export]
macro_rules! define_entity_extractor {
    ($extractor_name:ident, [ $($key:expr),* $(,)? ]) => {
        #[derive(Debug, Clone, Default)]
        pub struct $extractor_name;

        impl $crate::extractor::EntityExtractor for $extractor_name {
            fn extract_from_value(&self, value: &::serde_json::Value) -> ::std::option::Option<::std::string::String> {
                $crate::extractor::extract_entity_text_from_value(value, Some(&[ $($key),* ]))
            }

            fn extract_from_prompt(&self, prompt: &str) -> ::std::string::String {
                $crate::extractor::extract_incoming_entity_text_with_keys(prompt, &[ $($key),* ])
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::name(r#"{"name": "Central Station"}"#, "Central Station")]
    #[case::title(r#"{"title": "Chief Architect"}"#, "Chief Architect")]
    #[case::label(r#"{"label": "High Priority"}"#, "High Priority")]
    // Domain-specific keys like item_name, category_name, product_title match automatically
    #[case::item_name(r#"{"item_name": "Fairlife 2% Milk"}"#, "Fairlife 2% Milk")]
    #[case::category_name(r#"{"category_name": "Dairy & Eggs"}"#, "Dairy & Eggs")]
    #[case::product_title(r#"{"product_title": "Mechanical Keyboard"}"#, "Mechanical Keyboard")]
    #[case::device_id(r#"{"device_id": "DEV-4091"}"#, "DEV-4091")]
    // A descriptive key wins over an identifier, whatever the map order.
    #[case::name_over_id(r#"{"category_id": 7, "name": "Milk"}"#, "Milk")]
    #[case::title_over_key(r#"{"api_key": "k-1", "title": "Chief Architect"}"#, "Chief Architect")]
    #[case::item_label_over_id(r#"{"a_id": 1, "item_label": "Fresh"}"#, "Fresh")]
    #[case::any_string(r#"{"arbitrary_key": "Something unique"}"#, "Something unique")]
    #[case::raw_slice("Raw Unformatted Entity", "Raw Unformatted Entity")]
    #[case::nothing_to_extract("", "Unidentified Entity")]
    fn extracts_the_entity_text(#[case] record: &str, #[case] expected: &str) {
        let prompt = format!("### INCOMING RECORD\n{record}\n\n###");
        assert_eq!(extract_incoming_entity_text(&prompt), expected);
    }

    #[test]
    fn test_custom_priority_keys() {
        let prompt =
            "### INCOMING RECORD\n{\"sku\": \"SKU-999\", \"title\": \"Generic Title\"}\n\n###";
        // Default prioritizes title over unknown key sku
        assert_eq!(extract_incoming_entity_text(prompt), "Generic Title");

        // Custom keys prioritize sku
        let extracted = extract_incoming_entity_text_with_keys(prompt, &["sku", "title"]);
        assert_eq!(extracted, "SKU-999");
    }

    #[test]
    fn test_macros() {
        let prompt = "### INCOMING RECORD\n{\"custom_alias\": \"Alpha Centauri\", \"name\": \"Star System\"}\n\n###";

        let default_name = extract_entity_text!(prompt);
        assert_eq!(default_name, "Star System");

        let custom_name = extract_entity_text!(prompt, ["custom_alias", "name"]);
        assert_eq!(custom_name, "Alpha Centauri");

        define_entity_extractor!(AstronomyExtractor, ["custom_alias", "name"]);
        let extractor = AstronomyExtractor;
        assert_eq!(extractor.extract_from_prompt(prompt), "Alpha Centauri");
    }

    #[test]
    fn test_entity_properties_macro() {
        entity_properties! {
            PatientRecord {
                keys: ["patient_id", "medical_record_number", "mrn"],
                default_label: "Unknown Patient",
            }
        }

        let record = PatientRecord;
        let prompt = "### INCOMING RECORD\n{\"patient_id\": \"PT-99881\"}\n\n###";
        assert_eq!(record.extract_from_prompt(prompt), "PT-99881");

        let prompt_empty = "### INCOMING RECORD\n{}\n\n###";
        assert_eq!(record.extract_from_prompt(prompt_empty), "Unknown Patient");
    }

    use serde_json::json;

    fn prompt_with(record: &str) -> String {
        format!("### INCOMING RECORD\n{record}\n\n###")
    }

    #[rstest]
    #[case::name("name", true)]
    #[case::uppercase("TITLE", true)]
    #[case::suffix("item_label", true)]
    #[case::prefix("name_full", true)]
    #[case::title_prefix("title_main", true)]
    #[case::label_prefix("label_text", true)]
    #[case::identifier("id", true)]
    #[case::key_suffix("api_key", true)]
    #[case::other("colour", false)]
    fn recognizes_entity_keys(#[case] key: &str, #[case] expected: bool) {
        assert_eq!(is_heuristic_entity_key(key), expected);
    }

    #[rstest]
    #[case::missing_header("no header here", None)]
    #[case::blank_record("### INCOMING RECORD\n   \n\n###", None)]
    #[case::trimmed("### INCOMING RECORD\n  abc  \n\n###", Some("abc"))]
    #[case::runs_to_the_end("### INCOMING RECORD\nabc", Some("abc"))]
    fn finds_the_record_slice(#[case] prompt: &str, #[case] expected: Option<&str>) {
        assert_eq!(extract_record_slice(prompt), expected);
    }

    #[rstest]
    #[case::string(json!("  Milk  "), Some("Milk"))]
    #[case::blank_string(json!("   "), None)]
    #[case::number(json!(42), Some("42"))]
    #[case::boolean(json!(true), Some("true"))]
    #[case::null(json!(null), None)]
    #[case::array(json!(["x"]), None)]
    #[case::object_without_labels(json!({"n": null}), None)]
    fn extracts_from_values(#[case] value: Value, #[case] expected: Option<&str>) {
        let config = EntityExtractorConfig::new();
        assert_eq!(config.extract_from_value(&value).as_deref(), expected);
    }

    #[test]
    fn the_configuration_builders_steer_extraction() {
        let config = EntityExtractorConfig::new()
            .with_keys(["sku"])
            .with_priority_key("barcode")
            .enable_suffix_heuristics(false)
            .fallback_to_any_string(false)
            .with_default_label("Nothing");
        assert_eq!(config.priority_keys, vec!["sku", "barcode"]);

        let value = json!({"barcode": 12345, "name": "Ignored"});
        assert_eq!(config.extract_from_value(&value).as_deref(), Some("12345"));
        // With heuristics and the any-string fallback both off, an unlisted key finds nothing.
        let unlisted = json!({"name": "Ignored", "other": "x"});
        assert_eq!(config.extract_from_value(&unlisted), None);
        assert_eq!(
            extract_incoming_entity_text_with_config("no record", &config),
            "Nothing"
        );
    }

    #[test]
    fn the_slice_fallback_can_be_switched_off() {
        let config = EntityExtractorConfig {
            fallback_to_slice: false,
            ..EntityExtractorConfig::default()
        };
        assert_eq!(
            config.extract_from_prompt(&prompt_with("plain text")),
            "Unidentified Entity"
        );
        assert_eq!(
            config.extract_from_prompt("no record at all"),
            "Unidentified Entity"
        );
    }

    #[test]
    fn values_can_be_read_with_or_without_preferred_keys() {
        let value = json!({"sku": "S-1", "name": "Milk"});
        assert_eq!(
            extract_entity_text_from_value(&value, None).as_deref(),
            Some("Milk")
        );
        assert_eq!(
            extract_entity_text_from_value(&value, Some(&["sku"])).as_deref(),
            Some("S-1")
        );
    }

    struct Fixed;

    impl EntityExtractor for Fixed {
        fn extract_from_value(&self, value: &Value) -> Option<String> {
            value.get("x").and_then(Value::as_str).map(str::to_string)
        }
    }

    #[rstest]
    #[case::json(r#"{"x": "found"}"#, "found")]
    #[case::json_without_the_key(r#"{"y": "nope"}"#, "Unidentified Entity")]
    #[case::not_json("free text", "free text")]
    fn the_default_prompt_extraction_uses_the_value_extractor(
        #[case] record: &str,
        #[case] expected: &str,
    ) {
        assert_eq!(Fixed.extract_from_prompt(&prompt_with(record)), expected);
        assert_eq!(
            Fixed.extract_from_prompt("no record"),
            "Unidentified Entity"
        );
    }

    struct Keyed;

    impl EntityProperties for Keyed {
        fn property_keys(&self) -> &[&'static str] {
            &["sku", "count", "flag"]
        }
    }

    #[rstest]
    #[case::string(json!({"sku": " A "}), Some("A"))]
    #[case::number(json!({"count": 3}), Some("3"))]
    #[case::boolean(json!({"flag": false}), Some("false"))]
    #[case::blank(json!({"sku": "  "}), None)]
    #[case::not_an_object(json!("A"), None)]
    fn developer_defined_keys_label_an_object(
        #[case] value: Value,
        #[case] expected: Option<&str>,
    ) {
        assert_eq!(Keyed.extract_label(&value).as_deref(), expected);
    }

    #[test]
    fn the_entity_properties_macro_handles_odd_records() {
        entity_properties! {
            Plain {
                keys: ["sku"],
            }
        }
        let record = Plain;
        assert_eq!(record.property_keys(), &["sku"]);
        assert_eq!(
            record.extract_from_prompt(&prompt_with("not json")),
            "not json"
        );
        assert_eq!(record.extract_from_prompt("nothing"), "Unidentified Entity");
        let value = json!({"sku": "S-2"});
        assert_eq!(
            EntityExtractor::extract_from_value(&record, &value).as_deref(),
            Some("S-2")
        );
    }

    #[test]
    fn the_defined_extractor_reads_values_too() {
        define_entity_extractor!(Barcodes, ["barcode"]);
        let value = json!({"barcode": "B-1"});
        assert_eq!(Barcodes.extract_from_value(&value).as_deref(), Some("B-1"));
    }
}
