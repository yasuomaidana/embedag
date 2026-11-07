//! Rust field types to Diesel SQL types.

use proc_macro2::TokenStream;
use quote::quote;

/// The Diesel SQL type of a field type; unknown types are bound as `Text`.
pub fn map_syn_type_to_diesel_sql_type(ty: &syn::Type) -> TokenStream {
    let text = quote!(#ty).to_string().replace(' ', "");
    if let Some(inner) = text.strip_prefix("Option<").and_then(option_inner) {
        if let Some(base) = base_sql_type(inner) {
            return quote!(diesel::sql_types::Nullable<#base>);
        }
    } else if let Some(base) = base_sql_type(&text) {
        return base;
    }
    quote!(diesel::sql_types::Text)
}

/// `String>` of `Option<String>` becomes `String`.
fn option_inner(rest: &str) -> Option<&str> {
    rest.split_once('>').map(|(inner, _)| inner)
}

fn base_sql_type(name: &str) -> Option<TokenStream> {
    Some(match name {
        "String" | "&str" => quote!(diesel::sql_types::Text),
        "i32" => quote!(diesel::sql_types::Integer),
        "i64" => quote!(diesel::sql_types::BigInt),
        "f64" => quote!(diesel::sql_types::Double),
        "bool" => quote!(diesel::sql_types::Bool),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use syn::{Type, parse_quote};

    #[rstest]
    #[case::string(parse_quote!(String), "diesel::sql_types::Text")]
    #[case::str_ref(parse_quote!(&str), "diesel::sql_types::Text")]
    #[case::int(parse_quote!(i32), "diesel::sql_types::Integer")]
    #[case::bigint(parse_quote!(i64), "diesel::sql_types::BigInt")]
    #[case::double(parse_quote!(f64), "diesel::sql_types::Double")]
    #[case::boolean(parse_quote!(bool), "diesel::sql_types::Bool")]
    #[case::opt_string(
        parse_quote!(Option<String>),
        "diesel::sql_types::Nullable<diesel::sql_types::Text>"
    )]
    #[case::opt_i32(
        parse_quote!(Option<i32>),
        "diesel::sql_types::Nullable<diesel::sql_types::Integer>"
    )]
    #[case::unknown(parse_quote!(Uuid), "diesel::sql_types::Text")]
    #[case::unknown_option(parse_quote!(Option<Uuid>), "diesel::sql_types::Text")]
    #[case::option_without_close(parse_quote!(Option), "diesel::sql_types::Text")]
    fn maps(#[case] ty: Type, #[case] expected: &str) {
        let got = map_syn_type_to_diesel_sql_type(&ty)
            .to_string()
            .replace(' ', "");
        assert_eq!(got, expected.replace(' ', ""));
    }
}
