use std::collections::HashSet;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum IndexError {
    #[error("Dimension mismatch: expected {expected}, got {got}")]
    DimensionMismatch { expected: usize, got: usize },
    #[error("Zero vector cannot be normalized")]
    ZeroVector,
}

/// In-memory dense vector index utilizing cosine similarity.
#[derive(Debug, Default, Clone)]
pub struct DenseVectorIndex {
    dimensions: Option<usize>,
    vectors: Vec<(Uuid, Vec<f32>)>,
}

impl DenseVectorIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_dimensions(dim: usize) -> Self {
        Self {
            dimensions: Some(dim),
            vectors: Vec::new(),
        }
    }

    /// Inserts or updates an embedding vector for a given node UUID.
    pub fn insert(&mut self, id: Uuid, embedding: Vec<f32>) -> Result<(), IndexError> {
        if let Some(dim) = self.dimensions {
            if embedding.len() != dim {
                return Err(IndexError::DimensionMismatch {
                    expected: dim,
                    got: embedding.len(),
                });
            }
        } else {
            self.dimensions = Some(embedding.len());
        }

        if let Some(pos) = self
            .vectors
            .iter()
            .position(|(existing_id, _)| *existing_id == id)
        {
            self.vectors[pos] = (id, embedding);
        } else {
            self.vectors.push((id, embedding));
        }

        Ok(())
    }

    /// Queries top-K nearest neighbors using cosine similarity.
    pub fn search(&self, query_vec: &[f32], top_k: usize) -> Result<Vec<(Uuid, f32)>, IndexError> {
        if let Some(dim) = self.dimensions
            && query_vec.len() != dim
        {
            return Err(IndexError::DimensionMismatch {
                expected: dim,
                got: query_vec.len(),
            });
        }

        let query_norm = norm(query_vec);
        if query_norm < 1e-9 {
            return Err(IndexError::ZeroVector);
        }

        let mut scored: Vec<(Uuid, f32)> = self
            .vectors
            .iter()
            .filter_map(|(id, vec)| {
                let v_norm = norm(vec);
                if v_norm < 1e-9 {
                    None
                } else {
                    let dot_product: f32 =
                        query_vec.iter().zip(vec.iter()).map(|(a, b)| a * b).sum();
                    let cosine = dot_product / (query_norm * v_norm);
                    Some((*id, cosine))
                }
            })
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(top_k);

        Ok(scored)
    }
}

fn norm(v: &[f32]) -> f32 {
    v.iter().map(|x| x * x).sum::<f32>().sqrt()
}

/// Sparse lexical index combining token matching and character trigrams for typo resilience.
#[derive(Debug, Default, Clone)]
pub struct SparseLexicalIndex {
    documents: Vec<LexicalDocument>,
}

#[derive(Debug, Clone)]
pub struct LexicalDocument {
    pub id: Uuid,
    pub business_key: String,
    pub raw_text: String,
    tokens: HashSet<String>,
    trigrams: HashSet<String>,
}

impl SparseLexicalIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Indexes a document text and business key.
    pub fn insert(
        &mut self,
        id: Uuid,
        business_key: impl Into<String>,
        raw_text: impl Into<String>,
    ) {
        let business_key = business_key.into();
        let raw_text = raw_text.into();

        let full_text = format!("{} {}", business_key, raw_text);
        let tokens: HashSet<String> = tokenize(&full_text);
        let trigrams: HashSet<String> = compute_trigrams(&full_text);

        if let Some(pos) = self.documents.iter().position(|d| d.id == id) {
            self.documents[pos] = LexicalDocument {
                id,
                business_key,
                raw_text,
                tokens,
                trigrams,
            };
        } else {
            self.documents.push(LexicalDocument {
                id,
                business_key,
                raw_text,
                tokens,
                trigrams,
            });
        }
    }

    /// Searches documents using hybrid BM25-like token overlap and trigram fuzzy matching.
    pub fn search(&self, query: &str, top_k: usize) -> Vec<(Uuid, f32)> {
        let query_tokens = tokenize(query);
        let query_trigrams = compute_trigrams(query);

        if query_tokens.is_empty() && query_trigrams.is_empty() {
            return Vec::new();
        }

        let mut scored: Vec<(Uuid, f32)> = self
            .documents
            .iter()
            .filter_map(|doc| {
                let score = doc.score(&query_tokens, &query_trigrams);
                (score > 0.01).then_some((doc.id, score))
            })
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(top_k);

        scored
    }
}

impl LexicalDocument {
    /// Share of the query tokens the document contains (exact SKUs, codes, words).
    fn token_overlap(&self, query_tokens: &HashSet<String>) -> f32 {
        if query_tokens.is_empty() {
            return 0.0;
        }
        self.tokens.intersection(query_tokens).count() as f32 / query_tokens.len() as f32
    }

    /// Trigram Jaccard similarity (handles typos, e.g. "Carols V" -> "Carlos V").
    fn trigram_similarity(&self, query_trigrams: &HashSet<String>) -> f32 {
        if query_trigrams.is_empty() || self.trigrams.is_empty() {
            return 0.0;
        }
        let intersection = self.trigrams.intersection(query_trigrams).count();
        // Neither side is empty here, so the union is not either.
        let union = self.trigrams.union(query_trigrams).count();
        intersection as f32 / union as f32
    }

    /// Weighted towards the exact token match when there is one, else the trigram similarity.
    fn score(&self, query_tokens: &HashSet<String>, query_trigrams: &HashSet<String>) -> f32 {
        let token_overlap = self.token_overlap(query_tokens);
        let trigram_sim = self.trigram_similarity(query_trigrams);
        if token_overlap > 0.0 {
            0.6 * token_overlap + 0.4 * trigram_sim
        } else {
            trigram_sim
        }
    }
}

fn tokenize(text: &str) -> HashSet<String> {
    text.split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_lowercase())
        .collect()
}

/// Character trigrams of every word, each word padded with `^` and `$` (so it has at least three).
fn compute_trigrams(text: &str) -> HashSet<String> {
    text.to_lowercase()
        .split_whitespace()
        .flat_map(|word| {
            let padded: Vec<char> = format!("^{word}$").chars().collect();
            padded
                .windows(3)
                .map(|window| window.iter().collect::<String>())
                .collect::<Vec<_>>()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_1_SQRT_2;

    #[test]
    fn test_dense_vector_search() {
        let mut index = DenseVectorIndex::with_dimensions(3);
        let id_a = Uuid::new_v4();
        let id_b = Uuid::new_v4();
        let id_c = Uuid::new_v4();

        index.insert(id_a, vec![1.0, 0.0, 0.0]).unwrap();
        index.insert(id_b, vec![0.0, 1.0, 0.0]).unwrap();
        index
            .insert(id_c, vec![FRAC_1_SQRT_2, FRAC_1_SQRT_2, 0.0])
            .unwrap();

        let query = vec![1.0, 0.0, 0.0];
        let results = index.search(&query, 2).unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, id_a);
        assert!((results[0].1 - 1.0).abs() < 1e-4);
        assert_eq!(results[1].0, id_c);
        assert!((results[1].1 - FRAC_1_SQRT_2).abs() < 1e-3);
    }

    #[test]
    fn test_sparse_fts_typo_resilience() {
        let mut index = SparseLexicalIndex::new();
        let id_carlos = Uuid::new_v4();
        let id_snickers = Uuid::new_v4();
        let id_sku = Uuid::new_v4();

        index.insert(id_carlos, "carlos_v", "Carlos V Dark Chocolate 50g");
        index.insert(
            id_snickers,
            "snickers_bar",
            "Snickers Milk Chocolate Peanut Bar",
        );
        index.insert(id_sku, "sku-90210", "Generic Product SKU-90210");

        // 1. Typo query: "Carols V" should match "Carlos V" via trigrams
        let typo_results = index.search("Carols V", 5);
        assert!(!typo_results.is_empty(), "Typo query must return matches");
        assert_eq!(
            typo_results[0].0, id_carlos,
            "Typo 'Carols V' must rank Carlos V first"
        );

        // 2. Exact SKU query: "sku-90210" should rank id_sku first
        let sku_results = index.search("SKU-90210", 5);
        assert!(!sku_results.is_empty());
        assert_eq!(sku_results[0].0, id_sku, "Exact SKU must rank #1");
    }

    use rstest::rstest;

    #[test]
    fn dense_index_updates_in_place_and_rejects_wrong_dimensions() {
        let mut index = DenseVectorIndex::new();
        let id = Uuid::new_v4();
        index.insert(id, vec![1.0, 0.0]).unwrap();
        // The first insert fixed the dimensionality at two.
        assert!(matches!(
            index.insert(Uuid::new_v4(), vec![1.0]),
            Err(IndexError::DimensionMismatch {
                expected: 2,
                got: 1
            })
        ));
        // Re-inserting the same id replaces the vector instead of adding a second entry.
        index.insert(id, vec![0.0, 1.0]).unwrap();
        let hits = index.search(&[0.0, 1.0], 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert!((hits[0].1 - 1.0).abs() < 1e-6);
    }

    #[test]
    fn dense_search_validates_the_query_and_skips_zero_vectors() {
        let mut index = DenseVectorIndex::with_dimensions(2);
        index.insert(Uuid::new_v4(), vec![0.0, 0.0]).unwrap();
        index.insert(Uuid::new_v4(), vec![1.0, 0.0]).unwrap();
        index.insert(Uuid::new_v4(), vec![0.5, 0.5]).unwrap();

        assert!(matches!(
            index.search(&[1.0], 5),
            Err(IndexError::DimensionMismatch { .. })
        ));
        assert!(matches!(
            index.search(&[0.0, 0.0], 5),
            Err(IndexError::ZeroVector)
        ));
        // The stored zero vector never scores, and only the best `top_k` are kept.
        assert_eq!(index.search(&[1.0, 0.0], 10).unwrap().len(), 2);
        assert_eq!(index.search(&[1.0, 0.0], 1).unwrap().len(), 1);
    }

    #[rstest]
    #[case::empty_query("", 0)]
    #[case::no_overlap("zzzzzz", 0)]
    #[case::exact_token("espresso", 1)]
    #[case::typo("espreso", 1)]
    fn sparse_search_matches_tokens_and_typos(#[case] query: &str, #[case] hits: usize) {
        let mut index = SparseLexicalIndex::new();
        index.insert(Uuid::new_v4(), "SKU-1", "espresso beans");
        assert_eq!(index.search(query, 10).len(), hits);
    }

    #[test]
    fn sparse_documents_are_replaced_by_id_and_results_are_capped() {
        let mut index = SparseLexicalIndex::new();
        let id = Uuid::new_v4();
        index.insert(id, "A", "tea leaves");
        index.insert(id, "A", "coffee beans");
        assert!(index.search("tea", 5).is_empty());
        assert_eq!(index.search("coffee", 5).len(), 1);

        index.insert(Uuid::new_v4(), "B", "coffee grounds");
        index.insert(Uuid::new_v4(), "C", "coffee filters");
        assert_eq!(index.search("coffee", 2).len(), 2);
    }

    #[test]
    fn trigrams_pad_each_word() {
        let trigrams = compute_trigrams("Ab c");
        for expected in ["^ab", "ab$", "^c$"] {
            assert!(trigrams.contains(expected), "{expected} missing");
        }
        assert!(compute_trigrams("").is_empty());
    }

    #[test]
    fn queries_without_word_characters_and_empty_documents_score_nothing() {
        let mut index = SparseLexicalIndex::new();
        index.insert(Uuid::new_v4(), "", "");
        index.insert(Uuid::new_v4(), "K", "plain words");
        // No tokens in the query, only trigrams of the punctuation: nothing overlaps.
        assert!(index.search("!!", 5).is_empty());
        // The empty document has no trigrams, so only the other one can match.
        assert_eq!(index.search("plain", 5).len(), 1);
    }
}
