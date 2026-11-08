//! Struct-level attributes: `#[diesel(table_name = ..)]`, `#[hierarchical(..)]` and doc comments.

use syn::parse::ParseStream;
use syn::{Attribute, Expr, ExprLit, Ident, Lit, LitStr, Meta, MetaNameValue};

/// What the struct-level attributes of a derived model say.
#[derive(Default, Debug)]
pub struct StructAttrs {
    pub table_name: Option<String>,
    pub id_col: Option<String>,
    pub parent_col: Option<String>,
    pub name_col: Option<String>,
    pub description: Option<String>,
}

impl StructAttrs {
    /// Reads every relevant attribute, in source order.
    pub fn parse(attrs: &[Attribute]) -> Self {
        let mut parsed = Self::default();
        for attr in attrs {
            parsed.apply(attr);
        }
        parsed
    }

    fn apply(&mut self, attr: &Attribute) {
        if attr.path().is_ident("diesel") {
            self.apply_diesel(attr);
        } else if attr.path().is_ident("hierarchical") {
            self.apply_hierarchical(attr);
        } else if attr.path().is_ident("doc") {
            self.apply_doc(attr);
        }
    }

    fn apply_diesel(&mut self, attr: &Attribute) {
        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("table_name") {
                self.table_name = table_name_value(meta.value()?);
            }
            Ok(())
        });
    }

    fn apply_hierarchical(&mut self, attr: &Attribute) {
        let _ = attr.parse_nested_meta(|meta| {
            if let Some(slot) = self.slot(&meta.path) {
                *slot = Some(meta.value()?.parse::<LitStr>()?.value());
            }
            Ok(())
        });
    }

    fn slot(&mut self, path: &syn::Path) -> Option<&mut Option<String>> {
        let key = path.get_ident()?.to_string();
        match key.as_str() {
            "table" => Some(&mut self.table_name),
            "id" => Some(&mut self.id_col),
            "parent" => Some(&mut self.parent_col),
            "name" => Some(&mut self.name_col),
            "description" => Some(&mut self.description),
            _ => None,
        }
    }

    /// The first non-empty doc line describes the table, unless `description` said it already.
    fn apply_doc(&mut self, attr: &Attribute) {
        if self.description.is_some() {
            return;
        }
        if let Meta::NameValue(MetaNameValue {
            value:
                Expr::Lit(ExprLit {
                    lit: Lit::Str(text),
                    ..
                }),
            ..
        }) = &attr.meta
        {
            let doc = text.value().trim().to_string();
            if !doc.is_empty() {
                self.description = Some(doc);
            }
        }
    }
}

/// `table_name = users` or `table_name = "users"`.
fn table_name_value(value: ParseStream) -> Option<String> {
    if let Ok(ident) = value.parse::<Ident>() {
        return Some(ident.to_string());
    }
    value.parse::<LitStr>().ok().map(|lit| lit.value())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use syn::{DeriveInput, parse_quote};

    fn parsed(input: DeriveInput) -> StructAttrs {
        StructAttrs::parse(&input.attrs)
    }

    #[rstest]
    #[case::ident(parse_quote!(#[diesel(table_name = users)] struct S;), Some("users"))]
    #[case::literal(parse_quote!(#[diesel(table_name = "people")] struct S;), Some("people"))]
    #[case::neither(parse_quote!(#[diesel(table_name = 5)] struct S;), None)]
    #[case::other_key(parse_quote!(#[diesel(primary_key(id))] struct S;), None)]
    fn diesel_table_name(#[case] input: DeriveInput, #[case] expected: Option<&str>) {
        assert_eq!(parsed(input).table_name.as_deref(), expected);
    }

    #[test]
    fn hierarchical_keys_fill_their_slots() {
        let attrs = parsed(parse_quote! {
            #[hierarchical(table = "t", id = "i", parent = "p", name = "n", description = "d", unknown = "x")]
            struct S;
        });
        assert_eq!(attrs.table_name.as_deref(), Some("t"));
        assert_eq!(attrs.id_col.as_deref(), Some("i"));
        assert_eq!(attrs.parent_col.as_deref(), Some("p"));
        assert_eq!(attrs.name_col.as_deref(), Some("n"));
        assert_eq!(attrs.description.as_deref(), Some("d"));
    }

    #[test]
    fn a_path_that_is_not_a_single_ident_is_ignored() {
        let attrs = parsed(parse_quote! { #[hierarchical(a::b = "x")] struct S; });
        assert!(attrs.table_name.is_none());
    }

    #[rstest]
    #[case::doc(parse_quote!(#[doc = "  Things  "] struct S;), Some("Things"))]
    #[case::blank_doc(parse_quote!(#[doc = "   "] struct S;), None)]
    #[case::not_a_string(parse_quote!(#[doc = 5] struct S;), None)]
    #[case::not_name_value(parse_quote!(#[doc(hidden)] struct S;), None)]
    #[case::attribute_wins(
        parse_quote!(#[hierarchical(description = "Explicit")] #[doc = "Doc"] struct S;),
        Some("Explicit")
    )]
    #[case::unrelated_attribute(parse_quote!(#[derive(Debug)] struct S;), None)]
    fn description(#[case] input: DeriveInput, #[case] expected: Option<&str>) {
        assert_eq!(parsed(input).description.as_deref(), expected);
    }
}
