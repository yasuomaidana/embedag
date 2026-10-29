//! Finance invoice categorization domain implementation.
//!
//! Subdivided into cohesive modules:
//! - [`dto`]: Input and output DTO structures (`FinanceInvoice`, `InvoiceItem`, etc.).
//! - [`nodes`]: Four hierarchical taxonomy graph nodes.
//! - [`temporal`]: Temporal context extraction and utilities.
//! - [`engine`]: Multi-DAG categorization orchestration engine.
//! - [`item_inference`]: Product intrinsic and contextual purchase category inference.
//! - [`user_actions`]: Dynamic user category additions, overrides, and reclassifications.

pub mod dto;
pub mod engine;
pub mod item_inference;
pub mod nodes;
pub mod temporal;
pub mod user_actions;

pub use dto::*;
pub use engine::*;
pub use nodes::*;
pub use temporal::*;
