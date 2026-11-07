//! Field scan: which field is the id, the parent and the name, and each field's SQL type.

use proc_macro2::TokenStream;
use syn::punctuated::Punctuated;
use syn::token::Comma;
use syn::{Attribute, Data, DeriveInput, Error, Field, Fields, Ident, Result};

use super::types::map_syn_type_to_diesel_sql_type;

/// Everything the scan learned about the named fields of the struct.
#[derive(Default)]
pub struct FieldScan<'a> {
    pub names: Vec<String>,
    pub idents: Vec<&'a Ident>,
    pub sql_types: Vec<TokenStream>,
    pub id: Option<String>,
    pub parent: Option<String>,
    pub name: Option<String>,
    pub has_entity_level: bool,
}

/// The named fields of a struct, or an error for any other shape.
pub fn named_fields(input: &DeriveInput) -> Result<&Punctuated<Field, Comma>> {
    let struct_name = &input.ident;
    let Data::Struct(data) = &input.data else {
        return Err(Error::new_spanned(
            struct_name,
            "HierarchicalModel can only be derived on structs",
        ));
    };
    match &data.fields {
        Fields::Named(named) => Ok(&named.named),
        _ => Err(Error::new_spanned(
            struct_name,
            "HierarchicalModel can only be derived on structs with named fields",
        )),
    }
}

impl<'a> FieldScan<'a> {
    pub fn scan(fields: &'a Punctuated<Field, Comma>) -> Self {
        let mut scan = Self::default();
        for field in fields {
            if let Some(ident) = &field.ident {
                scan.visit(field, ident);
            }
        }
        scan
    }

    fn visit(&mut self, field: &'a Field, ident: &'a Ident) {
        let name = ident.to_string();
        self.has_entity_level |= name == "entity_level";
        self.apply_explicit_roles(&field.attrs, &name);
        self.apply_naming_conventions(&name);
        self.sql_types
            .push(map_syn_type_to_diesel_sql_type(&field.ty));
        self.names.push(name);
        self.idents.push(ident);
    }

    /// `#[hierarchical(id)]`, `(parent)` and `(name)` on the field always win.
    fn apply_explicit_roles(&mut self, attrs: &[Attribute], field_name: &str) {
        for attr in attrs.iter().filter(|a| a.path().is_ident("hierarchical")) {
            let _ = attr.parse_nested_meta(|meta| {
                if let Some(slot) = self.role_slot(&meta.path) {
                    *slot = Some(field_name.to_string());
                }
                Ok(())
            });
        }
    }

    fn role_slot(&mut self, path: &syn::Path) -> Option<&mut Option<String>> {
        if path.is_ident("id") {
            Some(&mut self.id)
        } else if path.is_ident("parent") {
            Some(&mut self.parent)
        } else if path.is_ident("name") {
            Some(&mut self.name)
        } else {
            None
        }
    }

    /// Fills the roles nothing claimed yet from the field's name.
    fn apply_naming_conventions(&mut self, name: &str) {
        if self.id.is_none() && is_id_like(name) {
            self.id = Some(name.to_string());
        }
        if self.parent.is_none() && is_parent_like(name) {
            self.parent = Some(name.to_string());
        }
        if self.name.is_none() && is_name_like(name) {
            self.name = Some(name.to_string());
        }
    }
}

fn is_id_like(name: &str) -> bool {
    name == "id" || (name.ends_with("_id") && !name.contains("parent"))
}

fn is_parent_like(name: &str) -> bool {
    name == "parent_id" || name.contains("parent")
}

fn is_name_like(name: &str) -> bool {
    matches!(name, "name" | "title" | "label") || name.ends_with("_name")
}

/// Whether the field called `parent` is an `Option`; a missing field counts as one.
pub fn parent_is_option(fields: &Punctuated<Field, Comma>, parent: &Ident) -> bool {
    fields
        .iter()
        .find(|f| f.ident.as_ref() == Some(parent))
        .is_none_or(|f| is_option_type(&f.ty))
}

fn is_option_type(ty: &syn::Type) -> bool {
    matches!(ty, syn::Type::Path(p) if p.path.segments.last().is_some_and(|s| s.ident == "Option"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use syn::parse_quote;

    fn fields_of(input: &DeriveInput) -> &Punctuated<Field, Comma> {
        named_fields(input).expect("named fields")
    }

    #[rstest]
    #[case::id("id", true)]
    #[case::suffix("order_id", true)]
    #[case::parent_suffix("parent_id", false)]
    #[case::other("title", false)]
    fn id_like(#[case] name: &str, #[case] expected: bool) {
        assert_eq!(is_id_like(name), expected);
    }

    #[rstest]
    #[case::parent_id("parent_id", true)]
    #[case::contains("my_parent", true)]
    #[case::other("id", false)]
    fn parent_like(#[case] name: &str, #[case] expected: bool) {
        assert_eq!(is_parent_like(name), expected);
    }

    #[rstest]
    #[case::name("name", true)]
    #[case::title("title", true)]
    #[case::label("label", true)]
    #[case::suffix("full_name", true)]
    #[case::other("id", false)]
    fn name_like(#[case] name: &str, #[case] expected: bool) {
        assert_eq!(is_name_like(name), expected);
    }

    #[test]
    fn conventions_pick_the_roles() {
        let input: DeriveInput = parse_quote!(
            struct S {
                id: String,
                parent_id: Option<String>,
                name: String,
                entity_level: String,
            }
        );
        let scan = FieldScan::scan(fields_of(&input));
        assert_eq!(scan.id.as_deref(), Some("id"));
        assert_eq!(scan.parent.as_deref(), Some("parent_id"));
        assert_eq!(scan.name.as_deref(), Some("name"));
        assert!(scan.has_entity_level);
        assert_eq!(scan.names.len(), 4);
        assert_eq!(scan.idents.len(), 4);
        assert_eq!(scan.sql_types.len(), 4);
    }

    #[test]
    fn explicit_attributes_beat_conventions() {
        let input: DeriveInput = parse_quote! {
            struct S {
                id: String,
                #[hierarchical(id)] key: String,
                #[hierarchical(parent)] up: String,
                #[hierarchical(name)] caption: String,
                #[hierarchical(other)] extra: String,
            }
        };
        let scan = FieldScan::scan(fields_of(&input));
        assert_eq!(scan.id.as_deref(), Some("key"));
        assert_eq!(scan.parent.as_deref(), Some("up"));
        assert_eq!(scan.name.as_deref(), Some("caption"));
        assert!(!scan.has_entity_level);
    }

    #[test]
    fn tuple_struct_fields_have_no_idents() {
        let input: DeriveInput = parse_quote!(
            struct S(String);
        );
        assert!(named_fields(&input).is_err());
    }

    #[test]
    fn enums_are_rejected() {
        let input: DeriveInput = parse_quote!(
            enum E {
                A,
            }
        );
        let err = named_fields(&input).unwrap_err();
        assert!(err.to_string().contains("only be derived on structs"));
    }

    #[test]
    fn unnamed_field_is_skipped_by_the_scan() {
        let tuple: DeriveInput = parse_quote!(
            struct T(String);
        );
        let named: DeriveInput = parse_quote!(
            struct N {
                id: String,
            }
        );
        let Data::Struct(data) = &tuple.data else {
            unreachable!("parsed as a struct")
        };
        let mut fields: Punctuated<Field, Comma> = data.fields.iter().cloned().collect();
        fields.extend(fields_of(&named).iter().cloned());
        let scan = FieldScan::scan(&fields);
        assert_eq!(scan.names, vec!["id".to_string()]);
    }

    #[rstest]
    #[case::option(parse_quote!(struct S { p: Option<String> }), true)]
    #[case::plain(parse_quote!(struct S { p: String }), false)]
    #[case::reference(parse_quote!(struct S { p: &'static str }), false)]
    #[case::missing(parse_quote!(struct S { other: String }), true)]
    fn parent_option(#[case] input: DeriveInput, #[case] expected: bool) {
        let parent: Ident = parse_quote!(p);
        assert_eq!(parent_is_option(fields_of(&input), &parent), expected);
    }
}
