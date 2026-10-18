//! `embedag-matcher`: Structural AST similarity matching and similarity graphs.
//!
//! Provides language-agnostic syntax [`Tree`] representations, subtree structural hashing
//! with [`AstMatcher`], and undirected [`SimilarityGraph`] clustering.

#![warn(missing_docs)]

pub mod ast;
pub mod graph;
pub mod tree;

pub use ast::{AstMatcher, Matcher};
pub use graph::SimilarityGraph;
pub use tree::Tree;

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_add_fn(fn_name: &str, lhs: &str, rhs: &str) -> Tree {
        Tree::new(
            "function_item",
            Some(fn_name.to_string()),
            vec![
                Tree::branch(
                    "parameters",
                    vec![
                        Tree::leaf("parameter", lhs),
                        Tree::leaf("parameter", rhs),
                    ],
                ),
                Tree::branch(
                    "block",
                    vec![Tree::branch(
                        "binary_expression",
                        vec![
                            Tree::leaf("identifier", lhs),
                            Tree::leaf("operator", "+"),
                            Tree::leaf("identifier", rhs),
                        ],
                    )],
                ),
            ],
        )
    }

    fn sample_unrelated_tree() -> Tree {
        Tree::branch(
            "struct_item",
            vec![
                Tree::branch(
                    "field_declaration_list",
                    vec![
                        Tree::branch(
                            "field_declaration",
                            vec![
                                Tree::leaf("field_identifier", "count"),
                                Tree::leaf("primitive_type", "u64"),
                            ],
                        ),
                        Tree::branch(
                            "field_declaration",
                            vec![
                                Tree::leaf("field_identifier", "active"),
                                Tree::leaf("primitive_type", "bool"),
                            ],
                        ),
                    ],
                ),
                Tree::branch(
                    "attribute_item",
                    vec![Tree::leaf("meta_item", "derive(Clone)")],
                ),
            ],
        )
    }

    #[test]
    fn test_identical_trees_score_one() {
        let t1 = sample_add_fn("add", "a", "b");
        let t2 = sample_add_fn("add", "a", "b");
        let matcher = AstMatcher::default();
        let s = matcher.score(&t1, &t2);
        assert!((s - 1.0).abs() < f32::EPSILON, "expected 1.0, got {s}");
    }

    #[test]
    fn test_label_only_rename_scores_one() {
        let t1 = sample_add_fn("compute_sum", "first_val", "second_val");
        let t2 = sample_add_fn("total", "x", "y");
        let matcher = AstMatcher::default();
        let s = matcher.score(&t1, &t2);
        assert!(
            (s - 1.0).abs() < f32::EPSILON,
            "renamed identifiers must still match 1.0, got {s}"
        );
    }

    #[test]
    fn test_unrelated_trees_score_low() {
        let func_tree = sample_add_fn("add", "a", "b");
        let struct_tree = sample_unrelated_tree();
        let matcher = AstMatcher::default();
        let s = matcher.score(&func_tree, &struct_tree);
        assert!(s < 0.3, "expected unrelated score < 0.3, got {s}");
    }

    #[test]
    fn test_score_is_symmetric() {
        let t1 = sample_add_fn("add", "a", "b");
        let mut t2 = sample_add_fn("add_extra", "a", "b");
        t2.children.push(Tree::branch(
            "return_statement",
            vec![
                Tree::leaf("keyword", "return"),
                Tree::leaf("literal", "0"),
            ],
        ));
        let t3 = sample_unrelated_tree();

        let matcher = AstMatcher::default();
        let s12 = matcher.score(&t1, &t2);
        let s21 = matcher.score(&t2, &t1);
        assert_eq!(s12.to_bits(), s21.to_bits());
        assert!(s12 > 0.3 && s12 < 1.0);

        let s23 = matcher.score(&t2, &t3);
        let s32 = matcher.score(&t3, &t2);
        assert_eq!(s23.to_bits(), s32.to_bits());
    }

    #[test]
    fn test_similarity_graph_threshold_neighbors_and_clusters() {
        let fn_a = sample_add_fn("add_a", "x", "y");
        let fn_b = sample_add_fn("add_b", "u", "v");
        let mut fn_c = sample_add_fn("add_c", "p", "q");
        fn_c.children.push(Tree::branch(
            "let_declaration",
            vec![
                Tree::leaf("identifier", "tmp"),
                Tree::leaf("integer_literal", "1"),
            ],
        ));
        let unrelated = sample_unrelated_tree();

        let items = vec![
            ("fn_b".to_string(), fn_b),
            ("fn_a".to_string(), fn_a),
            ("fn_c".to_string(), fn_c),
            ("struct_z".to_string(), unrelated),
        ];

        let matcher = AstMatcher::default();
        // fn_a vs fn_c: shared subtrees weigh 3 + 4 + 5 = 12 out of a weighted union of 36
        // (the extra `let` changes fn_c's root hash), so the score is 1/3.
        let sim_graph = SimilarityGraph::build(&items, &matcher, 0.3);

        let neighbors_a = sim_graph.neighbors("fn_a");
        assert_eq!(neighbors_a.len(), 2);
        assert_eq!(neighbors_a[0].0, "fn_b");
        assert!((neighbors_a[0].1 - 1.0).abs() < f32::EPSILON);
        assert_eq!(neighbors_a[1].0, "fn_c");
        assert!((neighbors_a[1].1 - 1.0 / 3.0).abs() < 1e-6);

        assert!(sim_graph.neighbors("struct_z").is_empty());
        assert!(sim_graph.neighbors("nonexistent").is_empty());

        let clusters = sim_graph.clusters();
        assert_eq!(
            clusters,
            vec![
                vec!["fn_a".to_string(), "fn_b".to_string(), "fn_c".to_string()],
                vec!["struct_z".to_string()],
            ]
        );
    }
}
