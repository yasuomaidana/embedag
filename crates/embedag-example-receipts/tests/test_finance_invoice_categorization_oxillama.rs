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
    assert_eq!(engine.purchase_dag.node_count(), 0);

    // 2. Ingest regular purchase invoice (non-holiday) with variants
    // Note: invoice does NOT have is_holiday_purchase or seasonal_context fields
    let inv1 = FinanceInvoice {
        invoice_id: "INV-101".into(),
        merchant_id: "m_express_fuel".into(),
        date: "2026-10-12".into(), // Regular autumn day
        items: vec![
            FinanceInvoiceItem {
                item_id: "item_snickers".into(),
                item_name: "Snickers Bar 50g".into(),
                unit_price: 1.50,
                quantity: 2,
                total_price: 3.00,
                product_category_id: None,
                product_category_name: None,
                variant: Some(ProductVariant {
                    variant_name: "Single Bar 50g".into(),
                    units_count: 1,
                    package_type: "single".into(),
                }),
            },
            FinanceInvoiceItem {
                item_id: "item_ferrero_3".into(),
                item_name: "Ferrero Rocher (3-pack)".into(),
                unit_price: 2.50,
                quantity: 1,
                total_price: 2.50,
                product_category_id: None,
                product_category_name: None,
                variant: Some(ProductVariant {
                    variant_name: "3-pack".into(),
                    units_count: 3,
                    package_type: "pack".into(),
                }),
            },
            FinanceInvoiceItem {
                item_id: "item_ferrero_box16".into(),
                item_name: "Ferrero Rocher (Box of 16)".into(),
                unit_price: 9.99,
                quantity: 1,
                total_price: 9.99,
                product_category_id: None,
                product_category_name: None,
                variant: Some(ProductVariant {
                    variant_name: "Box of 16".into(),
                    units_count: 16,
                    package_type: "box".into(),
                }),
            },
        ],
        total: 15.49,
        invoice_category: None,
    };

    // Mode A: Let LLM decide invoice category
    let processed1 = engine
        .process_invoice(inv1, &InvoiceCategorizationMode::LlmDecided)
        .await
        .expect("process invoice 1");

    assert!(processed1.invoice_category.is_some());
    assert_eq!(
        processed1.invoice_category.as_deref(),
        Some("Everyday Retail")
    );

    // Both Ferrero variants and Snickers categorized into Chocolate / Confectionery
    for item in &processed1.items {
        assert!(item.product_category_name.is_some());
        let cat = item.product_category_name.as_ref().unwrap();
        assert!(
            cat.contains("Chocolate") || cat.contains("Groceries"),
            "Item category must be derived by LLM, got: {}",
            cat
        );
    }

    // 3. Ingest holiday purchase invoice (December 24 - Christmas Eve)
    // Holiday nature is inferred dynamically by the system from the invoice date
    let inv2 = FinanceInvoice {
        invoice_id: "INV-102".into(),
        merchant_id: "m_express_fuel".into(),
        date: "2026-12-24".into(), // Christmas Eve holiday purchase
        items: vec![FinanceInvoiceItem {
            item_id: "item_ferrero_box24".into(),
            item_name: "Ferrero Rocher (Box of 24 Gift Edition)".into(),
            unit_price: 18.50,
            quantity: 3,
            total_price: 55.50,
            product_category_id: None,
            product_category_name: None,
            variant: Some(ProductVariant {
                variant_name: "Box of 24".into(),
                units_count: 24,
                package_type: "box".into(),
            }),
        }],
        total: 55.50,
        invoice_category: None,
    };

    let processed2 = engine
        .process_invoice(inv2, &InvoiceCategorizationMode::LlmDecided)
        .await
        .expect("process invoice 2");

    // Dynamic holiday inference verification:
    // Invoice category is assigned "Holiday Gifting" based on inferred holiday date context
    assert_eq!(
        processed2.invoice_category.as_deref(),
        Some("Holiday Gifting"),
        "LLM must infer Christmas Eve gifting invoice as Holiday Gifting"
    );
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

    // Verify merchant_categories DAG updated
    let updated_record = engine.merchants.get("m_quickstop").unwrap();
    assert_eq!(updated_record.category_name, "Convenience Store");
    assert_eq!(updated_record.category_id, "mcat_convenience_store");
    assert!(
        engine
            .merchant_dag
            .node(&"mcat_convenience_store".to_string())
            .is_some(),
        "New category node must exist in merchant_categories DAG"
    );

    // 3. Condition 2: Mode B with custom developer-provided density function
    // Developer provides custom function evaluating density: total / sum_of_items_in_category
    let density_evaluator = Arc::new(
        |inv: &FinanceInvoice, category_spends: &HashMap<String, f64>| -> String {
            let chocolate_spend = category_spends.get("Chocolate").copied().unwrap_or(0.0);
            let density = if chocolate_spend > 0.0 {
                inv.total / chocolate_spend
            } else {
                1.0
            };

            if density <= 1.1 && inv.total > 20.0 {
                "High Density Confectionery Wholesale".to_string()
            } else if density <= 1.25 {
                "Concentrated Sweet Spend".to_string()
            } else {
                "Diversified Mixed Basket".to_string()
            }
        },
    );

    let inv = FinanceInvoice {
        invoice_id: "INV-201".into(),
        merchant_id: "m_quickstop".into(),
        date: "2026-10-15".into(),
        items: vec![
            FinanceInvoiceItem {
                item_id: "item_snickers".into(),
                item_name: "Snickers Bar 50g".into(),
                unit_price: 1.50,
                quantity: 4,
                total_price: 6.00,
                product_category_id: None,
                product_category_name: None,
                variant: Some(ProductVariant {
                    variant_name: "4-pack".into(),
                    units_count: 4,
                    package_type: "pack".into(),
                }),
            },
            FinanceInvoiceItem {
                item_id: "item_ferrero_box".into(),
                item_name: "Ferrero Rocher (Box of 16)".into(),
                unit_price: 12.00,
                quantity: 2,
                total_price: 24.00,
                product_category_id: None,
                product_category_name: None,
                variant: Some(ProductVariant {
                    variant_name: "Box of 16".into(),
                    units_count: 16,
                    package_type: "box".into(),
                }),
            },
        ],
        total: 30.00,
        invoice_category: None,
    };

    let processed = engine
        .process_invoice(inv, &InvoiceCategorizationMode::CustomDensity(density_evaluator))
        .await
        .expect("process invoice with custom density function");

    // Item categories remain fully controlled by LLM:
    for item in &processed.items {
        assert_eq!(
            item.product_category_name.as_deref(),
            Some("Chocolate"),
            "Items must be categorized by LLM"
        );
    }

    // Invoice category assigned by developer's custom density function:
    // Total $30 / Chocolate Spend $30 = Density 1.0 <= 1.1 with total > 20
    assert_eq!(
        processed.invoice_category.as_deref(),
        Some("High Density Confectionery Wholesale"),
        "Invoice category must be computed by developer's density function"
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

    // Check stored invoice in engine
    let updated_inv = &engine.invoices[0];
    let snickers = updated_inv
        .items
        .iter()
        .find(|i| i.item_name.contains("Snickers"))
        .unwrap();
    let ferrero = updated_inv
        .items
        .iter()
        .find(|i| i.item_name.contains("Ferrero"))
        .unwrap();

    // Snickers stays in standard Chocolate
    assert_eq!(snickers.product_category_name.as_deref(), Some("Chocolate"));
    // Ferrero updated to Luxury Chocolate
    assert_eq!(
        ferrero.product_category_name.as_deref(),
        Some("Luxury Chocolate")
    );
    assert_eq!(
        ferrero.product_category_id.as_deref(),
        Some("pcat_luxury_chocolate")
    );

    // Invoice category updated to reflect new luxury item category
    assert_eq!(
        updated_inv.invoice_category.as_deref(),
        Some("Luxury Luxury Chocolate")
    );

    // Verify purchase_categories DAG has Luxury Chocolate linked under Chocolate
    assert!(
        engine
            .purchase_dag
            .node(&"pcat_luxury_chocolate".to_string())
            .is_some(),
        "Luxury Chocolate node must exist in purchase_categories DAG"
    );
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
        date: "2026-06-15".into(),
        items: vec![FinanceInvoiceItem {
            item_id: "item_tshirt_walmart".into(),
            item_name: "Cotton Crewneck T-Shirt".into(),
            unit_price: 12.00,
            quantity: 2,
            total_price: 24.00,
            product_category_id: None,
            product_category_name: None,
            variant: Some(ProductVariant {
                variant_name: "Standard Pack".into(),
                units_count: 2,
                package_type: "pack".into(),
            }),
        }],
        total: 24.00,
        invoice_category: None,
    };

    let processed_walmart = engine
        .process_invoice(walmart_invoice, &InvoiceCategorizationMode::LlmDecided)
        .await
        .expect("process walmart invoice");

    // Item category at Walmart must be Everyday Casualwear
    let walmart_item = &processed_walmart.items[0];
    assert_eq!(
        walmart_item.product_category_name.as_deref(),
        Some("Everyday Casualwear"),
        "A t-shirt bought at Walmart must be inferred as Everyday Casualwear"
    );
    assert_eq!(
        processed_walmart.invoice_category.as_deref(),
        Some("Everyday Retail"),
        "Walmart invoice must be classified as Everyday Retail"
    );

    // 2. Ingest Prada Invoice with the SAME item name Cotton Crewneck T-Shirt ($650.00)
    let prada_invoice = FinanceInvoice {
        invoice_id: "INV-PRADA-001".into(),
        merchant_id: "m_prada".into(),
        date: "2026-06-16".into(),
        items: vec![FinanceInvoiceItem {
            item_id: "item_tshirt_prada".into(),
            item_name: "Cotton Crewneck T-Shirt".into(),
            unit_price: 650.00,
            quantity: 1,
            total_price: 650.00,
            product_category_id: None,
            product_category_name: None,
            variant: Some(ProductVariant {
                variant_name: "Single Piece".into(),
                units_count: 1,
                package_type: "single".into(),
            }),
        }],
        total: 650.00,
        invoice_category: None,
    };

    let processed_prada = engine
        .process_invoice(prada_invoice, &InvoiceCategorizationMode::LlmDecided)
        .await
        .expect("process prada invoice");

    // Item category at Prada must be Luxury Designer Fashion due to merchant context and price point!
    let prada_item = &processed_prada.items[0];
    assert_eq!(
        prada_item.product_category_name.as_deref(),
        Some("Luxury Designer Fashion"),
        "The same t-shirt bought at Prada must be inferred as Luxury Designer Fashion"
    );
    assert_eq!(
        processed_prada.invoice_category.as_deref(),
        Some("Luxury Designer Fashion"),
        "Prada invoice must be classified as Luxury Designer Fashion"
    );

    // Verify both category branches exist distinct in the purchase_categories DAG:
    assert!(engine.purchase_dag.node(&"pcat_everyday_casualwear".to_string()).is_some());
    assert!(engine.purchase_dag.node(&"pcat_luxury_designer_fashion".to_string()).is_some());
}
