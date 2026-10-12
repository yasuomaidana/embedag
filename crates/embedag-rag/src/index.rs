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
        if scored.len() > top_k {
            scored.truncate(top_k);
        }

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
                // 1. Exact token overlap (handles exact SKUs, codes, words)
                let token_overlap = if !query_tokens.is_empty() {
                    let common = doc.tokens.intersection(&query_tokens).count();
                    common as f32 / query_tokens.len() as f32
                } else {
                    0.0
                };

                // 2. Trigram Jaccard similarity (handles typos e.g., "Carols V" -> "Carlos V")
                let trigram_sim = if !query_trigrams.is_empty() && !doc.trigrams.is_empty() {
                    let intersection = doc.trigrams.intersection(&query_trigrams).count();
                    let union = doc.trigrams.union(&query_trigrams).count();
                    if union > 0 {
                        intersection as f32 / union as f32
                    } else {
                        0.0
                    }
                } else {
                    0.0
                };

                // Composite score weighted towards exact token match if present, else trigram
                let score = if token_overlap > 0.0 {
                    0.6 * token_overlap + 0.4 * trigram_sim
                } else {
                    trigram_sim
                };

                if score > 0.01 {
                    Some((doc.id, score))
                } else {
                    None
                }
            })
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        if scored.len() > top_k {
            scored.truncate(top_k);
        }

        scored
    }
}

fn tokenize(text: &str) -> HashSet<String> {
    text.split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_lowercase())
        .collect()
}

fn compute_trigrams(text: &str) -> HashSet<String> {
    let lower = text.to_lowercase();
    let mut trigrams = HashSet::new();

    for word in lower.split_whitespace() {
        if word.is_empty() {
            continue;
        }
        let padded = format!("^{}$", word);
        let chars: Vec<char> = padded.chars().collect();
        if chars.len() < 3 {
            trigrams.insert(padded);
        } else {
            for i in 0..=chars.len() - 3 {
                let trigram: String = chars[i..i + 3].iter().collect();
                trigrams.insert(trigram);
            }
        }
    }

    trigrams
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
}
