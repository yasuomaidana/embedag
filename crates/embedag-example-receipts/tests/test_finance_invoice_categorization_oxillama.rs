use embedag_diesel::cte::HierarchicalNode;
use embedag_example_receipts::finance::*;
use embedag_llm::provider::OxillamaProvider;
use std::collections::HashMap;
use std::sync::Arc;

#[tokio::test]
async fn test_finance_invoice_categorization_oxillama_llm_decided_and_time_dependency() {
    let provider = OxillamaProvider::new("llama-3-8b-instruct", "q4_k_m");
    let mut engine = FinanceCategorizationEngine::new(provider);

    // 1. Register merchant with commercial tags in merchant_categories DAG
    let merchant = FinanceMerchant {
        merchant_id: "m_express_fuel".into(),
        name: "Express Fuel & Snacks".into(),
        tags: vec!["fuel".into(), "retail".into(), "24h".into()],
        category_id: "mcat_gas_station".into(),
        category_name: "Gas Station".into(),
    };
    engine.register_merchant(merchant);

    assert_eq!(engine.merchant_dag.node_count(), 1);
    assert_eq!(engine.item_dag.node_count(), 0);
    assert_eq!(engine.purchase_dag.node_count(), 0);
    assert_eq!(engine.invoice_dag.node_count(), 0);

    // 2. Ingest regular purchase invoice (non-holiday) with variants
    // Note: invoice does NOT have is_holiday_purchase or seasonal_context fields
    let inv1 = FinanceInvoice {
        invoice_id: "INV-101".into(),
        merchant_id: "m_express_fuel".into(),
        date: parse_date("2026-10-12"),
        purchase_content: None,
        items: vec![
            InvoiceItem::new(
                "item_snickers",
                "Snickers Bar 50g",
                1.50,
                2,
                Some(ProductVariant {
                    variant_name: "Single Bar 50g".into(),
                    units_count: 1,
                    package_type: "single".into(),
                }),
            ),
            InvoiceItem::new(
                "item_ferrero_3",
                "Ferrero Rocher (3-pack)",
                2.50,
                1,
                Some(ProductVariant {
                    variant_name: "3-pack".into(),
                    units_count: 3,
                    package_type: "pack".into(),
                }),
            ),
            InvoiceItem::new(
                "item_ferrero_box16",
                "Ferrero Rocher (Box of 16)",
                9.99,
                1,
                Some(ProductVariant {
                    variant_name: "Box of 16".into(),
                    units_count: 16,
                    package_type: "box".into(),
                }),
            ),
        ],
        total: 15.49,
    };

    // Mode A: Let LLM decide invoice category
    let processed1 = engine
        .process_invoice(inv1, &InvoiceCategorizationMode::LlmDecided)
        .await
        .expect("process invoice 1");

    assert!(!processed1.invoice_categories.is_empty());
    assert!(processed1.invoice_categories.contains(&"Everyday Retail".to_string()));

    // Both Ferrero variants and Snickers categorized into Chocolate hierarchy
    let snickers_item_cat = processed1.item_categories.get("item_snickers").unwrap();
    assert_eq!(snickers_item_cat.1, "Chocolate Bar");

    let ferrero_item_cat = processed1.item_categories.get("item_ferrero_3").unwrap();
    assert_eq!(ferrero_item_cat.1, "Hazelnut Chocolate");

    // 3. Ingest holiday purchase invoice (December 24 - Christmas Eve)
    // Holiday nature is inferred dynamically by the system from the invoice date
    let inv2 = FinanceInvoice {
        invoice_id: "INV-102".into(),
        merchant_id: "m_express_fuel".into(),
        date: parse_date("2026-12-24"),
        purchase_content: None,
        items: vec![InvoiceItem::new(
            "item_ferrero_box24",
            "Ferrero Rocher (Box of 24 Gift Edition)",
            18.50,
            3,
            Some(ProductVariant {
                variant_name: "Box of 24".into(),
                units_count: 24,
                package_type: "box".into(),
            }),
        )],
        total: 55.50,
    };

    let processed2 = engine
        .process_invoice(inv2, &InvoiceCategorizationMode::LlmDecided)
        .await
        .expect("process invoice 2");

    assert!(
        processed2.invoice_categories.contains(&"Holiday".to_string()),
        "LLM must infer Christmas Eve gifting invoice as Holiday"
    );

    let holiday_purchase_cats = processed2.purchase_categories.get("item_ferrero_box24").unwrap();
    let cat_names: Vec<&str> = holiday_purchase_cats.iter().map(|(_, name)| name.as_str()).collect();
    assert!(cat_names.contains(&"Holiday"));
    assert!(cat_names.contains(&"Gift"));

    // 4. Ingest birthday gift purchase invoice to validate [Gift, Birthday Gift] hierarchy
    let inv3 = FinanceInvoice {
        invoice_id: "INV-103".into(),
        merchant_id: "m_express_fuel".into(),
        date: parse_date("2026-08-20"),
        purchase_content: Some("today is John's birthday".into()),
        items: vec![InvoiceItem::new(
            "item_ferrero_bday",
            "Ferrero Rocher Festive Box",
            15.00,
            1,
            Some(ProductVariant {
                variant_name: "Birthday Ribbon Edition".into(),
                units_count: 16,
                package_type: "box".into(),
            }),
        )],
        total: 15.00,
    };

    let processed3 = engine
        .process_invoice(inv3, &InvoiceCategorizationMode::LlmDecided)
        .await
        .expect("process invoice 3 birthday");

    let bday_purchase_cats = processed3.purchase_categories.get("item_ferrero_bday").unwrap();
    let bday_names: Vec<&str> = bday_purchase_cats.iter().map(|(_, name)| name.as_str()).collect();
    assert!(bday_names.contains(&"Gift"));
    assert!(bday_names.contains(&"Birthday Gift"));

    let bday_node = engine.purchase_dag.node(&"pcat_birthday_gift".to_string()).unwrap();
    assert_eq!(bday_node.parent_id(), Some("pcat_gift".to_string()));
}

#[tokio::test]
async fn test_finance_invoice_categorization_oxillama_custom_density_and_user_overrides() {
    let provider = OxillamaProvider::new("llama-3-8b-instruct", "q4_k_m");
    let mut engine = FinanceCategorizationEngine::new(provider);

    // 1. Register merchant initially as Gas Station
    let merchant = FinanceMerchant {
        merchant_id: "m_quickstop".into(),
        name: "QuickStop Corner".into(),
        tags: vec!["retail".into(), "gas_station".into(), "lottery".into()],
        category_id: "mcat_gas_station".into(),
        category_name: "Gas Station".into(),
    };
    engine.register_merchant(merchant);

    // 2. Condition 1: User manually updates merchant by adding "Convenience Store" category
    let updated_merchant = engine.user_update_merchant_category("m_quickstop", "Convenience Store");
    assert!(updated_merchant, "Merchant category update must succeed");

    let updated_record = engine.merchants.get("m_quickstop").unwrap();
    assert_eq!(updated_record.category_name, "Convenience Store");
    assert_eq!(updated_record.category_id, "mcat_convenience_store");

    // 3. Condition 2: Mode B with custom developer-provided density function
    // Developer provides custom function evaluating density: total / sum_of_items_in_category
    let density_evaluator = Arc::new(
        |inv: &FinanceInvoice, category_spends: &HashMap<String, f64>| -> Vec<String> {
            let chocolate_spend = category_spends
                .iter()
                .filter(|(name, _)| name.contains("Chocolate"))
                .map(|(_, spend)| *spend)
                .sum::<f64>();

            let density = if chocolate_spend > 0.0 {
                inv.total / chocolate_spend
            } else {
                1.0
            };

            if density <= 1.1 && inv.total > 20.0 {
                vec!["High Density Confectionery Wholesale".to_string()]
            } else if density <= 1.25 {
                vec!["Concentrated Sweet Spend".to_string()]
            } else {
                vec!["Diversified Mixed Basket".to_string()]
            }
        },
    );

    let inv = FinanceInvoice {
        invoice_id: "INV-201".into(),
        merchant_id: "m_quickstop".into(),
        date: parse_date("2026-10-15"),
        purchase_content: None,
        items: vec![
            InvoiceItem::new(
                "item_snickers",
                "Snickers Bar 50g",
                1.50,
                4,
                Some(ProductVariant {
                    variant_name: "4-pack".into(),
                    units_count: 4,
                    package_type: "pack".into(),
                }),
            ),
            InvoiceItem::new(
                "item_ferrero_box",
                "Ferrero Rocher (Box of 16)",
                12.00,
                2,
                Some(ProductVariant {
                    variant_name: "Box of 16".into(),
                    units_count: 16,
                    package_type: "box".into(),
                }),
            ),
        ],
        total: 30.00,
    };

    let processed = engine
        .process_invoice(inv, &InvoiceCategorizationMode::CustomDensity(density_evaluator))
        .await
        .expect("process invoice with custom density function");

    // Item categories remain fully controlled by LLM:
    assert_eq!(
        processed.item_categories.get("item_snickers").unwrap().1,
        "Chocolate Bar"
    );
    assert_eq!(
        processed.item_categories.get("item_ferrero_box").unwrap().1,
        "Hazelnut Chocolate"
    );

    // Invoice category assigned by developer's custom density function
    assert_eq!(
        processed.invoice_categories,
        vec!["High Density Confectionery Wholesale".to_string()]
    );

    // 4. Condition 3: User adds item category "Luxury Chocolate"
    // Ferrero Rocher is classified as luxury chocolate, while Snickers is standard chocolate.
    // All invoices and items must update!
    let reclassified = engine.user_add_item_category_and_reclassify(
        "Luxury Chocolate",
        "Chocolate",
        |item| item.item_name.contains("Ferrero"),
    );

    assert_eq!(reclassified, 1, "Exactly 1 Ferrero line item reclassified");

    let updated_inv = &engine.invoices[0];
    assert_eq!(
        updated_inv.item_categories.get("item_snickers").unwrap().1,
        "Chocolate Bar"
    );
    assert_eq!(
        updated_inv.item_categories.get("item_ferrero_box").unwrap().1,
        "Luxury Chocolate"
    );

    // Invoice category updated to reflect Luxury
    assert!(updated_inv.invoice_categories.contains(&"Luxury".to_string()));
}

#[tokio::test]
async fn test_finance_invoice_oxillama_walmart_vs_prada_tshirt_categorization() {
    let provider = OxillamaProvider::new("llama-3-8b-instruct", "q4_k_m");
    let mut engine = FinanceCategorizationEngine::new(provider);

    // Register Walmart: Discount Department Store / Mass market
    let walmart = FinanceMerchant {
        merchant_id: "m_walmart".into(),
        name: "Walmart Supercenter".into(),
        tags: vec!["discount".into(), "mass_market".into(), "retail".into()],
        category_id: "mcat_discount_store".into(),
        category_name: "Discount Department Store".into(),
    };
    engine.register_merchant(walmart);

    // Register Prada: Luxury Fashion Boutique / Haute Couture
    let prada = FinanceMerchant {
        merchant_id: "m_prada".into(),
        name: "Prada Milano".into(),
        tags: vec!["luxury".into(), "designer".into(), "haute_couture".into(), "high_fashion".into()],
        category_id: "mcat_luxury_boutique".into(),
        category_name: "Luxury Fashion Boutique".into(),
    };
    engine.register_merchant(prada);

    // 1. Ingest Walmart Invoice with a generic Cotton Crewneck T-Shirt ($12.00)
    let walmart_invoice = FinanceInvoice {
        invoice_id: "INV-WALMART-001".into(),
        merchant_id: "m_walmart".into(),
        date: parse_date("2026-06-15"),
        purchase_content: None,
        items: vec![InvoiceItem::new(
            "item_tshirt_walmart",
            "Cotton Crewneck T-Shirt",
            12.00,
            2,
            Some(ProductVariant {
                variant_name: "Standard Pack".into(),
                units_count: 2,
                package_type: "pack".into(),
            }),
        )],
        total: 24.00,
    };

    let processed_walmart = engine
        .process_invoice(walmart_invoice, &InvoiceCategorizationMode::LlmDecided)
        .await
        .expect("process walmart invoice");

    // Item category at Walmart: intrinsic T-Shirt
    let walmart_item_cat = processed_walmart.item_categories.get("item_tshirt_walmart").unwrap();
    assert_eq!(walmart_item_cat.1, "T-Shirt");

    // Purchase categories for Walmart: Everyday Retail
    let walmart_p_cats = processed_walmart.purchase_categories.get("item_tshirt_walmart").unwrap();
    let walmart_p_names: Vec<&str> = walmart_p_cats.iter().map(|(_, name)| name.as_str()).collect();
    assert!(walmart_p_names.contains(&"Everyday Retail"));
    assert!(!walmart_p_names.contains(&"Luxury"));

    // Invoice category: Everyday Retail
    assert!(processed_walmart.invoice_categories.contains(&"Everyday Retail".to_string()));

    // 2. Ingest Prada Invoice with the SAME item name Cotton Crewneck T-Shirt ($650.00)
    let prada_invoice = FinanceInvoice {
        invoice_id: "INV-PRADA-001".into(),
        merchant_id: "m_prada".into(),
        date: parse_date("2026-06-16"),
        purchase_content: None,
        items: vec![InvoiceItem::new(
            "item_tshirt_prada",
            "Cotton Crewneck T-Shirt",
            650.00,
            1,
            Some(ProductVariant {
                variant_name: "Single Piece".into(),
                units_count: 1,
                package_type: "single".into(),
            }),
        )],
        total: 650.00,
    };

    let processed_prada = engine
        .process_invoice(prada_invoice, &InvoiceCategorizationMode::LlmDecided)
        .await
        .expect("process prada invoice");

    // Item category at Prada is ALSO intrinsic T-Shirt!
    let prada_item_cat = processed_prada.item_categories.get("item_tshirt_prada").unwrap();
    assert_eq!(prada_item_cat.1, "T-Shirt");

    // BUT context-dependent purchase categories for Prada: [Luxury, Designer Fashion]
    let prada_p_cats = processed_prada.purchase_categories.get("item_tshirt_prada").unwrap();
    let prada_p_names: Vec<&str> = prada_p_cats.iter().map(|(_, name)| name.as_str()).collect();
    assert!(prada_p_names.contains(&"Luxury"));
    assert!(prada_p_names.contains(&"Designer Fashion"));

    // Invoice category: Luxury
    assert!(processed_prada.invoice_categories.contains(&"Luxury".to_string()));

    // Verify Designer Fashion is child of Luxury in purchase_dag
    let des_node = engine.purchase_dag.node(&"pcat_designer_fashion".to_string()).unwrap();
    assert_eq!(des_node.parent_id(), Some("pcat_luxury".to_string()));
}

#[tokio::test]
async fn test_finance_invoice_oxillama_pure_llm_and_purchase_content_context() {
    let provider = OxillamaProvider::new("llama-3-8b-instruct", "q4_k_m");
    let mut engine = FinanceCategorizationEngine::new(provider);

    let boutique = FinanceMerchant {
        merchant_id: "m_boutique".into(),
        name: "Saint Laurent Paris".into(),
        tags: vec!["luxury".into(), "designer".into()],
        category_id: "mcat_luxury_boutique".into(),
        category_name: "Luxury Fashion Boutique".into(),
    };
    engine.register_merchant(boutique);

    // Scenario A: purchase_content = "my birthday" -> assigns Birthday Celebration (not gift)
    let inv_self = FinanceInvoice::new(
        "INV-SELF-BDAY-OXI",
        "m_boutique",
        parse_date("2026-08-20"),
        vec![InvoiceItem::new("item_jacket", "Wool Tailored Jacket", 1200.0, 1, None)],
        1200.0,
    )
    .with_purchase_content("celebrating my birthday with self purchase");

    let processed_self = engine
        .process_invoice(inv_self, &InvoiceCategorizationMode::LlmDecided)
        .await
        .expect("process self birthday oxillama");

    let p_cats_self = processed_self.purchase_categories.get("item_jacket").unwrap();
    let names_self: Vec<&str> = p_cats_self.iter().map(|(_, n)| n.as_str()).collect();
    assert!(names_self.contains(&"Birthday Celebration"));
    assert!(!names_self.contains(&"Gift"));

    // Scenario B: purchase_content = "today is Alice's birthday" -> assigns Gift AND Birthday Gift
    let inv_gift = FinanceInvoice::new(
        "INV-FRIEND-BDAY-OXI",
        "m_boutique",
        parse_date("2026-08-21"),
        vec![InvoiceItem::new("item_scarf", "Silk Printed Scarf", 350.0, 1, None)],
        350.0,
    )
    .with_purchase_content("today is Alice's birthday");

    let processed_gift = engine
        .process_invoice(inv_gift, &InvoiceCategorizationMode::LlmDecided)
        .await
        .expect("process friend birthday gift oxillama");

    let p_cats_gift = processed_gift.purchase_categories.get("item_scarf").unwrap();
    let names_gift: Vec<&str> = p_cats_gift.iter().map(|(_, n)| n.as_str()).collect();
    assert!(names_gift.contains(&"Gift"));
    assert!(names_gift.contains(&"Birthday Gift"));
}
