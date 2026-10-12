use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Error, Fields, Ident, LitStr, Result};

pub fn derive_hierarchical_model_impl(input: DeriveInput) -> Result<TokenStream> {
    let struct_name = &input.ident;

    let mut table_name = None;
    let mut id_col = None;
    let mut parent_col = None;
    let mut name_col = None;
    let mut table_description = None;

    // Check struct-level attributes
    for attr in &input.attrs {
        if attr.path().is_ident("diesel") {
            let _ = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("table_name") {
                    let value = meta.value()?;
                    if let Ok(ident) = value.parse::<Ident>() {
                        table_name = Some(ident.to_string());
                    } else if let Ok(lit) = value.parse::<LitStr>() {
                        table_name = Some(lit.value());
                    }
                }
                Ok(())
            });
        } else if attr.path().is_ident("hierarchical") {
            let _ = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("table") {
                    let value: LitStr = meta.value()?.parse()?;
                    table_name = Some(value.value());
                } else if meta.path.is_ident("id") {
                    let value: LitStr = meta.value()?.parse()?;
                    id_col = Some(value.value());
                } else if meta.path.is_ident("parent") {
                    let value: LitStr = meta.value()?.parse()?;
                    parent_col = Some(value.value());
                } else if meta.path.is_ident("name") {
                    let value: LitStr = meta.value()?.parse()?;
                    name_col = Some(value.value());
                } else if meta.path.is_ident("description") {
                    let value: LitStr = meta.value()?.parse()?;
                    table_description = Some(value.value());
                }
                Ok(())
            });
        } else if attr.path().is_ident("doc")
            && table_description.is_none()
            && let syn::Meta::NameValue(syn::MetaNameValue {
                value:
                    syn::Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Str(lit_str),
                        ..
                    }),
                ..
            }) = &attr.meta
        {
            let doc_text = lit_str.value().trim().to_string();
            if !doc_text.is_empty() {
                table_description = Some(doc_text);
            }
        }
    }

    // Default table name: snake_case of struct_name
    let table_name = table_name.unwrap_or_else(|| to_snake_case(&struct_name.to_string()));

    // Check field-level attributes
    let fields = match &input.data {
        Data::Struct(data_struct) => match &data_struct.fields {
            Fields::Named(fields_named) => &fields_named.named,
            _ => {
                return Err(Error::new_spanned(
                    struct_name,
                    "HierarchicalModel can only be derived on structs with named fields",
                ))
            }
        },
        _ => {
            return Err(Error::new_spanned(
                struct_name,
                "HierarchicalModel can only be derived on structs",
            ))
        }
    };

    let mut detected_id = None;
    let mut detected_parent = None;
    let mut detected_name = None;

    for field in fields {
        let field_ident = match &field.ident {
            Some(i) => i.to_string(),
            None => continue,
        };

        for attr in &field.attrs {
            if attr.path().is_ident("hierarchical") {
                let _ = attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("id") {
                        detected_id = Some(field_ident.clone());
                    } else if meta.path.is_ident("parent") {
                        detected_parent = Some(field_ident.clone());
                    } else if meta.path.is_ident("name") {
                        detected_name = Some(field_ident.clone());
                    }
                    Ok(())
                });
            }
        }

        if detected_id.is_none()
            && (field_ident == "id"
                || (field_ident.ends_with("_id") && !field_ident.contains("parent")))
        {
            detected_id = Some(field_ident.clone());
        }
        if detected_parent.is_none()
            && (field_ident == "parent_id" || field_ident.contains("parent"))
        {
            detected_parent = Some(field_ident.clone());
        }
        if detected_name.is_none()
            && (field_ident == "name"
                || field_ident == "title"
                || field_ident == "label"
                || field_ident.ends_with("_name"))
        {
            detected_name = Some(field_ident.clone());
        }
    }

    let final_id_col = id_col.or(detected_id).unwrap_or_else(|| "id".to_string());
    let final_parent_col = parent_col
        .or(detected_parent)
        .unwrap_or_else(|| "parent_id".to_string());
    let final_name_col = name_col
        .or(detected_name)
        .unwrap_or_else(|| "name".to_string());

    let table_desc_tokens = match &table_description {
        Some(desc) => quote! { Some(#desc) },
        None => quote! { None },
    };

    let expanded = quote! {
        impl embedag_diesel::cte::HierarchicalModel for #struct_name {
            fn table_name() -> &'static str {
                #table_name
            }

            fn id_column() -> &'static str {
                #final_id_col
            }

            fn parent_column() -> &'static str {
                #final_parent_col
            }

            fn name_column() -> &'static str {
                #final_name_col
            }

            fn table_description() -> Option<&'static str> {
                #table_desc_tokens
            }
        }

        impl #struct_name {
            /// Returns the [`embedag_diesel::cte::HierarchicalTableConfig`] auto-derived by macro.
            pub fn table_config() -> embedag_diesel::cte::HierarchicalTableConfig {
                <Self as embedag_diesel::cte::HierarchicalModel>::table_config()
            }

            /// Formats this model's table context for feeding LLM extraction prompts.
            pub fn table_context() -> String {
                <Self as embedag_diesel::cte::HierarchicalModel>::table_context()
            }
        }
    };

    Ok(expanded)
}

fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            for lc in c.to_lowercase() {
                result.push(lc);
            }
        } else {
            result.push(c);
        }
    }
    result
}
