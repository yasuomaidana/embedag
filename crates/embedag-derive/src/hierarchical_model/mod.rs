//! `#[derive(HierarchicalModel)]`: reads the attributes and fields, then generates the impls.

mod attrs;
mod codegen;
mod fields;
mod sql;
mod types;

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{DeriveInput, Ident, Result};

use attrs::StructAttrs;
use codegen::{Columns, Model};
use fields::{FieldScan, named_fields, parent_is_option};
use sql::{Dialect, statements};

pub fn derive_hierarchical_model_impl(input: DeriveInput) -> Result<TokenStream> {
    let attrs = StructAttrs::parse(&input.attrs);
    let fields = named_fields(&input)?;
    let scan = FieldScan::scan(fields);
    let model = resolve_model(&input, attrs, &scan, fields);

    let sqlite = statements(Dialect::Sqlite, &model.table, &scan.names, &model.id_col);
    let pg = statements(Dialect::Postgres, &model.table, &scan.names, &model.id_col);
    let columns = Columns::new(&scan, &model.id_col, &model.id);

    let model_impl = codegen::model_impl(&model);
    let node_impl = codegen::node_impl(&model);
    let payload_impl = codegen::payload_impl(&model);
    let inherent_impl = codegen::inherent_impl(&model, &columns, &sqlite, &pg);
    Ok(quote! {
        #model_impl
        #node_impl
        #payload_impl
        #inherent_impl
    })
}

/// Combines the explicit attributes, the detected fields and the defaults into the final model.
fn resolve_model<'a>(
    input: &'a DeriveInput,
    attrs: StructAttrs,
    scan: &FieldScan,
    fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
) -> Model<'a> {
    let struct_name = &input.ident;
    let id_col = attrs
        .id_col
        .or(scan.id.clone())
        .unwrap_or_else(|| "id".into());
    let parent_col = attrs
        .parent_col
        .or(scan.parent.clone())
        .unwrap_or_else(|| "parent_id".into());
    let name_col = attrs
        .name_col
        .or(scan.name.clone())
        .unwrap_or_else(|| "name".into());
    let ident = |col: &str| Ident::new(col, Span::call_site());
    let parent = ident(&parent_col);
    Model {
        struct_name,
        table: attrs
            .table_name
            .unwrap_or_else(|| to_snake_case(&struct_name.to_string())),
        description: attrs.description,
        id: ident(&id_col),
        parent_is_option: parent_is_option(fields, &parent),
        parent,
        name: ident(&name_col),
        id_col,
        parent_col,
        name_col,
        has_entity_level: scan.has_entity_level,
    }
}

fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            result.push('_');
        }
        result.extend(c.to_lowercase());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use syn::parse_quote;

    fn expand(input: DeriveInput) -> String {
        derive_hierarchical_model_impl(input)
            .expect("derive")
            .to_string()
    }

    #[rstest]
    #[case("Category", "category")]
    #[case("ProductCategory", "product_category")]
    #[case("already_snake", "already_snake")]
    fn snake_case(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(to_snake_case(input), expected);
    }

    #[test]
    fn defaults_without_any_attribute() {
        let out = expand(parse_quote!(
            struct Thing {
                value: String,
            }
        ));
        assert!(out.contains("\"thing\""));
        assert!(out.contains("\"id\""));
        assert!(out.contains("\"parent_id\""));
        assert!(out.contains("None"));
        assert!(out.contains("Self :: table_name ()"));
    }

    #[test]
    fn attributes_and_fields_drive_the_output() {
        let out = expand(parse_quote! {
            /// Product tree
            #[diesel(table_name = categories)]
            struct Category {
                entity_level: String,
                category_id: String,
                parent: String,
                title: String,
            }
        });
        assert!(out.contains("\"categories\""));
        assert!(out.contains("\"category_id\""));
        assert!(out.contains("Some (\"Product tree\")"));
        assert!(out.contains("& self . entity_level"));
        assert!(out.contains("Some (self . parent . to_string ())"));
        assert!(out.contains("INSERT INTO categories (entity_level, category_id, parent, title)"));
        assert!(out.contains("UPDATE categories SET entity_level = $1, parent = $2, title = $3 WHERE category_id = $4;"));
    }

    #[test]
    fn invalid_shapes_are_errors() {
        assert!(
            derive_hierarchical_model_impl(parse_quote!(
                enum E {
                    A,
                }
            ))
            .is_err()
        );
    }
}
