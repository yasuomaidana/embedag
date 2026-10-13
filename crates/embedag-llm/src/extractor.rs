//! Generalized entity extraction from prompt records and structured payloads.
//!
//! Provides domain-agnostic heuristics, configurable key priorities, and declarative
//! macros to eliminate application-specific hardcoding across inference pipelines.

use serde_json::Value;

/// Deprecated placeholder for legacy code. Developers must configure their own domain keys
/// explicitly via [`entity_properties!`] or [`EntityExtractorConfig::with_keys`].
#[deprecated(
    note = "Define explicit domain keys via `entity_properties!` or `EntityExtractorConfig::with_keys`"
)]
pub const DEFAULT_ENTITY_KEYS: &[&str] = &[];

/// Checks whether a JSON property key matches general identification naming conventions.
///
/// Matches standard keys ("name", "title", "label", "id", "key") and common affix patterns
/// (`_name`, `_title`, `_label`, `_id`, `_key`) case-insensitively when heuristics are enabled.
pub fn is_heuristic_entity_key(key: &str) -> bool {
    let lower = key.to_lowercase();
    matches!(lower.as_str(), "name" | "title" | "label" | "id" | "key")
        || lower.ends_with("_name")
        || lower.ends_with("_title")
        || lower.ends_with("_label")
        || lower.ends_with("_id")
        || lower.ends_with("_key")
        || lower.starts_with("name_")
        || lower.starts_with("title_")
        || lower.starts_with("label_")
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
            if let Some(val) = map.get(*key) {
                if let Some(s) = val.as_str() {
                    let trimmed = s.trim();
                    if !trimmed.is_empty() {
                        return Some(trimmed.to_string());
                    }
                } else if val.is_number() || val.is_boolean() {
                    return Some(val.to_string());
                }
            }
        }
        None
    }
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

impl EntityExtractor for EntityExtractorConfig {
    fn extract_from_value(&self, value: &Value) -> Option<String> {
        match value {
            Value::String(s) => {
                let trimmed = s.trim();
                if !trimmed.is_empty() {
                    Some(trimmed.to_string())
                } else {
                    None
                }
            }
            Value::Object(map) => {
                // 1. Check user-configured priority keys first (100% developer control)
                for key in &self.priority_keys {
                    if let Some(val) = map.get(key) {
                        if let Some(s) = val.as_str() {
                            let trimmed = s.trim();
                            if !trimmed.is_empty() {
                                return Some(trimmed.to_string());
                            }
                        } else if val.is_number() || val.is_boolean() {
                            return Some(val.to_string());
                        }
                    }
                }

                // 2. Check suffix heuristics (*_name, *_title, *_label, *_id, *_key) if enabled
                if self.enable_suffix_heuristics {
                    for (k, val) in map {
                        if is_heuristic_entity_key(k) {
                            if let Some(s) = val.as_str() {
                                let trimmed = s.trim();
                                if !trimmed.is_empty() {
                                    return Some(trimmed.to_string());
                                }
                            } else if val.is_number() || val.is_boolean() {
                                return Some(val.to_string());
                            }
                        }
                    }
                }

                // 3. Fallback to any non-empty string in the object if enabled
                if self.fallback_to_any_string {
                    for (_k, val) in map {
                        if let Some(s) = val.as_str() {
                            let trimmed = s.trim();
                            if !trimmed.is_empty() {
                                return Some(trimmed.to_string());
                            }
                        }
                    }
                }

                None
            }
            Value::Number(n) => Some(n.to_string()),
            Value::Bool(b) => Some(b.to_string()),
            _ => None,
        }
    }

    fn extract_from_prompt(&self, prompt: &str) -> String {
        if let Some(slice) = extract_record_slice(prompt) {
            if let Ok(v) = serde_json::from_str::<Value>(slice) {
                if let Some(extracted) = self.extract_from_value(&v) {
                    return extracted;
                }
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
    let mut config = EntityExtractorConfig::default();
    config.priority_keys = candidate_keys.iter().map(|s| s.to_string()).collect();
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

    #[test]
    fn test_domain_agnostic_default_keys() {
        let prompt_name = "### INCOMING RECORD\n{\"name\": \"Central Station\"}\n\n###";
        assert_eq!(extract_incoming_entity_text(prompt_name), "Central Station");

        let prompt_title = "### INCOMING RECORD\n{\"title\": \"Chief Architect\"}\n\n###";
        assert_eq!(extract_incoming_entity_text(prompt_title), "Chief Architect");

        let prompt_label = "### INCOMING RECORD\n{\"label\": \"High Priority\"}\n\n###";
        assert_eq!(extract_incoming_entity_text(prompt_label), "High Priority");
    }

    #[test]
    fn test_suffix_heuristics() {
        // Domain-specific keys like item_name, category_name, product_title match automatically without hardcoding
        let prompt_item = "### INCOMING RECORD\n{\"item_name\": \"Fairlife 2% Milk\"}\n\n###";
        assert_eq!(extract_incoming_entity_text(prompt_item), "Fairlife 2% Milk");

        let prompt_cat = "### INCOMING RECORD\n{\"category_name\": \"Dairy & Eggs\"}\n\n###";
        assert_eq!(extract_incoming_entity_text(prompt_cat), "Dairy & Eggs");

        let prompt_product = "### INCOMING RECORD\n{\"product_title\": \"Mechanical Keyboard\"}\n\n###";
        assert_eq!(extract_incoming_entity_text(prompt_product), "Mechanical Keyboard");

        let prompt_id = "### INCOMING RECORD\n{\"device_id\": \"DEV-4091\"}\n\n###";
        assert_eq!(extract_incoming_entity_text(prompt_id), "DEV-4091");
    }

    #[test]
    fn test_custom_priority_keys() {
        let prompt = "### INCOMING RECORD\n{\"sku\": \"SKU-999\", \"title\": \"Generic Title\"}\n\n###";
        // Default prioritizes title over unknown key sku
        assert_eq!(extract_incoming_entity_text(prompt), "Generic Title");

        // Custom keys prioritize sku
        let extracted = extract_incoming_entity_text_with_keys(prompt, &["sku", "title"]);
        assert_eq!(extracted, "SKU-999");
    }

    #[test]
    fn test_fallback_to_any_string_and_slice() {
        let prompt_misc = "### INCOMING RECORD\n{\"arbitrary_key\": \"Something unique\"}\n\n###";
        assert_eq!(extract_incoming_entity_text(prompt_misc), "Something unique");

        let prompt_raw = "### INCOMING RECORD\nRaw Unformatted Entity\n\n###";
        assert_eq!(extract_incoming_entity_text(prompt_raw), "Raw Unformatted Entity");

        let prompt_empty = "### INCOMING RECORD\n\n\n###";
        assert_eq!(extract_incoming_entity_text(prompt_empty), "Unidentified Entity");
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
}
