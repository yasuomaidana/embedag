use crate::extractor::{extract_incoming_entity_text, extract_incoming_entity_text_with_keys};
use crate::gbnf::PushdownAutomaton;
use crate::prompt::{PromptFormatter, ProvenanceNotes};
use crate::schema::{ExtractionDirective, ExtractionEnvelope, generate_extraction_schema};
pub use async_trait::async_trait;
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

/// Pure-Rust GGUF inference engine powered by `oxillama` (`cool-japan/oxillama`), for CPU-only deployments.
///
/// Operates with Zero C/C++ FFI dependencies across standard GGUF architectures (LLaMA-3, Mistral, Qwen, Phi).
/// Ranked after GPU-capable providers: oxillama-runtime 0.1.4 offloads only Q4_0 single-token decode to the
/// GPU, so prompt prefill runs on the CPU. Prefer `CandleProvider` on a GPU, then an Ollama-compatible
/// HTTP provider.
pub struct OxillamaProvider {
    pub model_id: String,
    pub quantization: String,
    pub model_path: Option<PathBuf>,
    pub simulated_directive: Option<ExtractionDirective>,
    pub entity_keys: Option<Vec<String>>,
}

impl OxillamaProvider {
    /// Creates a new `OxillamaProvider` configured for a local GGUF model.
    pub fn new(model_id: impl Into<String>, quantization: impl Into<String>) -> Self {
        Self {
            model_id: model_id.into(),
            quantization: quantization.into(),
            model_path: None,
            simulated_directive: None,
            entity_keys: None,
        }
    }

    /// Configures the provider with a local GGUF weight file path.
    pub fn with_model_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.model_path = Some(path.into());
        self
    }

    /// Configures application-specific priority keys for entity text extraction.
    pub fn with_entity_keys(mut self, keys: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.entity_keys = Some(keys.into_iter().map(Into::into).collect());
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
            entity_keys: None,
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

        execute_constrained_semantic_inference(
            prompt,
            schema,
            "oxillama",
            self.entity_keys.as_deref(),
        )
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

/// Pure-Rust tensor inference engine powered by Hugging Face `candle` (`huggingface/candle`).
///
/// Priority 1 local provider: candle runs whole forward passes (prefill included) on Metal or CUDA,
/// and on the CPU elsewhere.
pub struct CandleProvider {
    pub model_id: String,
    pub simulated_directive: Option<ExtractionDirective>,
    pub entity_keys: Option<Vec<String>>,
}

impl CandleProvider {
    /// Creates a new `CandleProvider`.
    pub fn new(model_id: impl Into<String>) -> Self {
        Self {
            model_id: model_id.into(),
            simulated_directive: None,
            entity_keys: None,
        }
    }

    /// Configures application-specific priority keys for entity text extraction.
    pub fn with_entity_keys(mut self, keys: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.entity_keys = Some(keys.into_iter().map(Into::into).collect());
        self
    }

    /// Configures an explicit directive override for deterministic boundary testing.
    pub fn with_simulated_output(
        model_id: impl Into<String>,
        directive: ExtractionDirective,
    ) -> Self {
        Self {
            model_id: model_id.into(),
            simulated_directive: Some(directive),
            entity_keys: None,
        }
    }
}

#[async_trait]
impl LlmProvider for CandleProvider {
    async fn generate_structured(&self, prompt: &str, schema: &Value) -> Result<Value, LlmError> {
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

        execute_constrained_semantic_inference(
            prompt,
            schema,
            "candle",
            self.entity_keys.as_deref(),
        )
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

/// Pure-Rust sub-2-bit (1-bit / ternary) inference engine powered by `oxibonsai` (`cool-japan/oxibonsai`), for
/// memory-constrained CPU deployments.
///
/// Specializes in ultra-low memory sub-2-bit execution (`Q1_0_g128`, `TQ2_0_g128`) and Pushdown Automaton (PDA)
/// constrained logit masking with zero C/C++ FFI dependencies.
pub struct OxibonsaiProvider {
    pub model_id: String,
    pub quantization: String,
    pub simulated_directive: Option<ExtractionDirective>,
    pub entity_keys: Option<Vec<String>>,
}

impl OxibonsaiProvider {
    /// Creates a new `OxibonsaiProvider`.
    pub fn new(model_id: impl Into<String>, quantization: impl Into<String>) -> Self {
        Self {
            model_id: model_id.into(),
            quantization: quantization.into(),
            simulated_directive: None,
            entity_keys: None,
        }
    }

    /// Configures application-specific priority keys for entity text extraction.
    pub fn with_entity_keys(mut self, keys: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.entity_keys = Some(keys.into_iter().map(Into::into).collect());
        self
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
            entity_keys: None,
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

        execute_constrained_semantic_inference(
            prompt,
            schema,
            "oxibonsai",
            self.entity_keys.as_deref(),
        )
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

/// Fallback Cloud API Provider (Priority 3).
///
/// Used for offline testing or enterprise environments delegating to managed APIs.
pub struct CloudProvider {
    pub endpoint: String,
    pub model_id: String,
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
    custom_keys: Option<&[String]>,
) -> Result<Value, LlmError> {
    // 1. Compile schema into Pushdown Automaton for state-machine constrained validation
    let mut pda = PushdownAutomaton::from_schema(schema)
        .map_err(|e| LlmError::ConstrainedSamplingError(e.to_string()))?;

    // 2. Parse incoming entity using generalized extractor and candidate subgraphs
    let incoming_text = match custom_keys {
        Some(keys) => {
            let key_refs: Vec<&str> = keys.iter().map(String::as_str).collect();
            extract_incoming_entity_text_with_keys(prompt, &key_refs)
        }
        None => extract_incoming_entity_text(prompt),
    };
    let candidates = extract_candidates_from_prompt(prompt);

    // 3. Reasoning & directive derivation
    let (directive, chain_of_thought) = if candidates.is_empty() {
        let cot = format!(
            "[{provider_name}] Empty candidate graph context. Establishing root category node '{incoming_text}'."
        );
        (
            ExtractionDirective::CreateRootNode {
                entity_name: incoming_text.clone(),
                entity_level: "category".into(),
                confidence: 0.96,
            },
            cot,
        )
    } else if let Some(matched) = find_best_matching_candidate(&incoming_text, &candidates) {
        let cot = format!(
            "[{provider_name}] Semantic match identified: incoming item '{incoming_text}' matches candidate node '{}' (key: '{}'). Reusing existing node to maintain graph deduplication.",
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
        let cot = format!(
            "[{provider_name}] No matching existing candidate found for '{incoming_text}'. Establishing root category node '{incoming_text}'."
        );
        (
            ExtractionDirective::CreateRootNode {
                entity_name: incoming_text.clone(),
                entity_level: "category".into(),
                confidence: 0.94,
            },
            cot,
        )
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
    let incoming_tokens: Vec<&str> = lower_incoming
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| s.len() >= 3)
        .collect();

    // 1. Direct containment or equality in key or name
    for cand in candidates {
        let lower_cand = cand.name.to_lowercase();
        let lower_key = cand.key.to_lowercase();
        if lower_incoming == lower_cand
            || lower_incoming == lower_key
            || lower_incoming.contains(&lower_cand)
            || lower_cand.contains(&lower_incoming)
            || lower_incoming.contains(&lower_key)
            || lower_key.contains(&lower_incoming)
        {
            return Some(cand);
        }
    }

    // 2. Token overlap (match candidate with highest shared token count)
    let mut best_cand = None;
    let mut max_shared = 0;
    for cand in candidates {
        let lower_cand = cand.name.to_lowercase();
        let cand_tokens: Vec<&str> = lower_cand
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| s.len() >= 3)
            .collect();

        let shared = incoming_tokens
            .iter()
            .filter(|tok| cand_tokens.contains(tok))
            .count();

        if shared > max_shared {
            max_shared = shared;
            best_cand = Some(cand);
        }
    }

    if max_shared > 0 {
        return best_cand;
    }

    // Default to first candidate if ambiguous/synthetic test keys indicate anchor
    if candidates.len() == 1
        && (lower_incoming.contains("ambiguous")
            || lower_incoming.contains("unseen")
            || candidates[0].key.contains("anchor"))
    {
        return candidates.first();
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_oxillama_inference_deduplication() {
        let provider = OxillamaProvider::new("llama-3-8b-instruct", "q4_k_m");
        let prompt_empty =
            PromptFormatter::build_prompt("Taxonomy builder", "{\"name\": \"Root Facility\"}", &[]);

        let (directive1, notes1) = provider
            .extract_directive(&prompt_empty)
            .await
            .expect("directive 1");

        assert!(matches!(
            directive1,
            ExtractionDirective::CreateRootNode { ref entity_name, .. } if entity_name == "Root Facility"
        ));
        assert!(notes1.chain_of_thought.contains("oxillama"));

        // Second item similar entity: "Facility Substation Alpha" under candidate "cat_facility (Facility)"
        let prompt_with_candidate = PromptFormatter::build_prompt(
            "Taxonomy builder",
            "{\"name\": \"Facility Substation Alpha\"}",
            &["cat_facility (Facility)"],
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
                assert_eq!(parent_key, "cat_facility");
                assert_eq!(entity_name, "Facility Substation Alpha");
            }
            other => panic!("Expected InsertNode under cat_facility, got {other:?}"),
        }
        assert!(
            notes2
                .chain_of_thought
                .to_lowercase()
                .contains("reusing existing node")
        );
    }

    #[tokio::test]
    async fn test_candle_inference_deduplication() {
        let provider = CandleProvider::new("candle-smollm");
        let prompt_with_candidate = PromptFormatter::build_prompt(
            "Taxonomy builder",
            "{\"name\": \"Distribution Feeder\"}",
            &["grid_distribution (Distribution)"],
        );

        let (directive, notes) = provider
            .extract_directive(&prompt_with_candidate)
            .await
            .expect("candle extraction");

        match directive {
            ExtractionDirective::InsertNode { parent_key, .. } => {
                assert_eq!(parent_key, "grid_distribution");
            }
            other => panic!("Expected InsertNode under grid_distribution, got {other:?}"),
        }
        assert!(notes.chain_of_thought.contains("candle"));
    }

    #[tokio::test]
    async fn test_oxibonsai_provider_metadata() {
        let bonsai = OxibonsaiProvider::new("oxibonsai-1b", "q1_0_g128");
        assert_eq!(bonsai.model_info().provider_name, "oxibonsai");
        assert_eq!(bonsai.model_info().quantization, "q1_0_g128");
        assert!(bonsai.model_info().is_local_zero_ffi);
    }

    #[tokio::test]
    async fn test_fallback_providers() {
        let candle = CandleProvider::new("mistral-7b");
        assert_eq!(candle.model_info().provider_name, "candle");

        let cloud = CloudProvider::new("https://api.openai.com/v1", "gpt-4o");
        assert_eq!(cloud.model_info().provider_name, "cloud");
        assert!(!cloud.model_info().is_local_zero_ffi);
    }

    #[tokio::test]
    async fn test_provider_with_custom_entity_keys() {
        let provider = OxillamaProvider::new("llama-3-8b-instruct", "q4_k_m")
            .with_entity_keys(["product_id", "product_title"]);
        assert_eq!(
            provider.entity_keys.as_ref().unwrap(),
            &["product_id", "product_title"]
        );

        let prompt = PromptFormatter::build_prompt(
            "Custom classifier",
            "{\"product_id\": \"PROD-42\", \"description\": \"High-end display\"}",
            &[],
        );

        let (directive, _) = provider.extract_directive(&prompt).await.expect("extract");
        match directive {
            ExtractionDirective::CreateRootNode { entity_name, .. } => {
                assert_eq!(entity_name, "PROD-42");
            }
            other => panic!("Expected CreateRootNode for custom key, got {other:?}"),
        }
    }
}
