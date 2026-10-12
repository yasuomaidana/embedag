use chrono::{DateTime, TimeZone, Utc};
use embedag_core::model::MultimodalBlob;
use embedag_core::traits::AsGraphContext;
use embedag_derive::AsGraphContext;

#[derive(AsGraphContext)]
struct CompleteProduct {
    #[graph(key)]
    sku: String,

    #[graph(text)]
    description: String,

    #[graph(scalar)]
    price_usd: f64,

    #[graph(scalar = "weight_grams")]
    weight: f64,

    #[graph(time)]
    scanned_at: DateTime<Utc>,

    #[graph(geo)]
    location: (f64, f64),

    #[graph(tag)]
    category: String,

    #[graph(tag = "brand_name")]
    brand: String,

    #[graph(blob)]
    image: MultimodalBlob,

    #[graph(tokens)]
    lexical: Vec<(u32, f32)>,
}

#[test]
fn test_derive_as_graph_context_codegen() {
    let dt = Utc
        .with_ymd_and_hms(2026, 4, 15, 10, 30, 0)
        .single()
        .unwrap();
    let blob = MultimodalBlob::new("image/jpeg", "s3://bucket/test.jpg");

    let product = CompleteProduct {
        sku: "SKU-999".to_string(),
        description: "Dark Chocolate Bar 50g".to_string(),
        price_usd: 1.50,
        weight: 50.0,
        scanned_at: dt,
        location: (2.3522, 48.8566),
        category: "Confectionery".to_string(),
        brand: "Carlos V".to_string(),
        image: blob.clone(),
        lexical: vec![(10, 0.8), (20, 0.5)],
    };

    assert_eq!(product.context_key(), "SKU-999");
    assert_eq!(product.text_repr(), "Dark Chocolate Bar 50g");

    let scalars = product.scalar_features();
    assert_eq!(scalars.get("price_usd"), Some(&1.50));
    assert_eq!(scalars.get("weight_grams"), Some(&50.0));

    assert_eq!(product.timestamp_utc(), Some(dt));
    assert_eq!(product.geographic_coordinates(), Some((2.3522, 48.8566)));

    let tags = product.categorical_tags();
    assert_eq!(tags.get("category"), Some(&"Confectionery".to_string()));
    assert_eq!(tags.get("brand_name"), Some(&"Carlos V".to_string()));

    let blobs = product.multimodal_blobs();
    assert_eq!(blobs.len(), 1);
    assert_eq!(blobs[0], blob);

    let tokens = product.sparse_lexical_tokens();
    assert_eq!(tokens, vec![(10, 0.8), (20, 0.5)]);
}

#[derive(AsGraphContext)]
struct MinimalRecord {
    #[graph(key)]
    id: i64,

    #[graph(text)]
    title: String,
}

#[test]
fn test_derive_as_graph_context_minimal() {
    let rec = MinimalRecord {
        id: 42,
        title: "Simple Title".to_string(),
    };

    assert_eq!(rec.context_key(), "42");
    assert_eq!(rec.text_repr(), "Simple Title");
    assert!(rec.scalar_features().is_empty());
    assert!(rec.timestamp_utc().is_none());
    assert!(rec.geographic_coordinates().is_none());
    assert!(rec.categorical_tags().is_empty());
    assert!(rec.multimodal_blobs().is_empty());
}
