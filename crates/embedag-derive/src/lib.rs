//! Procedural derive macros for `embedag`.\n//!\n//! Provides `#[derive(AsGraphContext)]`, `#[derive(GraphTarget)]`, and `#[derive(HierarchicalModel)]`.

use proc_macro::TokenStream;
use syn::{parse_macro_input, DeriveInput};

mod as_graph_context;
mod graph_target;
mod hierarchical_model;

/// Derives [`AsGraphContext`](embedag_core::traits::AsGraphContext) for a domain struct.
#[proc_macro_derive(AsGraphContext, attributes(graph))]
pub fn derive_as_graph_context(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    as_graph_context::derive_as_graph_context_impl(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Derives conversion helpers to and from [`GraphTarget`](embedag_core::model::GraphTarget).
#[proc_macro_derive(GraphTarget, attributes(target))]
pub fn derive_graph_target(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    graph_target::derive_graph_target_impl(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Derives [`HierarchicalModel`](embedag_diesel::cte::HierarchicalModel) and type-safe recursive
/// CTE query generators for a Diesel domain struct.
///
/// # Struct Attributes
/// - `#[diesel(table_name = categories)]` or `#[hierarchical(table = "categories")]`: Relational table name (defaults to snake_case of struct).
/// - `#[hierarchical(id = "id")]`: Identifier column name (defaults to field `id` or `*_id`).
/// - `#[hierarchical(parent = "parent_id")]`: Parent reference column name (defaults to field `parent_id` or `*parent*`).
/// - `#[hierarchical(name = "name")]`: Display name/label column name (defaults to field `name` or `label`/`title`).
///
/// # Field Attributes
/// - `#[hierarchical(id)]`: Explicitly flags this field as the node ID.
/// - `#[hierarchical(parent)]`: Explicitly flags this field as the parent ID reference.
/// - `#[hierarchical(name)]`: Explicitly flags this field as the node name/label.
#[proc_macro_derive(HierarchicalModel, attributes(hierarchical, diesel))]
pub fn derive_hierarchical_model(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    hierarchical_model::derive_hierarchical_model_impl(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
