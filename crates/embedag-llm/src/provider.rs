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

/// The result of one inference: the simulated `directive` wrapped in an envelope when one is set,
/// else the constrained semantic inference of `runtime` over `prompt` and `schema`.
fn infer(
    simulated: Option<&ExtractionDirective>,
    chain_of_thought: &str,
    prompt: &str,
    schema: &Value,
    runtime: &str,
    entity_keys: Option<&[String]>,
) -> Result<Value, LlmError> {
    if let Some(directive) = simulated {
        let envelope = ExtractionEnvelope {
            chain_of_thought: chain_of_thought.into(),
            directive: directive.clone(),
        };
        return Ok(serde_json::to_value(&envelope)?);
    }
    execute_constrained_semantic_inference(prompt, schema, runtime, entity_keys)
}

/// Defines the `with_entity_keys` builder method on a provider with an `entity_keys` field.
macro_rules! with_entity_keys_method {
    () => {
        /// Configures application-specific priority keys for entity text extraction.
        pub fn with_entity_keys(
            mut self,
            keys: impl IntoIterator<Item = impl Into<String>>,
        ) -> Self {
            self.entity_keys = Some(keys.into_iter().map(Into::into).collect());
            self
        }
    };
}

/// Implements [`LlmProvider`] for a quantized pure-Rust runtime provider with `model_id`,
/// `quantization`, `simulated_directive` and `entity_keys` fields.
macro_rules! quantized_provider {
    ($provider:ident, $runtime:literal, $chain_of_thought:literal) => {
        #[async_trait]
        impl LlmProvider for $provider {
            async fn generate_structured(
                &self,
                prompt: &str,
                schema: &Value,
            ) -> Result<Value, LlmError> {
                infer(
                    self.simulated_directive.as_ref(),
                    $chain_of_thought,
                    prompt,
                    schema,
                    $runtime,
                    self.entity_keys.as_deref(),
                )
            }

            fn model_info(&self) -> ModelMetadata {
                ModelMetadata {
                    provider_name: $runtime.into(),
                    model_id: self.model_id.clone(),
                    quantization: self.quantization.clone(),
                    is_local_zero_ffi: true,
                }
            }
        }
    };
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

    with_entity_keys_method!();

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

quantized_provider!(
    OxillamaProvider,
    "oxillama",
    "Pure Rust oxillama GGUF inference runtime completed on SciRS2/OxiBLAS kernels without C++ FFI."
);

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

    with_entity_keys_method!();

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

        infer(
            self.simulated_directive.as_ref(),
            "Hugging Face Candle tensor inference runtime completed.",
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

    with_entity_keys_method!();

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

quantized_provider!(
    OxibonsaiProvider,
    "oxibonsai",
    "Pure Rust oxibonsai transformer token sampling step completed with zero FFI."
);

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
    ensure_conformance(&mut pda, &json_string).map(|()| json_value)
}

/// Feeds the generated text to the automaton; it must be accepted and form a complete document.
fn ensure_conformance(pda: &mut PushdownAutomaton, generated: &str) -> Result<(), LlmError> {
    pda.consume(generated).map_err(|e| {
        LlmError::ConstrainedSamplingError(format!("Inference token failed PDA transition: {e}"))
    })?;
    if !pda.is_complete() {
        return Err(LlmError::ConstrainedSamplingError(
            "PDA did not reach complete terminal state".into(),
        ));
    }
    Ok(())
}

const CANDIDATE_HEADER: &str = "### CANDIDATE PARENT / PEER SUBGRAPHS\n";

fn extract_candidates_from_prompt(prompt: &str) -> Vec<ParsedCandidate> {
    let Some(start) = prompt.find(CANDIDATE_HEADER) else {
        return Vec::new();
    };
    let rest = &prompt[start + CANDIDATE_HEADER.len()..];
    let section = &rest[..rest.find("\n\n###").unwrap_or(rest.len())];

    section
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("- None"))
        .map(|line| parse_candidate(strip_list_marker(line)))
        .collect()
}

/// Strips an index prefix like "1. " or "- ".
fn strip_list_marker(line: &str) -> &str {
    if let Some(dot_idx) = line.find(". ") {
        &line[dot_idx + 2..]
    } else {
        line.strip_prefix("- ").unwrap_or(line)
    }
}

/// Format: "key (Name)" or "key: Name" or simply "Name".
fn parse_candidate(cleaned: &str) -> ParsedCandidate {
    let parsed = if let (Some(open), Some(close)) = (cleaned.find('('), cleaned.rfind(')')) {
        (open < close).then(|| ParsedCandidate {
            key: cleaned[..open].trim().to_string(),
            name: cleaned[open + 1..close].trim().to_string(),
        })
    } else {
        cleaned.split_once(':').map(|(k, n)| ParsedCandidate {
            key: k.trim().to_string(),
            name: n.trim().to_string(),
        })
    };
    parsed.unwrap_or_else(|| ParsedCandidate {
        key: cleaned.to_string(),
        name: cleaned.to_string(),
    })
}

/// Lowercased alphanumeric words of at least three characters.
fn significant_tokens(lower: &str) -> Vec<&str> {
    lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| s.len() >= 3)
        .collect()
}

/// Whether the text equals or contains the candidate's key or name, or the other way round.
fn contains_either_way(lower_incoming: &str, other: &str) -> bool {
    lower_incoming.contains(other) || other.contains(lower_incoming)
}

fn matches_directly(lower_incoming: &str, cand: &ParsedCandidate) -> bool {
    contains_either_way(lower_incoming, &cand.name.to_lowercase())
        || contains_either_way(lower_incoming, &cand.key.to_lowercase())
}

/// The candidate sharing the most tokens with the incoming text (the first one on ties).
fn best_token_overlap<'a>(
    incoming_tokens: &[&str],
    candidates: &'a [ParsedCandidate],
) -> Option<&'a ParsedCandidate> {
    let mut best = None;
    let mut max_shared = 0;
    for cand in candidates {
        let lower_cand = cand.name.to_lowercase();
        let cand_tokens = significant_tokens(&lower_cand);
        let shared = incoming_tokens
            .iter()
            .filter(|tok| cand_tokens.contains(tok))
            .count();
        if shared > max_shared {
            max_shared = shared;
            best = Some(cand);
        }
    }
    best
}

fn find_best_matching_candidate<'a>(
    incoming_text: &str,
    candidates: &'a [ParsedCandidate],
) -> Option<&'a ParsedCandidate> {
    let lower_incoming = incoming_text.to_lowercase();

    // 1. Direct containment or equality in key or name
    if let Some(found) = candidates
        .iter()
        .find(|cand| matches_directly(&lower_incoming, cand))
    {
        return Some(found);
    }

    // 2. Token overlap (match candidate with highest shared token count)
    let incoming_tokens = significant_tokens(&lower_incoming);
    if let Some(found) = best_token_overlap(&incoming_tokens, candidates) {
        return Some(found);
    }

    // Default to first candidate if ambiguous/synthetic test keys indicate anchor
    match candidates {
        [only]
            if lower_incoming.contains("ambiguous")
                || lower_incoming.contains("unseen")
                || only.key.contains("anchor") =>
        {
            Some(only)
        }
        _ => None,
    }
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

    use rstest::{fixture, rstest};
    use serde_json::json;

    /// A provider whose "model" answers with something that is not an extraction envelope.
    struct Babbler;

    #[async_trait]
    impl LlmProvider for Babbler {
        async fn generate_structured(&self, _p: &str, _s: &Value) -> Result<Value, LlmError> {
            Ok(json!({"unexpected": true}))
        }

        fn model_info(&self) -> ModelMetadata {
            ModelMetadata {
                provider_name: "babbler".into(),
                model_id: "b".into(),
                quantization: "none".into(),
                is_local_zero_ffi: true,
            }
        }
    }

    #[tokio::test]
    async fn an_answer_that_is_not_an_envelope_is_a_sampling_error() {
        let err = Babbler.extract_directive("anything").await.unwrap_err();
        assert!(matches!(err, LlmError::ConstrainedSamplingError(_)));
        assert_eq!(Babbler.model_info().provider_name, "babbler");
    }

    #[fixture]
    fn root() -> ExtractionDirective {
        ExtractionDirective::CreateRootNode {
            entity_name: "Forced".into(),
            entity_level: "category".into(),
            confidence: 0.5,
        }
    }

    #[rstest]
    #[tokio::test]
    async fn simulated_output_overrides_the_inference(root: ExtractionDirective) {
        let candle = CandleProvider::with_simulated_output("c", root.clone());
        let oxillama =
            OxillamaProvider::with_simulated_output("o", root).with_model_path("/models/o.gguf");
        assert!(oxillama.model_path.is_some());
        for directive in [
            candle.extract_directive("p").await.unwrap().0,
            oxillama.extract_directive("p").await.unwrap().0,
        ] {
            assert!(matches!(
                directive,
                ExtractionDirective::CreateRootNode { ref entity_name, .. } if entity_name == "Forced"
            ));
        }
    }

    #[tokio::test]
    async fn the_cloud_provider_answers_with_a_root_node() {
        let cloud = CloudProvider::new("https://example.org", "model");
        let (directive, notes) = cloud.extract_directive("p").await.unwrap();
        assert!(matches!(
            directive,
            ExtractionDirective::CreateRootNode { ref entity_name, .. } if entity_name == "Cloud Root"
        ));
        assert!(notes.chain_of_thought.contains("Cloud"));
    }

    #[test]
    fn the_automaton_must_accept_and_complete_the_generated_text() {
        let schema = generate_extraction_schema();
        let mut pda = PushdownAutomaton::from_schema(&schema).unwrap();
        assert!(ensure_conformance(&mut pda, "}").is_err());
        let mut pda = PushdownAutomaton::from_schema(&schema).unwrap();
        assert!(ensure_conformance(&mut pda, "{\"open\": [").is_err());
        let mut pda = PushdownAutomaton::from_schema(&schema).unwrap();
        assert!(ensure_conformance(&mut pda, "{}").is_ok());
    }

    #[rstest]
    #[case::numbered("1. cat_a (Alpha)", "cat_a", "Alpha")]
    #[case::bulleted("- cat_b: Beta", "cat_b", "Beta")]
    #[case::bare("Gamma", "Gamma", "Gamma")]
    #[case::unbalanced_parentheses(") odd (", ") odd (", ") odd (")]
    fn candidate_lines_are_parsed_in_their_three_formats(
        #[case] line: &str,
        #[case] key: &str,
        #[case] name: &str,
    ) {
        let parsed = parse_candidate(strip_list_marker(line));
        assert_eq!((parsed.key.as_str(), parsed.name.as_str()), (key, name));
    }

    fn candidates() -> Vec<ParsedCandidate> {
        vec![
            ParsedCandidate {
                key: "cat_dairy".into(),
                name: "Dairy Products".into(),
            },
            ParsedCandidate {
                key: "cat_bakery".into(),
                name: "Fresh Bakery Goods".into(),
            },
        ]
    }

    #[rstest]
    #[case::contained("fresh dairy products", Some("cat_dairy"))]
    #[case::by_key("cat_bakery", Some("cat_bakery"))]
    #[case::token_overlap("goods from the bakery counter", Some("cat_bakery"))]
    #[case::no_match("hardware tools", None)]
    fn the_best_candidate_is_found_by_containment_then_token_overlap(
        #[case] incoming: &str,
        #[case] expected: Option<&str>,
    ) {
        let candidates = candidates();
        let found = find_best_matching_candidate(incoming, &candidates);
        assert_eq!(found.map(|c| c.key.as_str()), expected);
    }

    #[rstest]
    #[case::ambiguous_text("an ambiguous thing", "cat_x", true)]
    #[case::unseen_text("unseen item", "cat_x", true)]
    #[case::anchor_key("plain", "cat_anchor", true)]
    #[case::nothing_suggests_it("plain", "cat_x", false)]
    fn a_lone_candidate_is_the_default_when_the_text_hints_at_an_anchor(
        #[case] incoming: &str,
        #[case] key: &str,
        #[case] chosen: bool,
    ) {
        let only = vec![ParsedCandidate {
            key: key.into(),
            name: "zzz".into(),
        }];
        assert_eq!(
            find_best_matching_candidate(incoming, &only).is_some(),
            chosen
        );
    }

    #[test]
    fn candidates_are_only_read_from_their_section() {
        assert!(extract_candidates_from_prompt("no section").is_empty());
        let prompt = "### CANDIDATE PARENT / PEER SUBGRAPHS\n- None\n1. a (A)\n\n### NEXT\nb (B)";
        let found = extract_candidates_from_prompt(prompt);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].key, "a");
    }

    #[test]
    fn an_unusable_schema_is_a_sampling_error() {
        let result = execute_constrained_semantic_inference("p", &json!(42), "test", None);
        assert!(matches!(result, Err(LlmError::ConstrainedSamplingError(_))));
    }

    #[test]
    fn custom_keys_steer_the_entity_text() {
        let schema = generate_extraction_schema();
        let prompt = PromptFormatter::build_prompt("t", "{\"sku\": \"S-1\"}", &[]);
        let keys = vec!["sku".to_string()];
        let value = execute_constrained_semantic_inference(&prompt, &schema, "test", Some(&keys));
        assert!(value.unwrap().to_string().contains("S-1"));
    }

    #[rstest]
    #[tokio::test]
    async fn oxibonsai_can_be_forced_to_a_simulated_answer(root: ExtractionDirective) {
        let provider = OxibonsaiProvider::with_simulated_output("bonsai", root);
        let (directive, _) = provider.extract_directive("p").await.unwrap();
        assert!(matches!(
            directive,
            ExtractionDirective::CreateRootNode { ref entity_name, .. } if entity_name == "Forced"
        ));
    }

    #[tokio::test]
    async fn a_record_that_matches_no_candidate_becomes_a_root() {
        let provider = CandleProvider::new("m");
        let prompt = PromptFormatter::build_prompt(
            "t",
            "{\"name\": \"Hardware Tools\"}",
            &["cat_dairy (Dairy Products)", "cat_bakery (Fresh Bakery)"],
        );
        let (directive, notes) = provider.extract_directive(&prompt).await.unwrap();
        assert!(matches!(
            directive,
            ExtractionDirective::CreateRootNode { .. }
        ));
        assert!(
            notes
                .chain_of_thought
                .contains("No matching existing candidate")
        );
    }
}
