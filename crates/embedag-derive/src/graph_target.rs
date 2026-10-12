//! Implementation of `#[derive(GraphTarget)]`.
//!
//! Traceability: `doc/01_Development_Plan.md` (Ticket 1.3.2) and
//! `doc/04_Core_Domain_and_Entity_Contracts.md` §2.3.

use proc_macro2::TokenStream;
use quote::quote;
use syn::spanned::Spanned;
use syn::{Data, DeriveInput, Fields, Ident, Variant};

#[derive(Clone, Copy, PartialEq, Eq)]
enum TargetVariantKind {
    Exact,
    Fuzzy,
    Child,
    MultiChild,
    Root,
    Ambiguous,
}

fn detect_kind(variant: &Variant) -> Option<TargetVariantKind> {
    for attr in &variant.attrs {
        if attr.path().is_ident("target") {
            let mut detected = None;
            let _ = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("exact") {
                    detected = Some(TargetVariantKind::Exact);
                } else if meta.path.is_ident("fuzzy") {
                    detected = Some(TargetVariantKind::Fuzzy);
                } else if meta.path.is_ident("child") {
                    detected = Some(TargetVariantKind::Child);
                } else if meta.path.is_ident("multi_child") {
                    detected = Some(TargetVariantKind::MultiChild);
                } else if meta.path.is_ident("root") {
                    detected = Some(TargetVariantKind::Root);
                } else if meta.path.is_ident("ambiguous") {
                    detected = Some(TargetVariantKind::Ambiguous);
                }
                Ok(())
            });
            if detected.is_some() {
                return detected;
            }
        }
    }

    let name = variant.ident.to_string().to_lowercase();
    if name.contains("exact") {
        Some(TargetVariantKind::Exact)
    } else if name.contains("fuzzy") {
        Some(TargetVariantKind::Fuzzy)
    } else if name.contains("multi") {
        Some(TargetVariantKind::MultiChild)
    } else if name.contains("child") {
        Some(TargetVariantKind::Child)
    } else if name.contains("root") {
        Some(TargetVariantKind::Root)
    } else if name.contains("ambiguous") {
        Some(TargetVariantKind::Ambiguous)
    } else {
        None
    }
}

fn build_into_arm(
    variant: &Variant,
    kind: TargetVariantKind,
    enum_ident: &Ident,
    core: &TokenStream,
) -> TokenStream {
    let var_ident = &variant.ident;
    match kind {
        TargetVariantKind::Exact => quote! {
            #enum_ident::#var_ident(k) => #core::model::GraphTarget::ExactMatch(k),
        },
        TargetVariantKind::Fuzzy => quote! {
            #enum_ident::#var_ident { key, similarity } => #core::model::GraphTarget::FuzzyCandidate { key, similarity },
        },
        TargetVariantKind::Child => quote! {
            #enum_ident::#var_ident { parent_key, relationship } => #core::model::GraphTarget::NewChildOf { parent_key, relationship },
        },
        TargetVariantKind::MultiChild => quote! {
            #enum_ident::#var_ident { parent_keys } => #core::model::GraphTarget::NewMultiParentChild { parent_keys },
        },
        TargetVariantKind::Root => quote! {
            #enum_ident::#var_ident => #core::model::GraphTarget::NewRoot,
        },
        TargetVariantKind::Ambiguous => quote! {
            #enum_ident::#var_ident(candidates) => #core::model::GraphTarget::Ambiguous(candidates),
        },
    }
}

fn build_from_arm(
    variant: &Variant,
    kind: TargetVariantKind,
    enum_ident: &Ident,
    core: &TokenStream,
) -> TokenStream {
    let var_ident = &variant.ident;
    match kind {
        TargetVariantKind::Exact => quote! {
            #core::model::GraphTarget::ExactMatch(k) => ::std::result::Result::Ok(#enum_ident::#var_ident(k)),
        },
        TargetVariantKind::Fuzzy => quote! {
            #core::model::GraphTarget::FuzzyCandidate { key, similarity } => ::std::result::Result::Ok(#enum_ident::#var_ident { key, similarity }),
        },
        TargetVariantKind::Child => quote! {
            #core::model::GraphTarget::NewChildOf { parent_key, relationship } => ::std::result::Result::Ok(#enum_ident::#var_ident { parent_key, relationship }),
        },
        TargetVariantKind::MultiChild => quote! {
            #core::model::GraphTarget::NewMultiParentChild { parent_keys } => ::std::result::Result::Ok(#enum_ident::#var_ident { parent_keys }),
        },
        TargetVariantKind::Root => quote! {
            #core::model::GraphTarget::NewRoot => ::std::result::Result::Ok(#enum_ident::#var_ident),
        },
        TargetVariantKind::Ambiguous => quote! {
            #core::model::GraphTarget::Ambiguous(candidates) => ::std::result::Result::Ok(#enum_ident::#var_ident(candidates)),
        },
    }
}

pub fn derive_graph_target_impl(input: DeriveInput) -> syn::Result<TokenStream> {
    let struct_span = input.span();
    let ident = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let core = quote!(::embedag_core);

    let data_enum = match &input.data {
        Data::Enum(e) => e,
        Data::Struct(s) => match &s.fields {
            Fields::Named(f) => {
                let target_field = f.named.iter().find(|field| {
                    field.attrs.iter().any(|a| a.path().is_ident("target"))
                        || field.ident.as_ref().map(|i| i == "target").unwrap_or(false)
                });
                if let Some(field) = target_field {
                    let f_ident = field.ident.as_ref().unwrap();
                    return Ok(quote! {
                        impl #impl_generics #ident #ty_generics #where_clause {
                            pub fn into_graph_target(self) -> #core::model::GraphTarget<String> {
                                self.#f_ident
                            }
                        }
                    });
                }
                return Err(syn::Error::new(
                    struct_span,
                    "#[derive(GraphTarget)] on struct requires a `target` field or #[target] attribute",
                ));
            }
            _ => {
                return Err(syn::Error::new(
                    struct_span,
                    "#[derive(GraphTarget)] requires an enum or struct with named fields",
                ));
            }
        },
        _ => {
            return Err(syn::Error::new(
                struct_span,
                "#[derive(GraphTarget)] only supports enums or structs",
            ));
        }
    };

    let mut into_arms = Vec::new();
    let mut from_arms = Vec::new();

    for variant in &data_enum.variants {
        if let Some(kind) = detect_kind(variant) {
            into_arms.push(build_into_arm(variant, kind, ident, &core));
            from_arms.push(build_from_arm(variant, kind, ident, &core));
        }
    }

    if into_arms.is_empty() {
        return Err(syn::Error::new(
            struct_span,
            "#[derive(GraphTarget)] could not detect any recognizable GraphTarget variants (e.g. ExactMatch, FuzzyCandidate, NewChildOf, NewRoot, Ambiguous)",
        ));
    }

    Ok(quote! {
        impl #impl_generics #ident #ty_generics #where_clause {
            pub fn into_graph_target<__K>(self) -> #core::model::GraphTarget<__K>
            where
                Self: ::std::convert::Into<#core::model::GraphTarget<__K>>,
            {
                ::std::convert::Into::into(self)
            }

            pub fn from_graph_target<__K>(target: #core::model::GraphTarget<__K>) -> ::std::result::Result<Self, &'static str>
            where
                Self: ::std::convert::TryFrom<#core::model::GraphTarget<__K>, Error = &'static str>,
            {
                ::std::convert::TryFrom::try_from(target)
            }
        }

        impl #impl_generics ::std::convert::From<#ident #ty_generics> for #core::model::GraphTarget<String> #where_clause {
            fn from(val: #ident #ty_generics) -> Self {
                match val {
                    #(#into_arms)*
                }
            }
        }

        impl #impl_generics ::std::convert::TryFrom<#core::model::GraphTarget<String>> for #ident #ty_generics #where_clause {
            type Error = &'static str;

            fn try_from(target: #core::model::GraphTarget<String>) -> ::std::result::Result<Self, Self::Error> {
                match target {
                    #(#from_arms)*
                    _ => ::std::result::Result::Err("unmatched GraphTarget variant for domain enum"),
                }
            }
        }
    })
}
