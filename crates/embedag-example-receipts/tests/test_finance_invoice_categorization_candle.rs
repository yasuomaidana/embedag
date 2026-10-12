use embedag_example_receipts::finance::*;
use embedag_llm::provider::CandleProvider;
use std::collections::HashMap;
use std::sync::Arc;

#[tokio::test]
async fn test_finance_invoice_categorization_candle_llm_decided_and_time_dependency() {
    let provider = CandleProvider::new("candle-smollm");
    let mut engine = FinanceCategorizationEngine::new(provider);

    // 1. Register merchant with commercial tags in merchant_categories DAG
    let merchant = FinanceMerchant {
        merchant_id: "m_metro_market".into(),
        name: "Metro Supermarket & Spirits".into(),
        tags: vec!["grocery".into(), "retail".into(), "organic".into()],
        category_id: "mcat_supermarket".into(),
        category_name: "Supermarket".into(),
    };
    engine.register_merchant(merchant);

    assert_eq!(engine.merchant_dag.node_count(), 1);
    assert_eq!(engine.purchase_dag.node_count(), 0);

    // 2. Ingest regular purchase invoice (non-holiday) with variants
    let inv1 = FinanceInvoice {
        invoice_id: "INV-CANDLE-01".into(),
        merchant_id: "m_metro_market".into(),
        date: "2026-05-18".into(), // Spring day (non-holiday)
        items: vec![
            FinanceInvoiceItem {
                item_id: "item_snickers_pack".into(),
                item_name: "Snickers Bar 50g".into(),
                unit_price: 1.25,
                quantity: 4,
                total_price: 5.00,
                product_category_id: None,
                product_category_name: None,
                variant: Some(ProductVariant {
                    variant_name: "4-pack".into(),
                    units_count: 4,
                    package_type: "pack".into(),
                }),
            },
            FinanceInvoiceItem {
                item_id: "item_ferrero_3".into(),
                item_name: "Ferrero Rocher (3-pack)".into(),
                unit_price: 2.75,
                quantity: 2,
                total_price: 5.50,
                product_category_id: None,
                product_category_name: None,
                variant: Some(ProductVariant {
                    variant_name: "3-pack".into(),
                    units_count: 3,
                    package_type: "pack".into(),
                }),
            },
        ],
        total: 10.50,
        invoice_category: None,
    };

    let processed1 = engine
        .process_invoice(inv1, &InvoiceCategorizationMode::LlmDecided)
        .await
        .expect("process invoice 1 candle");

    assert!(processed1.invoice_category.is_some());
    assert_eq!(
        processed1.invoice_category.as_deref(),
        Some("Everyday Retail")
    );

    // Both Ferrero and Snickers categorized into Chocolate / Confectionery by Candle
    for item in &processed1.items {
        assert!(item.product_category_name.is_some());
        let cat = item.product_category_name.as_ref().unwrap();
        assert!(
            cat.contains("Chocolate") || cat.contains("Groceries"),
            "Item category must be derived by Candle LLM, got: {}",
            cat
        );
    }

    // 3. Ingest holiday purchase invoice (December 25 - Christmas Day)
    // Holiday nature is inferred from date rather than pre-existing on invoice
    let inv2 = FinanceInvoice {
        invoice_id: "INV-CANDLE-02".into(),
        merchant_id: "m_metro_market".into(),
        date: "2026-12-25".into(), // Christmas Day holiday purchase
        items: vec![FinanceInvoiceItem {
            item_id: "item_ferrero_box16".into(),
            item_name: "Ferrero Rocher (Box of 16 Festive Edition)".into(),
            unit_price: 14.99,
            quantity: 2,
            total_price: 29.98,
            product_category_id: None,
            product_category_name: None,
            variant: Some(ProductVariant {
                variant_name: "Box of 16".into(),
                units_count: 16,
                package_type: "box".into(),
            }),
        }],
        total: 29.98,
        invoice_category: None,
    };

    let processed2 = engine
        .process_invoice(inv2, &InvoiceCategorizationMode::LlmDecided)
        .await
        .expect("process holiday invoice 2 candle");

    assert_eq!(
        processed2.invoice_category.as_deref(),
        Some("Holiday Gifting"),
        "Candle LLM must infer Christmas invoice as Holiday Gifting"
    );
}

#[tokio::test]
async fn test_finance_invoice_categorization_candle_custom_density_and_user_overrides() {
    let provider = CandleProvider::new("candle-smollm");
    let mut engine = FinanceCategorizationEngine::new(provider);

    // 1. Register merchant initially as General Retail
    let merchant = FinanceMerchant {
        merchant_id: "m_city_mart".into(),
        name: "City Mart Downtown".into(),
        tags: vec!["urban".into(), "retail".into(), "drinks".into()],
        category_id: "mcat_retail".into(),
        category_name: "General Retail".into(),
    };
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
        |inv: &FinanceInvoice, category_spends: &HashMap<String, f64>| -> String {
            let choco_spend = category_spends.get("Chocolate").copied().unwrap_or(0.0);
            let density = if choco_spend > 0.0 {
                inv.total / choco_spend
            } else {
                1.0
            };

            if density <= 1.05 && inv.total >= 40.0 {
                "Bulk Pure Confectionery".to_string()
            } else if density <= 1.3 {
                "Predominantly Sweet Basket".to_string()
            } else {
                "General Sundries".to_string()
            }
        },
    );

    let inv = FinanceInvoice {
        invoice_id: "INV-CANDLE-03".into(),
        merchant_id: "m_city_mart".into(),
        date: "2026-07-04".into(),
        items: vec![
            FinanceInvoiceItem {
                item_id: "item_snickers_bar".into(),
                item_name: "Snickers Bar 50g".into(),
                unit_price: 2.00,
                quantity: 5,
                total_price: 10.00,
                product_category_id: None,
                product_category_name: None,
                variant: Some(ProductVariant {
                    variant_name: "Single Bar 50g".into(),
                    units_count: 1,
                    package_type: "single".into(),
                }),
            },
            FinanceInvoiceItem {
                item_id: "item_ferrero_box16".into(),
                item_name: "Ferrero Rocher (Box of 16)".into(),
                unit_price: 15.00,
                quantity: 2,
                total_price: 30.00,
                product_category_id: None,
                product_category_name: None,
                variant: Some(ProductVariant {
                    variant_name: "Box of 16".into(),
                    units_count: 16,
                    package_type: "box".into(),
                }),
            },
        ],
        total: 40.00,
        invoice_category: None,
    };

    let processed = engine
        .process_invoice(inv, &InvoiceCategorizationMode::CustomDensity(density_evaluator))
        .await
        .expect("process invoice candle density");

    // Item categories controlled by Candle:
    for item in &processed.items {
        assert_eq!(item.product_category_name.as_deref(), Some("Chocolate"));
    }

    // Invoice category assigned by custom density function (40.0 / 40.0 = 1.0 <= 1.05 and total >= 40):
    assert_eq!(
        processed.invoice_category.as_deref(),
        Some("Bulk Pure Confectionery")
    );

    // 4. Condition 3: User adds item category "Luxury Chocolate"
    // Ferrero Rocher is luxury chocolate, Snickers is standard chocolate.
    let reclassified = engine.user_add_item_category_and_reclassify(
        "Luxury Chocolate",
        "Chocolate",
        |item| item.item_name.contains("Ferrero"),
    );
    assert_eq!(reclassified, 1);

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

    assert_eq!(snickers.product_category_name.as_deref(), Some("Chocolate"));
    assert_eq!(
        ferrero.product_category_name.as_deref(),
        Some("Luxury Chocolate")
    );
    assert_eq!(
        updated_inv.invoice_category.as_deref(),
        Some("Luxury Luxury Chocolate")
    );
}

#[tokio::test]
async fn test_finance_invoice_candle_walmart_vs_prada_tshirt_categorization() {
    let provider = CandleProvider::new("candle-smollm");
    let mut engine = FinanceCategorizationEngine::new(provider);

    // Register Walmart: Discount Department Store
    let walmart = FinanceMerchant {
        merchant_id: "m_walmart".into(),
        name: "Walmart Supercenter".into(),
        tags: vec!["discount".into(), "mass_market".into(), "retail".into()],
        category_id: "mcat_discount_store".into(),
        category_name: "Discount Department Store".into(),
    };
    engine.register_merchant(walmart);

    // Register Prada: Luxury Fashion Boutique
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
        invoice_id: "INV-CANDLE-WALMART-01".into(),
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
        .expect("process candle walmart invoice");

    let walmart_item = &processed_walmart.items[0];
    assert_eq!(
        walmart_item.product_category_name.as_deref(),
        Some("Everyday Casualwear"),
        "A t-shirt bought at Walmart must be inferred as Everyday Casualwear by Candle"
    );
    assert_eq!(
        processed_walmart.invoice_category.as_deref(),
        Some("Everyday Retail"),
        "Walmart invoice must be classified as Everyday Retail by Candle"
    );

    // 2. Ingest Prada Invoice with the SAME item name Cotton Crewneck T-Shirt ($650.00)
    let prada_invoice = FinanceInvoice {
        invoice_id: "INV-CANDLE-PRADA-01".into(),
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
        .expect("process candle prada invoice");

    let prada_item = &processed_prada.items[0];
    assert_eq!(
        prada_item.product_category_name.as_deref(),
        Some("Luxury Designer Fashion"),
        "The same t-shirt bought at Prada must be inferred as Luxury Designer Fashion by Candle"
    );
    assert_eq!(
        processed_prada.invoice_category.as_deref(),
        Some("Luxury Designer Fashion"),
        "Prada invoice must be classified as Luxury Designer Fashion by Candle"
    );

    // Verify distinct purchase_categories DAG branches
    assert!(engine.purchase_dag.node(&"pcat_everyday_casualwear".to_string()).is_some());
    assert!(engine.purchase_dag.node(&"pcat_luxury_designer_fashion".to_string()).is_some());
}
