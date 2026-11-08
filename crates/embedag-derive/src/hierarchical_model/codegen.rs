//! Token generation for the derived impls.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::Ident;

use super::fields::FieldScan;
use super::sql::{Statements, non_id_indices};

/// What the generated impls need to know about the model.
pub struct Model<'a> {
    pub struct_name: &'a Ident,
    pub table: String,
    pub id_col: String,
    pub parent_col: String,
    pub name_col: String,
    pub description: Option<String>,
    pub id: Ident,
    pub parent: Ident,
    pub name: Ident,
    pub parent_is_option: bool,
    pub has_entity_level: bool,
}

/// One database backend whose CRUD methods the derive generates.
pub struct Backend<'a> {
    /// Name used in the generated docs.
    pub label: &'a str,
    /// Suffix of the generated method names.
    pub suffix: &'a str,
    /// The Diesel connection type.
    pub conn: TokenStream,
    /// The insert, delete-by-id and update-by-id statements.
    pub sql: &'a Statements,
}

/// The columns of the model, as the generated binds need them.
pub struct Columns<'a> {
    idents: Vec<&'a Ident>,
    sql_types: Vec<&'a TokenStream>,
    non_id_idents: Vec<&'a Ident>,
    non_id_sql_types: Vec<&'a TokenStream>,
    id_ident: &'a Ident,
}

impl<'a> Columns<'a> {
    pub fn new(scan: &'a FieldScan, id_col: &str, id_ident: &'a Ident) -> Self {
        let non_id = non_id_indices(&scan.names, id_col);
        Self {
            idents: scan.idents.clone(),
            sql_types: scan.sql_types.iter().collect(),
            non_id_idents: non_id.iter().map(|&i| scan.idents[i]).collect(),
            non_id_sql_types: non_id.iter().map(|&i| &scan.sql_types[i]).collect(),
            id_ident,
        }
    }
}

/// `impl HierarchicalModel`: the table and column names.
pub fn model_impl(m: &Model) -> TokenStream {
    let Model {
        struct_name,
        table,
        id_col,
        parent_col,
        name_col,
        ..
    } = m;
    let description = match &m.description {
        Some(desc) => quote! { Some(#desc) },
        None => quote! { None },
    };
    quote! {
        impl embedag_diesel::cte::HierarchicalModel for #struct_name {
            fn table_name() -> &'static str {
                #table
            }

            fn id_column() -> &'static str {
                #id_col
            }

            fn parent_column() -> &'static str {
                #parent_col
            }

            fn name_column() -> &'static str {
                #name_col
            }

            fn table_description() -> Option<&'static str> {
                #description
            }
        }
    }
}

/// `impl HierarchicalNode`: id, parent and name accessors.
pub fn node_impl(m: &Model) -> TokenStream {
    let Model {
        struct_name,
        id,
        parent,
        name,
        ..
    } = m;
    let parent_extract = if m.parent_is_option {
        quote! { self.#parent.clone() }
    } else {
        quote! { Some(self.#parent.to_string()) }
    };
    quote! {
        impl embedag_diesel::cte::HierarchicalNode for #struct_name {
            type Id = String;

            fn node_id(&self) -> Self::Id {
                self.#id.clone()
            }

            fn parent_id(&self) -> Option<Self::Id> {
                #parent_extract
            }

            fn node_name(&self) -> &str {
                &self.#name
            }
        }
    }
}

/// `impl NodePayload`: key, name and entity level.
pub fn payload_impl(m: &Model) -> TokenStream {
    let Model {
        struct_name,
        id,
        name,
        ..
    } = m;
    let entity_level = if m.has_entity_level {
        quote! { &self.entity_level }
    } else {
        quote! { Self::table_name() }
    };
    quote! {
        impl embedag_core::traits::NodePayload for #struct_name {
            type Key = String;

            fn key(&self) -> &Self::Key {
                &self.#id
            }

            fn name(&self) -> &str {
                &self.#name
            }

            fn entity_level(&self) -> &str {
                #entity_level
            }
        }
    }
}

/// The inherent impl: table helpers, sync CRUD per backend and the async PostgreSQL CRUD.
pub fn inherent_impl(
    m: &Model,
    columns: &Columns,
    sqlite: &Statements,
    pg: &Statements,
) -> TokenStream {
    let struct_name = m.struct_name;
    let sqlite_methods = crud_methods(
        &Backend {
            label: "SQLite",
            suffix: "sqlite",
            conn: quote!(diesel::SqliteConnection),
            sql: sqlite,
        },
        columns,
    );
    let pg_methods = crud_methods(
        &Backend {
            label: "PostgreSQL",
            suffix: "pg",
            conn: quote!(diesel::pg::PgConnection),
            sql: pg,
        },
        columns,
    );
    let pg_async = pg_async_methods(columns, pg);
    quote! {
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

            #pg_async
        }
    }
}

/// The insert, batch insert, delete and update methods of one backend.
fn crud_methods(backend: &Backend, columns: &Columns) -> TokenStream {
    let Backend {
        label,
        suffix,
        conn: conn_ty,
        sql:
            Statements {
                insert: insert_sql,
                delete: delete_sql,
                update: update_sql,
            },
    } = backend;
    let Columns {
        idents,
        sql_types,
        non_id_idents,
        non_id_sql_types,
        id_ident,
    } = columns;
    let insert = format_ident!("insert_{}", suffix);
    let insert_batch = format_ident!("insert_batch_{}", suffix);
    let delete = format_ident!("delete_by_id_{}", suffix);
    let update = format_ident!("update_{}", suffix);
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

/// The `diesel-async` PostgreSQL insert, batch insert, delete and update methods.
fn pg_async_methods(columns: &Columns, pg: &Statements) -> TokenStream {
    let Columns {
        idents,
        sql_types,
        non_id_idents,
        non_id_sql_types,
        id_ident,
    } = columns;
    let Statements {
        insert: pg_insert_sql,
        delete: pg_delete_sql,
        update: pg_update_sql,
    } = pg;
    quote! {
        /// Type-safe Diesel async parameterized insert for PostgreSQL via `diesel-async`.
        pub async fn insert_pg_async(&self, conn: &mut diesel_async::AsyncPgConnection) -> diesel::QueryResult<usize> {
            let query = diesel::sql_query(#pg_insert_sql)
                #( .bind::<#sql_types, _>(&self.#idents) )*;
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
}
