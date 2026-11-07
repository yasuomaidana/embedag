//! Implementation of `#[derive(AsGraphContext)]`.
//!
//! Traceability: `doc/06_Procedural_Derive_Macros.md`.

use proc_macro2::TokenStream;
use quote::quote;
use syn::spanned::Spanned;
use syn::{Data, DeriveInput, Field, Fields, Ident};

#[derive(Default)]
struct ParsedFields {
    key_field: Option<Ident>,
    text_fields: Vec<Ident>,
    scalar_fields: Vec<(Ident, String)>,
    time_field: Option<Ident>,
    geo_field: Option<Ident>,
    tag_fields: Vec<(Ident, String)>,
    blob_fields: Vec<Ident>,
    tokens_field: Option<Ident>,
}

/// Records `field` in `slot`, which only one field may fill through `#[graph(attr)]`.
fn set_once(
    slot: &mut Option<Ident>,
    field: &Ident,
    meta: &syn::meta::ParseNestedMeta,
    attr: &str,
) -> syn::Result<()> {
    if slot.is_some() {
        return Err(syn::Error::new(
            meta.path.span(),
            format!("Only one field may be annotated with #[graph({attr})]"),
        ));
    }
    *slot = Some(field.clone());
    Ok(())
}

/// `field` with its name: the `= "name"` of the attribute, else the field's own name.
fn named(meta: &syn::meta::ParseNestedMeta, field: &Ident) -> syn::Result<(Ident, String)> {
    let mut custom_name = None;
    if meta.input.peek(syn::Token![=]) {
        let lit: syn::LitStr = meta.value()?.parse()?;
        custom_name = Some(lit.value());
    }
    Ok((
        field.clone(),
        custom_name.unwrap_or_else(|| field.to_string()),
    ))
}

fn parse_single_attr(
    meta: &syn::meta::ParseNestedMeta,
    field_ident: &Ident,
    parsed: &mut ParsedFields,
) -> syn::Result<()> {
    if meta.path.is_ident("key") {
        set_once(&mut parsed.key_field, field_ident, meta, "key")?;
    } else if meta.path.is_ident("text") {
        parsed.text_fields.push(field_ident.clone());
    } else if meta.path.is_ident("scalar") {
        parsed.scalar_fields.push(named(meta, field_ident)?);
    } else if meta.path.is_ident("time") || meta.path.is_ident("temporal") {
        set_once(&mut parsed.time_field, field_ident, meta, "time")?;
    } else if meta.path.is_ident("geo") || meta.path.is_ident("geospatial") {
        set_once(&mut parsed.geo_field, field_ident, meta, "geo")?;
    } else if meta.path.is_ident("tag") {
        parsed.tag_fields.push(named(meta, field_ident)?);
    } else if meta.path.is_ident("blob") || meta.path.is_ident("blobs") {
        parsed.blob_fields.push(field_ident.clone());
    } else if meta.path.is_ident("tokens") || meta.path.is_ident("sparse") {
        parsed.tokens_field = Some(field_ident.clone());
    } else {
        return Err(syn::Error::new(
            meta.path.span(),
            format!(
                "unrecognized #[graph(...)] attribute: `{}`",
                meta.path
                    .get_ident()
                    .map(|i| i.to_string())
                    .unwrap_or_default()
            ),
        ));
    }
    Ok(())
}

fn parse_field(field: &Field, parsed: &mut ParsedFields) -> syn::Result<()> {
    let field_ident = field.ident.as_ref().unwrap();
    for attr in &field.attrs {
        if attr.path().is_ident("graph") {
            attr.parse_nested_meta(|meta| parse_single_attr(&meta, field_ident, parsed))?;
        }
    }
    Ok(())
}

fn generate_text_repr(text_fields: &[Ident]) -> TokenStream {
    if text_fields.len() == 1 {
        let single = &text_fields[0];
        quote! { self.#single.to_string() }
    } else {
        let formats: Vec<_> = text_fields
            .iter()
            .map(|f| quote! { self.#f.to_string() })
            .collect();
        quote! { vec![#(#formats),*].join(" ") }
    }
}

pub fn derive_as_graph_context_impl(input: DeriveInput) -> syn::Result<TokenStream> {
    let struct_span = input.span();
    let struct_ident = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let core_crate = quote!(::embedag_core);

    let fields = match &input.data {
        Data::Struct(s) => match &s.fields {
            Fields::Named(f) => &f.named,
            _ => {
                return Err(syn::Error::new(
                    struct_span,
                    "#[derive(AsGraphContext)] requires named fields",
                ));
            }
        },
        _ => {
            return Err(syn::Error::new(
                struct_span,
                "#[derive(AsGraphContext)] only supports structs",
            ));
        }
    };

    let mut parsed = ParsedFields::default();
    for field in fields {
        parse_field(field, &mut parsed)?;
    }

    let key_ident = parsed.key_field.ok_or_else(|| {
        syn::Error::new(
            struct_span,
            "#[derive(AsGraphContext)] requires exactly one #[graph(key)]",
        )
    })?;

    if parsed.text_fields.is_empty() {
        return Err(syn::Error::new(
            struct_span,
            "#[derive(AsGraphContext)] requires at least one #[graph(text)]",
        ));
    }

    let text_repr_impl = generate_text_repr(&parsed.text_fields);
    let scalar_inserts = parsed
        .scalar_fields
        .iter()
        .map(|(f, name)| quote! { map.insert(#name.to_string(), self.#f as f64); });
    let tag_inserts = parsed
        .tag_fields
        .iter()
        .map(|(f, name)| quote! { map.insert(#name.to_string(), self.#f.to_string()); });
    let time_impl = parsed
        .time_field
        .map(|f| quote! { Some(self.#f) })
        .unwrap_or_else(|| quote! { None });
    let geo_impl = parsed
        .geo_field
        .map(|f| quote! { Some(self.#f) })
        .unwrap_or_else(|| quote! { None });
    let tokens_impl = parsed
        .tokens_field
        .map(|f| quote! { self.#f.clone() })
        .unwrap_or_else(|| quote! { ::std::vec::Vec::new() });
    let blob_impl = if parsed.blob_fields.is_empty() {
        quote! { ::std::vec::Vec::new() }
    } else {
        let inserts = parsed
            .blob_fields
            .iter()
            .map(|b| quote! { self.#b.clone() });
        quote! { vec![#(#inserts),*] }
    };

    Ok(quote! {
        impl #impl_generics #core_crate::traits::AsGraphContext for #struct_ident #ty_generics #where_clause {
            fn context_key(&self) -> ::std::string::String { self.#key_ident.to_string() }
            fn text_repr(&self) -> ::std::string::String { #text_repr_impl }
            fn sparse_lexical_tokens(&self) -> ::std::vec::Vec<(u32, f32)> { #tokens_impl }
            fn scalar_features(&self) -> #core_crate::model::FeatureMap {
                let mut map = ::std::collections::HashMap::new();
                #(#scalar_inserts)*
                map
            }
            fn timestamp_utc(&self) -> ::std::option::Option<::chrono::DateTime<::chrono::Utc>> { #time_impl }
            fn geographic_coordinates(&self) -> ::std::option::Option<(f64, f64)> { #geo_impl }
            fn categorical_tags(&self) -> #core_crate::model::TagMap {
                let mut map = ::std::collections::HashMap::new();
                #(#tag_inserts)*
                map
            }
            fn multimodal_blobs(&self) -> ::std::vec::Vec<#core_crate::model::MultimodalBlob> { #blob_impl }
        }
    })
}
