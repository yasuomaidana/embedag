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
                ));
            }
        },
        _ => {
            return Err(Error::new_spanned(
                struct_name,
                "HierarchicalModel can only be derived on structs",
            ));
        }
    };

    let mut detected_id = None;
    let mut detected_parent = None;
    let mut detected_name = None;
    let mut has_entity_level = false;

    let mut field_names = Vec::new();
    let mut field_idents = Vec::new();
    let mut field_sql_types = Vec::new();

    for field in fields {
        let field_ident = match &field.ident {
            Some(i) => i,
            None => continue,
        };
        let field_name = field_ident.to_string();

        if field_name == "entity_level" {
            has_entity_level = true;
        }

        for attr in &field.attrs {
            if attr.path().is_ident("hierarchical") {
                let _ = attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("id") {
                        detected_id = Some(field_name.clone());
                    } else if meta.path.is_ident("parent") {
                        detected_parent = Some(field_name.clone());
                    } else if meta.path.is_ident("name") {
                        detected_name = Some(field_name.clone());
                    }
                    Ok(())
                });
            }
        }

        if detected_id.is_none()
            && (field_name == "id"
                || (field_name.ends_with("_id") && !field_name.contains("parent")))
        {
            detected_id = Some(field_name.clone());
        }
        if detected_parent.is_none() && (field_name == "parent_id" || field_name.contains("parent"))
        {
            detected_parent = Some(field_name.clone());
        }
        if detected_name.is_none()
            && (field_name == "name"
                || field_name == "title"
                || field_name == "label"
                || field_name.ends_with("_name"))
        {
            detected_name = Some(field_name.clone());
        }

        let sql_type = map_syn_type_to_diesel_sql_type(&field.ty);
        field_names.push(field_name);
        field_idents.push(field_ident);
        field_sql_types.push(sql_type);
    }

    let final_id_col = id_col.or(detected_id).unwrap_or_else(|| "id".to_string());
    let final_parent_col = parent_col
        .or(detected_parent)
        .unwrap_or_else(|| "parent_id".to_string());
    let final_name_col = name_col
        .or(detected_name)
        .unwrap_or_else(|| "name".to_string());

    let id_ident = Ident::new(&final_id_col, proc_macro2::Span::call_site());
    let parent_ident = Ident::new(&final_parent_col, proc_macro2::Span::call_site());
    let name_ident = Ident::new(&final_name_col, proc_macro2::Span::call_site());

    let is_parent_option = fields
        .iter()
        .find(|f| {
            f.ident
                .as_ref()
                .map(|i| i == &parent_ident)
                .unwrap_or(false)
        })
        .map(|f| match &f.ty {
            syn::Type::Path(p) => p
                .path
                .segments
                .last()
                .map(|s| s.ident == "Option")
                .unwrap_or(false),
            _ => false,
        })
        .unwrap_or(true);

    let parent_extract = if is_parent_option {
        quote! { self.#parent_ident.clone() }
    } else {
        quote! { Some(self.#parent_ident.to_string()) }
    };

    let entity_level_extract = if has_entity_level {
        quote! { &self.entity_level }
    } else {
        quote! { Self::table_name() }
    };

    let table_desc_tokens = match &table_description {
        Some(desc) => quote! { Some(#desc) },
        None => quote! { None },
    };

    // Build SQL CRUD statements
    let cols_joined = field_names.join(", ");
    let sqlite_placeholders = vec!["?"; field_names.len()].join(", ");
    let sqlite_insert_sql =
        format!("INSERT INTO {table_name} ({cols_joined}) VALUES ({sqlite_placeholders});");

    let pg_placeholders: Vec<String> = (1..=field_names.len()).map(|i| format!("${i}")).collect();
    let pg_placeholders_joined = pg_placeholders.join(", ");
    let pg_insert_sql =
        format!("INSERT INTO {table_name} ({cols_joined}) VALUES ({pg_placeholders_joined});");

    let sqlite_delete_sql = format!("DELETE FROM {table_name} WHERE {final_id_col} = ?;");
    let pg_delete_sql = format!("DELETE FROM {table_name} WHERE {final_id_col} = $1;");

    let non_id_indices: Vec<usize> = field_names
        .iter()
        .enumerate()
        .filter(|(_, name)| *name != &final_id_col)
        .map(|(i, _)| i)
        .collect();

    let non_id_idents: Vec<&Ident> = non_id_indices.iter().map(|&i| field_idents[i]).collect();
    let non_id_sql_types: Vec<&TokenStream> = non_id_indices
        .iter()
        .map(|&i| &field_sql_types[i])
        .collect();

    let sqlite_update_sets = non_id_indices
        .iter()
        .map(|&i| format!("{} = ?", field_names[i]))
        .collect::<Vec<_>>()
        .join(", ");
    let sqlite_update_sql =
        format!("UPDATE {table_name} SET {sqlite_update_sets} WHERE {final_id_col} = ?;");

    let pg_update_sets = non_id_indices
        .iter()
        .enumerate()
        .map(|(seq, &i)| format!("{} = ${}", field_names[i], seq + 1))
        .collect::<Vec<_>>()
        .join(", ");
    let pg_update_sql = format!(
        "UPDATE {table_name} SET {pg_update_sets} WHERE {final_id_col} = ${};",
        non_id_indices.len() + 1
    );

    let columns = Columns {
        idents: &field_idents,
        sql_types: &field_sql_types,
        non_id_idents: &non_id_idents,
        non_id_sql_types: &non_id_sql_types,
        id_ident: &id_ident,
    };
    let sqlite_methods = crud_methods(
        &Backend {
            label: "SQLite",
            suffix: "sqlite",
            conn: quote!(diesel::SqliteConnection),
            sql: [&sqlite_insert_sql, &sqlite_delete_sql, &sqlite_update_sql],
        },
        &columns,
    );
    let pg_methods = crud_methods(
        &Backend {
            label: "PostgreSQL",
            suffix: "pg",
            conn: quote!(diesel::pg::PgConnection),
            sql: [&pg_insert_sql, &pg_delete_sql, &pg_update_sql],
        },
        &columns,
    );

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

        impl embedag_diesel::cte::HierarchicalNode for #struct_name {
            type Id = String;

            fn node_id(&self) -> Self::Id {
                self.#id_ident.clone()
            }

            fn parent_id(&self) -> Option<Self::Id> {
                #parent_extract
            }

            fn node_name(&self) -> &str {
                &self.#name_ident
            }
        }

        impl embedag_core::traits::NodePayload for #struct_name {
            type Key = String;

            fn key(&self) -> &Self::Key {
                &self.#id_ident
            }

            fn name(&self) -> &str {
                &self.#name_ident
            }

            fn entity_level(&self) -> &str {
                #entity_level_extract
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

            #sqlite_methods

            #pg_methods

            /// Type-safe Diesel async parameterized insert for PostgreSQL via `diesel-async`.
            pub async fn insert_pg_async(&self, conn: &mut diesel_async::AsyncPgConnection) -> diesel::QueryResult<usize> {
                let query = diesel::sql_query(#pg_insert_sql)
                    #( .bind::<#field_sql_types, _>(&self.#field_idents) )*;
                diesel_async::RunQueryDsl::execute(query, conn).await
            }

            /// Type-safe Diesel async batch insert for PostgreSQL via `diesel-async`.
            pub async fn insert_batch_pg_async(records: &[Self], conn: &mut diesel_async::AsyncPgConnection) -> diesel::QueryResult<usize> {
                let mut count = 0;
                for rec in records {
                    count += rec.insert_pg_async(&mut *conn).await?;
                }
                Ok(count)
            }

            /// Type-safe Diesel async parameterized delete by ID for PostgreSQL via `diesel-async`.
            pub async fn delete_by_id_pg_async(conn: &mut diesel_async::AsyncPgConnection, id: &str) -> diesel::QueryResult<usize> {
                let query = diesel::sql_query(#pg_delete_sql)
                    .bind::<diesel::sql_types::Text, _>(id);
                diesel_async::RunQueryDsl::execute(query, conn).await
            }

            /// Type-safe Diesel async parameterized update by ID for PostgreSQL via `diesel-async`.
            pub async fn update_pg_async(&self, conn: &mut diesel_async::AsyncPgConnection) -> diesel::QueryResult<usize> {
                let query = diesel::sql_query(#pg_update_sql)
                    #( .bind::<#non_id_sql_types, _>(&self.#non_id_idents) )*
                    .bind::<diesel::sql_types::Text, _>(&self.#id_ident);
                diesel_async::RunQueryDsl::execute(query, conn).await
            }
        }
    };

    Ok(expanded)
}

/// One database backend whose CRUD methods the derive generates.
struct Backend<'a> {
    /// Name used in the generated docs.
    label: &'a str,
    /// Suffix of the generated method names.
    suffix: &'a str,
    /// The Diesel connection type.
    conn: TokenStream,
    /// The insert, delete-by-id and update-by-id statements.
    sql: [&'a str; 3],
}

/// The columns of the model, as the generated binds need them.
struct Columns<'a> {
    idents: &'a [&'a Ident],
    sql_types: &'a [TokenStream],
    non_id_idents: &'a [&'a Ident],
    non_id_sql_types: &'a [&'a TokenStream],
    id_ident: &'a Ident,
}

/// The insert, batch insert, delete and update methods of one backend.
fn crud_methods(backend: &Backend, columns: &Columns) -> TokenStream {
    let Backend {
        label,
        suffix,
        conn: conn_ty,
        sql: [insert_sql, delete_sql, update_sql],
    } = backend;
    let Columns {
        idents,
        sql_types,
        non_id_idents,
        non_id_sql_types,
        id_ident,
    } = columns;
    let insert = quote::format_ident!("insert_{}", suffix);
    let insert_batch = quote::format_ident!("insert_batch_{}", suffix);
    let delete = quote::format_ident!("delete_by_id_{}", suffix);
    let update = quote::format_ident!("update_{}", suffix);
    let insert_doc = format!(" Type-safe Diesel parameterized insert for {label}.");
    let batch_doc = format!(" Type-safe Diesel batch insert for {label} within a transaction.");
    let delete_doc = format!(" Type-safe Diesel parameterized delete by ID for {label}.");
    let update_doc = format!(" Type-safe Diesel parameterized update by ID for {label}.");
    quote! {
        #[doc = #insert_doc]
        pub fn #insert(&self, conn: &mut #conn_ty) -> diesel::QueryResult<usize> {
            let query = diesel::sql_query(#insert_sql)
                #( .bind::<#sql_types, _>(&self.#idents) )*;
            diesel::RunQueryDsl::execute(query, conn)
        }

        #[doc = #batch_doc]
        pub fn #insert_batch(records: &[Self], conn: &mut #conn_ty) -> diesel::QueryResult<usize> {
            use diesel::Connection;
            conn.transaction(|conn| {
                let mut count = 0;
                for rec in records {
                    count += rec.#insert(conn)?;
                }
                Ok(count)
            })
        }

        #[doc = #delete_doc]
        pub fn #delete(conn: &mut #conn_ty, id: &str) -> diesel::QueryResult<usize> {
            let query = diesel::sql_query(#delete_sql)
                .bind::<diesel::sql_types::Text, _>(id);
            diesel::RunQueryDsl::execute(query, conn)
        }

        #[doc = #update_doc]
        pub fn #update(&self, conn: &mut #conn_ty) -> diesel::QueryResult<usize> {
            let query = diesel::sql_query(#update_sql)
                #( .bind::<#non_id_sql_types, _>(&self.#non_id_idents) )*
                .bind::<diesel::sql_types::Text, _>(&self.#id_ident);
            diesel::RunQueryDsl::execute(query, conn)
        }
    }
}

// TODO: Review and ensure all types are mapped correctly.
fn map_syn_type_to_diesel_sql_type(ty: &syn::Type) -> TokenStream {
    let type_str = quote!(#ty).to_string().replace(' ', "");
    if type_str == "String" || type_str == "&str" {
        quote!(diesel::sql_types::Text)
    } else if type_str.starts_with("Option<String>") || type_str.starts_with("Option<&str>") {
        quote!(diesel::sql_types::Nullable<diesel::sql_types::Text>)
    } else if type_str == "i32" {
        quote!(diesel::sql_types::Integer)
    } else if type_str.starts_with("Option<i32>") {
        quote!(diesel::sql_types::Nullable<diesel::sql_types::Integer>)
    } else if type_str == "i64" {
        quote!(diesel::sql_types::BigInt)
    } else if type_str.starts_with("Option<i64>") {
        quote!(diesel::sql_types::Nullable<diesel::sql_types::BigInt>)
    } else if type_str == "f64" {
        quote!(diesel::sql_types::Double)
    } else if type_str.starts_with("Option<f64>") {
        quote!(diesel::sql_types::Nullable<diesel::sql_types::Double>)
    } else if type_str == "bool" {
        quote!(diesel::sql_types::Bool)
    } else if type_str.starts_with("Option<bool>") {
        quote!(diesel::sql_types::Nullable<diesel::sql_types::Bool>)
    } else {
        quote!(diesel::sql_types::Text)
    }
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
