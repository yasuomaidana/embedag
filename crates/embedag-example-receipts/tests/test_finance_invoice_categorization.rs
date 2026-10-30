use embedag_diesel::cte::HierarchicalNode;
use embedag_example_receipts::finance::*;
mod common;

use common::Backend;
use rstest::rstest;
use std::collections::HashMap;
use std::sync::Arc;

/// A variant-carrying line item: id, name, unit price, quantity and (variant name, units, package).
fn item(id: &str, name: &str, price: f64, quantity: u32, v: (&str, u32, &str)) -> InvoiceItem {
    InvoiceItem::new(id, name, price, quantity, Some(variant(v.0, v.1, v.2)))
}

fn variant(name: &str, units_count: u32, package_type: &str) -> ProductVariant {
    ProductVariant {
        variant_name: name.into(),
        units_count,
        package_type: package_type.into(),
    }
}

fn merchant(
    id: &str,
    name: &str,
    tags: &[&str],
    category_id: &str,
    category_name: &str,
) -> FinanceMerchant {
    FinanceMerchant {
        merchant_id: id.into(),
        name: name.into(),
        tags: tags.iter().map(|t| t.to_string()).collect(),
        category_id: category_id.into(),
        category_name: category_name.into(),
    }
}

#[rstest]
#[tokio::test]
async fn test_finance_invoice_categorization_llm_decided_and_time_dependency(
    #[values(Backend::Candle, Backend::Oxillama)] backend: Backend,
) {
    let provider = backend.provider();
    let mut engine = FinanceCategorizationEngine::new(provider);

    // 1. Register merchant with commercial tags in merchant_categories DAG
    let merchant = merchant(
        "m_metro_market",
        "Metro Supermarket & Spirits",
        &["grocery", "retail", "organic"],
        "mcat_supermarket",
        "Supermarket",
    );
    engine.register_merchant(merchant);

    assert_eq!(engine.merchant_dag.node_count(), 1);
    assert_eq!(engine.item_dag.node_count(), 0);
    assert_eq!(engine.purchase_dag.node_count(), 0);
    assert_eq!(engine.invoice_dag.node_count(), 0);

    // 2. Ingest regular purchase invoice (non-holiday) with variants
    let inv1 = FinanceInvoice::new(
        "INV-CANDLE-01",
        "m_metro_market",
        parse_date("2026-05-18"),
        vec![
            item(
                "item_snickers_pack",
                "Snickers Bar 50g",
                1.25,
                4,
                ("4-pack", 4, "pack"),
            ),
            item(
                "item_ferrero_3",
                "Ferrero Rocher (3-pack)",
                2.75,
                2,
                ("3-pack", 3, "pack"),
            ),
        ],
        10.50,
    );

    let processed1 = engine
        .process_invoice(inv1, &InvoiceCategorizationMode::LlmDecided)
        .await
        .expect("process invoice 1");

    assert!(!processed1.invoice_categories.is_empty());
    assert!(
        processed1
            .invoice_categories
            .contains(&"Everyday Retail".to_string())
    );

    // Both Ferrero and Snickers categorized into Chocolate hierarchy
    let snickers_item_cat = processed1
        .item_categories
        .get("item_snickers_pack")
        .unwrap();
    assert_eq!(snickers_item_cat.1, "Chocolate Bar");

    let ferrero_item_cat = processed1.item_categories.get("item_ferrero_3").unwrap();
    assert_eq!(ferrero_item_cat.1, "Hazelnut Chocolate");

    // Verify item DAG has Chocolate root and sub-categories
    let chocolate_root = engine.item_dag.node(&"cat_chocolate".to_string()).unwrap();
    assert_eq!(chocolate_root.node_name(), "Chocolate");

    // 3. Ingest holiday purchase invoice (December 25 - Christmas Day)
    // Holiday nature is inferred from date rather than pre-existing on invoice
    let inv2 = FinanceInvoice::new(
        "INV-CANDLE-02",
        "m_metro_market",
        parse_date("2026-12-25"),
        vec![item(
            "item_ferrero_box16",
            "Ferrero Rocher (Box of 16 Festive Edition)",
            14.99,
            2,
            ("Box of 16", 16, "box"),
        )],
        29.98,
    );

    let processed2 = engine
        .process_invoice(inv2, &InvoiceCategorizationMode::LlmDecided)
        .await
        .expect("process holiday invoice 2");

    assert!(
        processed2
            .invoice_categories
            .contains(&"Holiday".to_string()),
        "Invoice near Christmas must include Holiday category"
    );

    // Purchase categories for Ferrero near Christmas must include [Holiday, Gift]
    let holiday_purchase_cats = processed2
        .purchase_categories
        .get("item_ferrero_box16")
        .unwrap();
    let cat_names: Vec<&str> = holiday_purchase_cats
        .iter()
        .map(|(_, name)| name.as_str())
        .collect();
    assert!(cat_names.contains(&"Holiday"));
    assert!(cat_names.contains(&"Gift"));

    // 4. Ingest birthday gift purchase invoice to validate [Gift, Birthday Gift] hierarchy
    let inv3 = FinanceInvoice {
        invoice_id: "INV-CANDLE-03".into(),
        merchant_id: "m_metro_market".into(),
        date: parse_date("2026-07-15"),
        purchase_content: Some("today is John's birthday".into()),
        items: vec![item(
            "item_ferrero_bday",
            "Ferrero Rocher Confectionery Box",
            12.50,
            1,
            ("Birthday Gift Wrap Edition", 12, "box"),
        )],
        total: 12.50,
    };

    let processed3 = engine
        .process_invoice(inv3, &InvoiceCategorizationMode::LlmDecided)
        .await
        .expect("process birthday gift invoice");

    let bday_purchase_cats = processed3
        .purchase_categories
        .get("item_ferrero_bday")
        .unwrap();
    let bday_names: Vec<&str> = bday_purchase_cats
        .iter()
        .map(|(_, name)| name.as_str())
        .collect();
    assert!(bday_names.contains(&"Gift"));
    assert!(bday_names.contains(&"Birthday Gift"));

    // Verify in purchase_dag that Birthday Gift has parent Gift ("birthday gift is a gift child")
    let bday_node = engine
        .purchase_dag
        .node(&"pcat_birthday_gift".to_string())
        .unwrap();
    assert_eq!(bday_node.parent_id(), Some("pcat_gift".to_string()));
}

#[rstest]
#[tokio::test]
async fn test_finance_invoice_categorization_custom_density_and_user_overrides(
    #[values(Backend::Candle, Backend::Oxillama)] backend: Backend,
) {
    let provider = backend.provider();
    let mut engine = FinanceCategorizationEngine::new(provider);

    // 1. Register merchant initially as General Retail
    let merchant = merchant(
        "m_city_mart",
        "City Mart Downtown",
        &["urban", "retail", "drinks"],
        "mcat_retail",
        "General Retail",
    );
    engine.register_merchant(merchant);

    // 2. Condition 1: User manually updates merchant by adding "Convenience Store"
    let updated = engine.user_update_merchant_category("m_city_mart", "Convenience Store");
    assert!(updated);

    let merchant_record = engine.merchants.get("m_city_mart").unwrap();
    assert_eq!(merchant_record.category_name, "Convenience Store");
    assert_eq!(merchant_record.category_id, "mcat_convenience_store");

    // 3. Condition 2: Custom density function provided by developer
    // Evaluates invoice density = total / sum_of_items_in_category
    let density_evaluator = Arc::new(
        |inv: &FinanceInvoice, category_spends: &HashMap<String, f64>| -> Vec<String> {
            let choco_spend = category_spends
                .iter()
                .filter(|(name, _)| name.contains("Chocolate"))
                .map(|(_, spend)| *spend)
                .sum::<f64>();

            let density = if choco_spend > 0.0 {
                inv.total / choco_spend
            } else {
                1.0
            };

            if density <= 1.05 && inv.total >= 40.0 {
                vec!["Bulk Pure Confectionery".to_string()]
            } else if density <= 1.3 {
                vec!["Predominantly Sweet Basket".to_string()]
            } else {
                vec!["General Sundries".to_string()]
            }
        },
    );

    // The invoice total is what its lines add up to: 5 x 2.00 + 2 x 15.00
    let lines = vec![
        item(
            "item_snickers_bar",
            "Snickers Bar 50g",
            2.00,
            5,
            ("Single Bar 50g", 1, "single"),
        ),
        item(
            "item_ferrero_box16",
            "Ferrero Rocher (Box of 16)",
            15.00,
            2,
            ("Box of 16", 16, "box"),
        ),
    ];
    let total = lines.iter().map(|line| line.total_price).sum();
    let inv = FinanceInvoice::new(
        "INV-CANDLE-DENSITY-01",
        "m_city_mart",
        parse_date("2026-07-04"),
        lines,
        total,
    );

    let processed = engine
        .process_invoice(
            inv,
            &InvoiceCategorizationMode::CustomDensity(density_evaluator),
        )
        .await
        .expect("process invoice density");

    // Item categories controlled by intrinsic item taxonomy:
    assert_eq!(
        processed
            .item_categories
            .get("item_snickers_bar")
            .unwrap()
            .1,
        "Chocolate Bar"
    );
    assert_eq!(
        processed
            .item_categories
            .get("item_ferrero_box16")
            .unwrap()
            .1,
        "Hazelnut Chocolate"
    );

    // Invoice category assigned by custom density function (40.0 / 40.0 = 1.0 <= 1.05 and total >= 40):
    assert_eq!(
        processed.invoice_categories,
        vec!["Bulk Pure Confectionery".to_string()]
    );

    // 4. Condition 3: User adds item category "Luxury Chocolate"
    // Ferrero Rocher is luxury chocolate, Snickers is standard chocolate.
    let reclassified =
        engine.user_add_item_category_and_reclassify("Luxury Chocolate", "Chocolate", |item| {
            item.item_name.contains("Ferrero")
        });
    assert_eq!(reclassified, 1);

    let updated_inv = &engine.invoices[0];
    assert_eq!(
        updated_inv
            .item_categories
            .get("item_snickers_bar")
            .unwrap()
            .1,
        "Chocolate Bar"
    );
    assert_eq!(
        updated_inv
            .item_categories
            .get("item_ferrero_box16")
            .unwrap()
            .1,
        "Luxury Chocolate"
    );

    // Verify invoice received "Luxury" category after reclassification
    assert!(
        updated_inv
            .invoice_categories
            .contains(&"Luxury".to_string())
    );
}

#[rstest]
#[tokio::test]
async fn test_finance_invoice_walmart_vs_prada_tshirt_categorization(
    #[values(Backend::Candle, Backend::Oxillama)] backend: Backend,
) {
    let provider = backend.provider();
    let mut engine = FinanceCategorizationEngine::new(provider);

    // Register Walmart: Discount Department Store
    let walmart = merchant(
        "m_walmart",
        "Walmart Supercenter",
        &["discount", "mass_market", "retail"],
        "mcat_discount_store",
        "Discount Department Store",
    );
    engine.register_merchant(walmart);

    // Register Prada: Luxury Fashion Boutique
    let prada = FinanceMerchant {
        merchant_id: "m_prada".into(),
        name: "Prada Milano".into(),
        tags: vec![
            "luxury".into(),
            "designer".into(),
            "haute_couture".into(),
            "high_fashion".into(),
        ],
        category_id: "mcat_luxury_boutique".into(),
        category_name: "Luxury Fashion Boutique".into(),
    };
    engine.register_merchant(prada);

    // 1. Ingest Walmart Invoice with a generic Cotton Crewneck T-Shirt ($12.00)
    let walmart_invoice = FinanceInvoice::new(
        "INV-CANDLE-WALMART-01",
        "m_walmart",
        parse_date("2026-06-15"),
        vec![item(
            "item_tshirt_walmart",
            "Cotton Crewneck T-Shirt",
            12.00,
            2,
            ("Standard Pack", 2, "pack"),
        )],
        24.00,
    );

    let processed_walmart = engine
        .process_invoice(walmart_invoice, &InvoiceCategorizationMode::LlmDecided)
        .await
        .expect("process walmart invoice");

    // Product-intrinsic item taxonomy: BOTH are T-Shirt under Apparel!
    let walmart_item_cat = processed_walmart
        .item_categories
        .get("item_tshirt_walmart")
        .unwrap();
    assert_eq!(walmart_item_cat.1, "T-Shirt");

    // Purchase categories for Walmart: Everyday Retail
    let walmart_p_cats = processed_walmart
        .purchase_categories
        .get("item_tshirt_walmart")
        .unwrap();
    let walmart_p_names: Vec<&str> = walmart_p_cats
        .iter()
        .map(|(_, name)| name.as_str())
        .collect();
    assert!(walmart_p_names.contains(&"Everyday Retail"));
    assert!(!walmart_p_names.contains(&"Luxury"));

    // Invoice category: Everyday Retail
    assert!(
        processed_walmart
            .invoice_categories
            .contains(&"Everyday Retail".to_string())
    );

    // 2. Ingest Prada Invoice with the SAME item name Cotton Crewneck T-Shirt ($650.00)
    let prada_invoice = FinanceInvoice::new(
        "INV-CANDLE-PRADA-01",
        "m_prada",
        parse_date("2026-06-16"),
        vec![item(
            "item_tshirt_prada",
            "Cotton Crewneck T-Shirt",
            650.00,
            1,
            ("Single Piece", 1, "single"),
        )],
        650.00,
    );

    let processed_prada = engine
        .process_invoice(prada_invoice, &InvoiceCategorizationMode::LlmDecided)
        .await
        .expect("process prada invoice");

    // Product-intrinsic item taxonomy: Prada item is ALSO a T-Shirt!
    let prada_item_cat = processed_prada
        .item_categories
        .get("item_tshirt_prada")
        .unwrap();
    assert_eq!(prada_item_cat.1, "T-Shirt");

    // BUT context-dependent purchase categories for Prada: [Luxury, Designer Fashion]
    let prada_p_cats = processed_prada
        .purchase_categories
        .get("item_tshirt_prada")
        .unwrap();
    let prada_p_names: Vec<&str> = prada_p_cats.iter().map(|(_, name)| name.as_str()).collect();
    assert!(prada_p_names.contains(&"Luxury"));
    assert!(prada_p_names.contains(&"Designer Fashion"));

    // Invoice category: Luxury
    assert!(
        processed_prada
            .invoice_categories
            .contains(&"Luxury".to_string())
    );

    // Verify Designer Fashion is child of Luxury in purchase_dag
    let des_node = engine
        .purchase_dag
        .node(&"pcat_designer_fashion".to_string())
        .unwrap();
    assert_eq!(des_node.parent_id(), Some("pcat_luxury".to_string()));
}

#[rstest]
#[tokio::test]
async fn test_finance_invoice_pure_llm_and_purchase_content_context(
    #[values(Backend::Candle, Backend::Oxillama)] backend: Backend,
) {
    let provider = backend.provider();
    let mut engine = FinanceCategorizationEngine::new(provider);

    let boutique = merchant(
        "m_boutique",
        "Saint Laurent Paris",
        &["luxury", "designer"],
        "mcat_luxury_boutique",
        "Luxury Fashion Boutique",
    );
    engine.register_merchant(boutique);

    // Scenario A: purchase_content = "my birthday" -> assigns Birthday Celebration (not gift)
    let inv_self = FinanceInvoice::new(
        "INV-SELF-BDAY",
        "m_boutique",
        parse_date("2026-08-20"),
        vec![InvoiceItem::new(
            "item_jacket",
            "Wool Tailored Jacket",
            1200.0,
            1,
            None,
        )],
        1200.0,
    )
    .with_purchase_content("celebrating my birthday with self purchase");

    let processed_self = engine
        .process_invoice(inv_self, &InvoiceCategorizationMode::LlmDecided)
        .await
        .expect("process self birthday");

    let p_cats_self = processed_self
        .purchase_categories
        .get("item_jacket")
        .unwrap();
    let names_self: Vec<&str> = p_cats_self.iter().map(|(_, n)| n.as_str()).collect();
    assert!(names_self.contains(&"Birthday Celebration"));
    assert!(!names_self.contains(&"Gift")); // "my birthday" -> not a gift!

    // Scenario B: purchase_content = "today is Alice's birthday" -> assigns Gift AND Birthday Gift
    let inv_gift = FinanceInvoice::new(
        "INV-FRIEND-BDAY",
        "m_boutique",
        parse_date("2026-08-21"),
        vec![InvoiceItem::new(
            "item_scarf",
            "Silk Printed Scarf",
            350.0,
            1,
            None,
        )],
        350.0,
    )
    .with_purchase_content("today is Alice's birthday");

    let processed_gift = engine
        .process_invoice(inv_gift, &InvoiceCategorizationMode::LlmDecided)
        .await
        .expect("process friend birthday gift");

    let p_cats_gift = processed_gift
        .purchase_categories
        .get("item_scarf")
        .unwrap();
    let names_gift: Vec<&str> = p_cats_gift.iter().map(|(_, n)| n.as_str()).collect();
    assert!(names_gift.contains(&"Gift"));
    assert!(names_gift.contains(&"Birthday Gift"));
}
