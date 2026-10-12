use crate::prompt::{PromptFormatter, ProvenanceNotes};
use crate::provider::{LlmError, LlmProvider};
use crate::schema::ExtractionDirective;
use std::collections::HashMap;
use std::sync::RwLock;

/// Resolution outcome produced by the multi-tiered waterfall pipeline.
#[derive(Debug, Clone, PartialEq)]
pub enum ResolutionOutcome {
    /// Tier 1: Deterministic O(1) hash match in alias cache or inverted index (100% confidence).
    Tier1DirectMatch { target_key: String, confidence: f32 },
    /// Tier 2: Composite hybrid distance threshold met (similarity >= 0.85).
    Tier2NearMatch { target_key: String, similarity: f32 },
    /// Tier 2 Novel Entity: Composite distance exceeds novelty threshold (similarity < 0.60).
    Tier2NovelEntity {
        suggested_parent_key: Option<String>,
    },
    /// Tier 3: Resolved by constrained LLM inference (confidence >= 0.80).
    Tier3LlmResolved {
        directive: ExtractionDirective,
        notes: ProvenanceNotes,
    },
    /// Tier 4: Ambiguous or low-confidence LLM output routed to Human-in-the-Loop dead-letter review.
    Tier4HumanReview {
        reason: String,
        candidate_keys: Vec<String>,
    },
}

/// In-memory LRU and inverted index store mapping normalized tokens/barcodes/aliases to canonical node keys.
#[derive(Debug, Default)]
pub struct AliasStore {
    aliases: RwLock<HashMap<String, String>>,
}

impl AliasStore {
    /// Creates a new empty alias store.
    pub fn new() -> Self {
        Self {
            aliases: RwLock::new(HashMap::new()),
        }
    }

    /// Normalizes incoming strings: trims whitespace, lowercases, and removes punctuation.
    pub fn normalize_token(token: &str) -> String {
        token
            .trim()
            .to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric() || c.is_whitespace() || *c == '-' || *c == '_')
            .collect()
    }

    /// Registers a normalized alias pointing to a canonical node key.
    pub fn insert_alias(&self, alias: &str, target_key: &str) {
        let norm = Self::normalize_token(alias);
        let mut guard = self.aliases.write().expect("alias write lock");
        guard.insert(norm, target_key.to_string());
    }

    /// Looks up a token in the store. Resolves in O(1) time (< 1ms).
    pub fn lookup(&self, token: &str) -> Option<String> {
        let norm = Self::normalize_token(token);
        let guard = self.aliases.read().expect("alias read lock");
        guard.get(&norm).cloned()
    }

    /// Records human operator correction from Tier 4 review to prevent duplicate future escalations.
    pub fn record_human_correction(&self, alias: &str, canonical_key: &str) {
        self.insert_alias(alias, canonical_key);
    }

    /// Returns the count of registered aliases.
    pub fn len(&self) -> usize {
        self.aliases.read().expect("alias read lock").len()
    }

    /// Returns true if the alias store is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Candidate entity representation for Tier 2 evaluation.
#[derive(Debug, Clone)]
pub struct CandidateMatch {
    pub key: String,
    pub similarity_score: f32,
}

/// Orchestrator for the 4-tier waterfall entity resolution pipeline.
pub struct WaterfallPipeline<P: LlmProvider> {
    pub alias_store: AliasStore,
    pub llm_provider: P,
    pub tier2_near_threshold: f32,  // Default: 0.85
    pub tier2_novel_threshold: f32, // Default: 0.60
    pub tier3_confidence_gate: f32, // Default: 0.80
}

impl<P: LlmProvider> WaterfallPipeline<P> {
    /// Creates a new waterfall pipeline with standard enterprise thresholds.
    pub fn new(llm_provider: P) -> Self {
        Self {
            alias_store: AliasStore::new(),
            llm_provider,
            tier2_near_threshold: 0.85,
            tier2_novel_threshold: 0.60,
            tier3_confidence_gate: 0.80,
        }
    }

    /// Executes waterfall resolution through Tiers 1 to 4.
    pub async fn resolve(
        &self,
        record_token: &str,
        candidates: &[CandidateMatch],
    ) -> Result<ResolutionOutcome, LlmError> {
        // --- TIER 1: Exact Key & Alias Inverted Index Cache ---
        if let Some(target_key) = self.alias_store.lookup(record_token) {
            return Ok(ResolutionOutcome::Tier1DirectMatch {
                target_key,
                confidence: 1.0,
            });
        }

        // --- TIER 2: Composite Hybrid Distance Thresholding ---
        if let Some(best) = candidates.iter().max_by(|a, b| {
            a.similarity_score
                .partial_cmp(&b.similarity_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        }) {
            if best.similarity_score >= self.tier2_near_threshold {
                return Ok(ResolutionOutcome::Tier2NearMatch {
                    target_key: best.key.clone(),
                    similarity: best.similarity_score,
                });
            } else if best.similarity_score < self.tier2_novel_threshold {
                return Ok(ResolutionOutcome::Tier2NovelEntity {
                    suggested_parent_key: None,
                });
            }
        } else {
            // No candidates exist in graph -> Novel entity
            return Ok(ResolutionOutcome::Tier2NovelEntity {
                suggested_parent_key: None,
            });
        }

        // --- TIER 3: Sovereign Constrained LLM Inference ---
        let candidate_keys: Vec<String> = candidates.iter().map(|c| c.key.clone()).collect();
        let candidate_refs: Vec<&str> = candidate_keys.iter().map(|s| s.as_str()).collect();
        let prompt = PromptFormatter::build_prompt(
            "You are a taxonomy classifier.",
            record_token,
            &candidate_refs,
        );

        let (directive, notes) = self.llm_provider.extract_directive(&prompt).await?;

        let confidence = match &directive {
            ExtractionDirective::InsertNode { confidence, .. } => *confidence,
            ExtractionDirective::InsertMultiParentNode { confidence, .. } => *confidence,
            ExtractionDirective::UpdateAttributes { confidence, .. } => *confidence,
            ExtractionDirective::RetargetEdge { confidence, .. } => *confidence,
            ExtractionDirective::CreateRootNode { confidence, .. } => *confidence,
            ExtractionDirective::MarkAmbiguous { .. } => 0.0,
        };

        if confidence >= self.tier3_confidence_gate {
            Ok(ResolutionOutcome::Tier3LlmResolved { directive, notes })
        } else {
            // --- TIER 4: Human-in-the-Loop Dead Letter Review ---
            Ok(ResolutionOutcome::Tier4HumanReview {
                reason: format!(
                    "LLM confidence {confidence:.2} fell below gate {:.2}",
                    self.tier3_confidence_gate
                ),
                candidate_keys: candidates.iter().map(|c| c.key.clone()).collect(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::OxillamaProvider;

    #[tokio::test]
    async fn test_alias_resolution_fast_path() {
        let provider = OxillamaProvider::new("llama-3-8b", "q4_k_m");
        let pipeline = WaterfallPipeline::new(provider);

        // Prepopulate Tier 1 store
        pipeline
            .alias_store
            .insert_alias("SKU-12345", "prod_organic_milk");
        pipeline
            .alias_store
            .insert_alias("  sku 12345! ", "prod_organic_milk");

        // Lookup exact and normalized aliases
        let outcome = pipeline
            .resolve("SKU-12345", &[])
            .await
            .expect("resolution should succeed");

        assert_eq!(
            outcome,
            ResolutionOutcome::Tier1DirectMatch {
                target_key: "prod_organic_milk".into(),
                confidence: 1.0,
            }
        );

        let normalized_outcome = pipeline
            .resolve("sku-12345", &[])
            .await
            .expect("normalized lookup");

        assert_eq!(
            normalized_outcome,
            ResolutionOutcome::Tier1DirectMatch {
                target_key: "prod_organic_milk".into(),
                confidence: 1.0,
            }
        );
    }

    #[tokio::test]
    async fn test_hybrid_threshold_routing() {
        let provider = OxillamaProvider::new("llama-3-8b", "q4_k_m");
        let pipeline = WaterfallPipeline::new(provider);

        // Case 1: High similarity (>= 0.85) -> Tier 2 Near Match
        let high_candidates = vec![CandidateMatch {
            key: "anchor_node_1".into(),
            similarity_score: 0.92,
        }];
        let outcome_high = pipeline
            .resolve("unseen_item", &high_candidates)
            .await
            .expect("near match resolution");
        assert_eq!(
            outcome_high,
            ResolutionOutcome::Tier2NearMatch {
                target_key: "anchor_node_1".into(),
                similarity: 0.92,
            }
        );

        // Case 2: Low similarity (< 0.60) -> Tier 2 Novel Entity
        let low_candidates = vec![CandidateMatch {
            key: "anchor_node_1".into(),
            similarity_score: 0.45,
        }];
        let outcome_low = pipeline
            .resolve("unseen_item", &low_candidates)
            .await
            .expect("novel entity resolution");
        assert_eq!(
            outcome_low,
            ResolutionOutcome::Tier2NovelEntity {
                suggested_parent_key: None
            }
        );

        // Case 3: Ambiguous similarity (0.60 <= score < 0.85) -> Routes to Tier 3 (LLM)
        let amb_candidates = vec![CandidateMatch {
            key: "anchor_node_1".into(),
            similarity_score: 0.72,
        }];
        let outcome_tier3 = pipeline
            .resolve("ambiguous_item", &amb_candidates)
            .await
            .expect("tier 3 resolution");

        match outcome_tier3 {
            ResolutionOutcome::Tier3LlmResolved { directive, .. } => {
                assert!(matches!(directive, ExtractionDirective::InsertNode { .. }));
            }
            _ => panic!("Expected Tier3LlmResolved for ambiguous candidate"),
        }
    }
}
