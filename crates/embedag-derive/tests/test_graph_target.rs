use embedag_core::model::GraphTarget;
use embedag_derive::GraphTarget;

/// Declared in another order than `GraphTarget` itself: the derive matches variants by their
/// shape, not by their position.
#[derive(Debug, PartialEq, GraphTarget)]
enum CustomPlacement {
    Root,
    Ambiguous(Vec<(String, f32)>),
    MultiChild {
        parent_keys: Vec<(String, String)>,
    },
    ChildOf {
        parent_key: String,
        relationship: String,
    },
    Fuzzy {
        key: String,
        similarity: f32,
    },
    Exact(String),
}

#[test]
fn test_derive_graph_target_enum_conversion() {
    // ExactMatch
    let domain_exact = CustomPlacement::Exact("CAT-100".to_string());
    let core_target: GraphTarget<String> = domain_exact.into_graph_target();
    assert_eq!(core_target, GraphTarget::ExactMatch("CAT-100".to_string()));
    let back: CustomPlacement = CustomPlacement::from_graph_target(core_target).unwrap();
    assert_eq!(back, CustomPlacement::Exact("CAT-100".to_string()));

    // FuzzyCandidate
    let domain_fuzzy = CustomPlacement::Fuzzy {
        key: "CAT-200".to_string(),
        similarity: 0.88,
    };
    let core_fuzzy: GraphTarget<String> = domain_fuzzy.into_graph_target();
    assert_eq!(
        core_fuzzy,
        GraphTarget::FuzzyCandidate {
            key: "CAT-200".to_string(),
            similarity: 0.88
        }
    );
    let back_fuzzy: CustomPlacement = CustomPlacement::from_graph_target(core_fuzzy).unwrap();
    assert_eq!(
        back_fuzzy,
        CustomPlacement::Fuzzy {
            key: "CAT-200".to_string(),
            similarity: 0.88
        }
    );

    // Root
    let domain_root = CustomPlacement::Root;
    let core_root: GraphTarget<String> = domain_root.into_graph_target();
    assert_eq!(core_root, GraphTarget::NewRoot);
    let back_root: CustomPlacement = CustomPlacement::from_graph_target(core_root).unwrap();
    assert_eq!(back_root, CustomPlacement::Root);

    // ChildOf
    let domain_child = CustomPlacement::ChildOf {
        parent_key: "P-1".to_string(),
        relationship: "is_a".to_string(),
    };
    let core_child: GraphTarget<String> = domain_child.into_graph_target();
    assert_eq!(
        core_child,
        GraphTarget::NewChildOf {
            parent_key: "P-1".to_string(),
            relationship: "is_a".to_string()
        }
    );
    let back_child: CustomPlacement = CustomPlacement::from_graph_target(core_child).unwrap();
    assert_eq!(
        back_child,
        CustomPlacement::ChildOf {
            parent_key: "P-1".to_string(),
            relationship: "is_a".to_string()
        }
    );
}
