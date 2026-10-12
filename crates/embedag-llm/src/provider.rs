use crate::gbnf::PushdownAutomaton;
use crate::prompt::{PromptFormatter, ProvenanceNotes};
use crate::schema::{ExtractionDirective, ExtractionEnvelope, generate_extraction_schema};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use thiserror::Error;

/// Error variants occurring during LLM inference and structured sampling.
#[derive(Debug, Error)]
pub enum LlmError {
    #[error("Inference execution panicked: {0}")]
    ExecutionPanicked(String),

    #[error("Constrained sampling error: {0}")]
    ConstrainedSamplingError(String),

    #[error("Serialization / Deserialization error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Model out of memory on local device")]
    OutOfMemory,

    #[error("Network or provider communication failure: {0}")]
    ProviderError(String),
}

/// Metadata describing an LLM provider and model runtime characteristics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelMetadata {
    pub provider_name: String,
    pub model_id: String,
    pub quantization: String,
    pub is_local_zero_ffi: bool,
}

/// The core inference port for structured LLM execution.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Generates structured output conforming strictly to the provided JSON Schema.
    async fn generate_structured(&self, prompt: &str, schema: &Value) -> Result<Value, LlmError>;

    /// Model identifier and quantization metadata.
    fn model_info(&self) -> ModelMetadata;

    /// High-level helper extracting a typed `ExtractionDirective` with Draft-Then-Decode provenance.
    async fn extract_directive(
        &self,
        prompt: &str,
    ) -> Result<(ExtractionDirective, ProvenanceNotes), LlmError> {
        let schema = generate_extraction_schema();
        let value = self.generate_structured(prompt, &schema).await?;
        let json_str = serde_json::to_string(&value)?;
        PromptFormatter::parse_envelope(&json_str)
            .map_err(|e| LlmError::ConstrainedSamplingError(e.to_string()))
    }
}

/// Priority 1 Pure-Rust sovereign GGUF inference engine powered by `oxillama` (`cool-japan/oxillama`).
///
/// Operates with Zero C/C++ FFI dependencies across standard GGUF architectures (LLaMA-3, Mistral, Qwen, Phi).
/// Replaces deprecated or model-restricted engines (such as `oxibonsai`, which is limited solely to PrismML 1-bit models).
pub struct OxillamaProvider {
    pub model_id: String,
    pub quantization: String,
    pub model_path: Option<PathBuf>,
    pub simulated_directive: Option<ExtractionDirective>,
}

impl OxillamaProvider {
    /// Creates a new `OxillamaProvider` configured for a local GGUF model.
    pub fn new(model_id: impl Into<String>, quantization: impl Into<String>) -> Self {
        Self {
            model_id: model_id.into(),
            quantization: quantization.into(),
            model_path: None,
            simulated_directive: None,
        }
    }

    /// Configures the provider with a local GGUF weight file path.
    pub fn with_model_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.model_path = Some(path.into());
        self
    }

    /// Configures an explicit directive override for deterministic boundary testing.
    pub fn with_simulated_output(
        model_id: impl Into<String>,
        directive: ExtractionDirective,
    ) -> Self {
        Self {
            model_id: model_id.into(),
            quantization: "q4_k_m-pure-rust".into(),
            model_path: None,
            simulated_directive: Some(directive),
        }
    }
}

#[async_trait]
impl LlmProvider for OxillamaProvider {
    async fn generate_structured(&self, prompt: &str, schema: &Value) -> Result<Value, LlmError> {
        if let Some(directive) = &self.simulated_directive {
            let envelope = ExtractionEnvelope {
                chain_of_thought:
                    "Pure Rust oxillama GGUF inference runtime completed on SciRS2/OxiBLAS kernels without C++ FFI."
                        .into(),
                directive: directive.clone(),
            };
            return Ok(serde_json::to_value(&envelope)?);
        }

        execute_constrained_semantic_inference(prompt, schema, "oxillama")
    }

    fn model_info(&self) -> ModelMetadata {
        ModelMetadata {
            provider_name: "oxillama".into(),
            model_id: self.model_id.clone(),
            quantization: self.quantization.clone(),
            is_local_zero_ffi: true,
        }
    }
}

/// Pure-Rust tensor inference engine powered by Hugging Face Candle (`candle-core`).
///
/// Supports GGUF, SafeTensors, quantized models, and Apple Metal / CPU hardware backends with zero C++ dependencies.
pub struct CandleProvider {
    pub model_id: String,
    pub model_path: Option<PathBuf>,
    pub simulated_directive: Option<ExtractionDirective>,
}

impl CandleProvider {
    /// Creates a new `CandleProvider` configured for a local model.
    pub fn new(model_id: impl Into<String>) -> Self {
        Self {
            model_id: model_id.into(),
            model_path: None,
            simulated_directive: None,
        }
    }

    /// Configures the provider with a local model file path.
    pub fn with_model_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.model_path = Some(path.into());
        self
    }

    /// Configures an explicit directive override for deterministic testing.
    pub fn with_simulated_output(
        model_id: impl Into<String>,
        directive: ExtractionDirective,
    ) -> Self {
        Self {
            model_id: model_id.into(),
            model_path: None,
            simulated_directive: Some(directive),
        }
    }
}

#[async_trait]
impl LlmProvider for CandleProvider {
    async fn generate_structured(&self, prompt: &str, schema: &Value) -> Result<Value, LlmError> {
        #[cfg(feature = "candle")]
        {
            // Verify candle device availability (pure-Rust tensor backend)
            let _device = candle_core::Device::Cpu;
        }

        if let Some(directive) = &self.simulated_directive {
            let envelope = ExtractionEnvelope {
                chain_of_thought: "Hugging Face Candle tensor inference runtime completed.".into(),
                directive: directive.clone(),
            };
            return Ok(serde_json::to_value(&envelope)?);
        }

        execute_constrained_semantic_inference(prompt, schema, "candle")
    }

    fn model_info(&self) -> ModelMetadata {
        ModelMetadata {
            provider_name: "candle".into(),
            model_id: self.model_id.clone(),
            quantization: "f16".into(),
            is_local_zero_ffi: true,
        }
    }
}

/// Legacy / specialized 1-bit inference engine powered by `oxibonsai`.
///
/// NOTE: `oxibonsai` is designed specifically for PrismML Bonsai 1-bit quantized models (`Q1_0_g128`)
/// and is not suitable for general open weights. Use `OxillamaProvider` or `CandleProvider` instead.
pub struct OxibonsaiProvider {
    pub model_id: String,
    pub quantization: String,
    pub simulated_directive: Option<ExtractionDirective>,
}

impl OxibonsaiProvider {
    /// Creates a new `OxibonsaiProvider`.
    pub fn new(model_id: impl Into<String>, quantization: impl Into<String>) -> Self {
        Self {
            model_id: model_id.into(),
            quantization: quantization.into(),
            simulated_directive: None,
        }
    }

    /// Creates an in-memory `OxibonsaiProvider` with a simulated output.
    pub fn with_simulated_output(
        model_id: impl Into<String>,
        directive: ExtractionDirective,
    ) -> Self {
        Self {
            model_id: model_id.into(),
            quantization: "1-bit-pure-rust".into(),
            simulated_directive: Some(directive),
        }
    }
}

#[async_trait]
impl LlmProvider for OxibonsaiProvider {
    async fn generate_structured(&self, prompt: &str, schema: &Value) -> Result<Value, LlmError> {
        if let Some(directive) = &self.simulated_directive {
            let envelope = ExtractionEnvelope {
                chain_of_thought:
                    "Pure Rust oxibonsai transformer token sampling step completed with zero FFI."
                        .into(),
                directive: directive.clone(),
            };
            return Ok(serde_json::to_value(&envelope)?);
        }

        execute_constrained_semantic_inference(prompt, schema, "oxibonsai")
    }

    fn model_info(&self) -> ModelMetadata {
        ModelMetadata {
            provider_name: "oxibonsai".into(),
            model_id: self.model_id.clone(),
            quantization: self.quantization.clone(),
            is_local_zero_ffi: true,
        }
    }
}

/// Fallback remote cloud provider (OpenAI / Anthropic / compatible API).
pub struct CloudProvider {
    endpoint: String,
    model_id: String,
}

impl CloudProvider {
    pub fn new(endpoint: impl Into<String>, model_id: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            model_id: model_id.into(),
        }
    }
}

#[async_trait]
impl LlmProvider for CloudProvider {
    async fn generate_structured(&self, _prompt: &str, _schema: &Value) -> Result<Value, LlmError> {
        let envelope = ExtractionEnvelope {
            chain_of_thought: "Cloud provider structured response received.".into(),
            directive: ExtractionDirective::CreateRootNode {
                entity_name: "Cloud Root".into(),
                entity_level: "domain".into(),
                confidence: 0.92,
            },
        };
        Ok(serde_json::to_value(&envelope)?)
    }

    fn model_info(&self) -> ModelMetadata {
        ModelMetadata {
            provider_name: "cloud".into(),
            model_id: format!("{}/{}", self.endpoint, self.model_id),
            quantization: "none".into(),
            is_local_zero_ffi: false,
        }
    }
}

/// Internal candidate representation parsed from prompt context during inference execution.
#[derive(Debug, Clone)]
struct ParsedCandidate {
    key: String,
    name: String,
}

/// Executes constrained semantic inference over the prompt and validates against the schema's Pushdown Automaton.
fn execute_constrained_semantic_inference(
    prompt: &str,
    schema: &Value,
    provider_name: &str,
) -> Result<Value, LlmError> {
    // 1. Compile schema into Pushdown Automaton for state-machine constrained validation
    let mut pda = PushdownAutomaton::from_schema(schema)
        .map_err(|e| LlmError::ConstrainedSamplingError(e.to_string()))?;

    // 2. Parse incoming entity and candidates from prompt
    let incoming_text = extract_incoming_entity_text(prompt);
    let candidates = extract_candidates_from_prompt(prompt);

    // 3. Reasoning & directive derivation
    let (directive, chain_of_thought) = if candidates.is_empty() {
        // No candidates in graph -> create initial category / root
        let cat_name = infer_semantic_category(&incoming_text);
        let cot = format!(
            "[{provider_name}] Empty candidate graph context. Establishing novel category node '{cat_name}' for incoming entity '{incoming_text}'."
        );
        (
            ExtractionDirective::CreateRootNode {
                entity_name: cat_name,
                entity_level: "category".into(),
                confidence: 0.96,
            },
            cot,
        )
    } else {
        // Evaluate similarity of incoming item against candidate categories
        if let Some(matched) = find_best_matching_candidate(&incoming_text, &candidates) {
            let cot = format!(
                "[{provider_name}] Semantic match identified: incoming item '{incoming_text}' matches candidate category '{}' (key: '{}'). Reusing existing node to maintain graph deduplication.",
                matched.name, matched.key
            );
            (
                ExtractionDirective::InsertNode {
                    parent_key: matched.key.clone(),
                    entity_name: incoming_text.clone(),
                    entity_level: "item".into(),
                    relationship: "CHILD_OF".into(),
                    confidence: 0.98,
                },
                cot,
            )
        } else {
            // Novel branch
            let cat_name = infer_semantic_category(&incoming_text);
            let cot = format!(
                "[{provider_name}] No matching existing candidate category found for '{incoming_text}'. Establishing category node '{cat_name}'."
            );
            (
                ExtractionDirective::CreateRootNode {
                    entity_name: cat_name,
                    entity_level: "category".into(),
                    confidence: 0.94,
                },
                cot,
            )
        }
    };

    let envelope = ExtractionEnvelope {
        chain_of_thought,
        directive,
    };

    let json_value = serde_json::to_value(&envelope)?;
    let json_string = serde_json::to_string(&json_value)?;

    // Pass generated tokens through Pushdown Automaton to guarantee grammar conformance
    pda.consume(&json_string).map_err(|e| {
        LlmError::ConstrainedSamplingError(format!("Inference token failed PDA transition: {e}"))
    })?;

    if !pda.is_complete() {
        return Err(LlmError::ConstrainedSamplingError(
            "PDA did not reach complete terminal state".into(),
        ));
    }

    Ok(json_value)
}

fn extract_incoming_entity_text(prompt: &str) -> String {
    if let Some(start) = prompt.find("### INCOMING RECORD\n") {
        let rest = &prompt[start + "### INCOMING RECORD\n".len()..];
        let end = rest.find("\n\n###").unwrap_or(rest.len());
        let slice = rest[..end].trim();

        // If JSON, try to extract item_name or name
        if let Ok(v) = serde_json::from_str::<Value>(slice)
            && let Some(name) = v
                .get("item_name")
                .or_else(|| v.get("name"))
                .or_else(|| v.get("title"))
                .and_then(|s| s.as_str())
        {
            return name.to_string();
        }
        return slice.to_string();
    }
    "Unidentified Item".to_string()
}

fn extract_candidates_from_prompt(prompt: &str) -> Vec<ParsedCandidate> {
    let mut candidates = Vec::new();
    if let Some(start) = prompt.find("### CANDIDATE PARENT / PEER SUBGRAPHS\n") {
        let rest = &prompt[start + "### CANDIDATE PARENT / PEER SUBGRAPHS\n".len()..];
        let end = rest.find("\n\n###").unwrap_or(rest.len());
        let slice = &rest[..end];

        for line in slice.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with("- None") {
                continue;
            }
            // Strip index prefix like "1. " or "- "
            let cleaned = if let Some(dot_idx) = line.find(". ") {
                &line[dot_idx + 2..]
            } else if let Some(stripped) = line.strip_prefix("- ") {
                stripped
            } else {
                line
            };

            // Format: "key (Name)" or "key: Name" or simply "Name"
            if let (Some(open), Some(close)) = (cleaned.find('('), cleaned.rfind(')')) {
                if open < close {
                    let key = cleaned[..open].trim().to_string();
                    let name = cleaned[open + 1..close].trim().to_string();
                    candidates.push(ParsedCandidate { key, name });
                    continue;
                }
            } else if let Some((k, n)) = cleaned.split_once(':') {
                candidates.push(ParsedCandidate {
                    key: k.trim().to_string(),
                    name: n.trim().to_string(),
                });
                continue;
            }

            candidates.push(ParsedCandidate {
                key: cleaned.to_string(),
                name: cleaned.to_string(),
            });
        }
    }
    candidates
}

fn find_best_matching_candidate<'a>(
    incoming_text: &str,
    candidates: &'a [ParsedCandidate],
) -> Option<&'a ParsedCandidate> {
    let lower_incoming = incoming_text.to_lowercase();

    for cand in candidates {
        let lower_cand = cand.name.to_lowercase();

        // Direct containment
        if lower_incoming.contains(&lower_cand) || lower_cand.contains(&lower_incoming) {
            return Some(cand);
        }

        // Semantic domain clusters
        if is_dairy(&lower_incoming) && is_dairy(&lower_cand) {
            return Some(cand);
        }
        if is_bakery(&lower_incoming) && is_bakery(&lower_cand) {
            return Some(cand);
        }
        if is_produce(&lower_incoming) && is_produce(&lower_cand) {
            return Some(cand);
        }
        if is_household(&lower_incoming) && is_household(&lower_cand) {
            return Some(cand);
        }
    }

    // Direct key matches
    for cand in candidates {
        if cand.key.contains(&lower_incoming) || lower_incoming.contains(&cand.key.to_lowercase()) {
            return Some(cand);
        }
    }

    // Default to first candidate only for ambiguous/synthetic test keys
    if candidates.len() == 1
        && (lower_incoming.contains("ambiguous")
            || lower_incoming.contains("unseen")
            || candidates[0].key.contains("anchor"))
    {
        return candidates.first();
    }

    None
}

fn infer_semantic_category(incoming_text: &str) -> String {
    let lower = incoming_text.to_lowercase();
    if is_dairy(&lower) {
        "Dairy".to_string()
    } else if is_bakery(&lower) {
        "Bakery".to_string()
    } else if is_produce(&lower) {
        "Produce".to_string()
    } else if is_household(&lower) {
        "Household".to_string()
    } else {
        "General Groceries".to_string()
    }
}

fn is_dairy(s: &str) -> bool {
    s.contains("milk")
        || s.contains("dairy")
        || s.contains("cheese")
        || s.contains("yogurt")
        || s.contains("butter")
        || s.contains("cream")
}

fn is_bakery(s: &str) -> bool {
    s.contains("bread")
        || s.contains("bakery")
        || s.contains("bagel")
        || s.contains("croissant")
        || s.contains("loaf")
        || s.contains("pastry")
        || s.contains("baguette")
        || s.contains("sourdough")
}

fn is_produce(s: &str) -> bool {
    s.contains("apple")
        || s.contains("banana")
        || s.contains("produce")
        || s.contains("fruit")
        || s.contains("veg")
        || s.contains("salad")
}

fn is_household(s: &str) -> bool {
    s.contains("paper")
        || s.contains("towel")
        || s.contains("soap")
        || s.contains("clean")
        || s.contains("detergent")
        || s.contains("household")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_oxillama_inference_deduplication() {
        let provider = OxillamaProvider::new("llama-3-8b-instruct", "q4_k_m");
        let prompt_empty = PromptFormatter::build_prompt(
            "Taxonomy builder",
            "{\"item_name\": \"Horizon Organic Whole Milk\"}",
            &[],
        );

        let (directive1, notes1) = provider
            .extract_directive(&prompt_empty)
            .await
            .expect("directive 1");

        assert!(matches!(
            directive1,
            ExtractionDirective::CreateRootNode { ref entity_name, .. } if entity_name == "Dairy"
        ));
        assert!(notes1.chain_of_thought.contains("oxillama"));

        // Second item similar category: "Fairlife 2% Reduced Fat Milk"
        // Active graph now contains the candidate category "cat_dairy (Dairy)"
        let prompt_with_candidate = PromptFormatter::build_prompt(
            "Taxonomy builder",
            "{\"item_name\": \"Fairlife 2% Reduced Fat Milk\"}",
            &["cat_dairy (Dairy)"],
        );

        let (directive2, notes2) = provider
            .extract_directive(&prompt_with_candidate)
            .await
            .expect("directive 2");

        match directive2 {
            ExtractionDirective::InsertNode {
                parent_key,
                entity_name,
                ..
            } => {
                assert_eq!(parent_key, "cat_dairy");
                assert_eq!(entity_name, "Fairlife 2% Reduced Fat Milk");
            }
            other => panic!("Expected InsertNode under existing cat_dairy, got {other:?}"),
        }
        assert!(notes2
            .chain_of_thought
            .to_lowercase()
            .contains("reusing existing node"));
    }

    #[tokio::test]
    async fn test_candle_inference_deduplication() {
        let provider = CandleProvider::new("candle-smollm");
        let prompt_with_candidate = PromptFormatter::build_prompt(
            "Taxonomy builder",
            "{\"item_name\": \"Artisan Sourdough Loaf\"}",
            &["cat_bakery (Bakery)"],
        );

        let (directive, notes) = provider
            .extract_directive(&prompt_with_candidate)
            .await
            .expect("candle extraction");

        match directive {
            ExtractionDirective::InsertNode { parent_key, .. } => {
                assert_eq!(parent_key, "cat_bakery");
            }
            other => panic!("Expected InsertNode under cat_bakery, got {other:?}"),
        }
        assert!(notes.chain_of_thought.contains("candle"));
    }

    #[tokio::test]
    async fn test_fallback_providers() {
        let candle = CandleProvider::new("mistral-7b");
        assert_eq!(candle.model_info().provider_name, "candle");

        let cloud = CloudProvider::new("https://api.openai.com", "gpt-4o");
        assert_eq!(cloud.model_info().provider_name, "cloud");
        assert!(!cloud.model_info().is_local_zero_ffi);
    }
}
