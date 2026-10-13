//! Generalized entity extraction from prompt records and structured payloads.
//!
//! Provides domain-agnostic heuristics, configurable key priorities, and declarative
//! macros to eliminate application-specific hardcoding across inference pipelines.

use serde_json::Value;

/// Standard, domain-agnostic keys tested when identifying the primary label of an entity.
///
/// These keys represent common semantic identity conventions across arbitrary domains.
pub const DEFAULT_ENTITY_KEYS: &[&str] = &["name", "title", "label", "id", "key"];

/// Checks whether a JSON property key matches general identification naming conventions.
///
/// Matches suffixes such as `_name`, `_title`, `_label`, `_id`, and `_key` (case-insensitive),
/// allowing arbitrary user schemas (e.g. `item_name`, `category_name`, `product_title`, `account_id`)
/// to be resolved without requiring hardcoded application terms in the core runtime.
pub fn is_heuristic_entity_key(key: &str) -> bool {
    let lower = key.to_lowercase();
    lower.ends_with("_name")
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

/// Configuration governing entity text extraction strategies, key priorities, and fallbacks.
#[derive(Debug, Clone, PartialEq)]
pub struct EntityExtractorConfig {
    /// Prioritized list of application-specific keys to evaluate first.
    pub priority_keys: Vec<String>,
    /// Whether to evaluate domain-agnostic standard keys (`["name", "title", "label", "id", "key"]`).
    pub include_default_keys: bool,
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
            include_default_keys: true,
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

    /// Controls whether domain-agnostic default keys are checked.
    pub fn include_default_keys(mut self, include: bool) -> Self {
        self.include_default_keys = include;
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
                // 1. Check user-configured priority keys first
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

                // 2. Check general default keys
                if self.include_default_keys {
                    for key in DEFAULT_ENTITY_KEYS {
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
                }

                // 3. Check suffix heuristics (*_name, *_title, *_label, *_id, *_key)
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

                // 4. Fallback to any non-empty string in the object
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
/// let prompt = "### INCOMING RECORD\n{\"item_name\": \"Oat Milk\"}\n\n### CANDIDATE PARENT";
///
/// // Domain-agnostic extraction (matches via suffix heuristic *_name):
/// let name = extract_entity_text!(prompt);
/// assert_eq!(name, "Oat Milk");
///
/// // Application-specific priority keys:
/// let name2 = extract_entity_text!(prompt, ["item_name", "category_name"]);
/// assert_eq!(name2, "Oat Milk");
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

/// Defines an application-specific entity extractor with prioritized domain keys.
///
/// Generates a struct implementing [`EntityExtractor`] and an associated extraction function.
///
/// # Examples
/// ```rust
/// use embedag_llm::{define_entity_extractor, EntityExtractor};
///
/// define_entity_extractor!(
///     ReceiptExtractor,
///     ["item_name", "category_name", "merchant_name"]
/// );
///
/// let extractor = ReceiptExtractor;
/// let prompt = "### INCOMING RECORD\n{\"item_name\": \"Whole Milk\"}\n\n###";
/// assert_eq!(extractor.extract_from_prompt(prompt), "Whole Milk");
/// ```
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
}
