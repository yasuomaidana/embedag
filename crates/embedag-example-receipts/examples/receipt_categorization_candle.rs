//! Example: Receipt categorization using pure-Rust Candle inference.
//!
//! Demonstrates:
//! 1. Defining only domain structures and their relationships (`InvoiceItem`, `ReceiptCategoryNode`).
//! 2. Automatic derive of table config, context, and type-safe CRUD operations.
//! 3. Passing field instructions context to the LLM via schemars attributes.
//! 4. Pipeline starts from an empty graph and builds taxonomy on the fly.
//! 5. Similar items reuse existing categories without creating duplicate nodes.

mod common;

use embedag_llm::provider::CandleProvider;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // In Candle, CandleProvider performs local CPU inference with the specified Hugging Face model
    common::run("Candle", CandleProvider::new("candle-smollm")).await
}
