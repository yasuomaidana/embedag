//! Macros for implementing the crate's traits without repeating their boilerplate.

/// Implements [`NodePayload`](crate::traits::NodePayload) for `$ty` with a `String` key.
///
/// Each accessor is a closure over the node: `key` and `name` return a reference into it, `level`
/// returns the entity level, and the optional `attributes` returns its JSON metadata.
///
/// ```
/// #[derive(Clone)]
/// struct Item { sku: String, title: String }
/// embedag_core::impl_node_payload!(
///     Item,
///     key = |n| &n.sku,
///     name = |n| &n.title,
///     level = |_| "Item"
/// );
/// ```
#[macro_export]
macro_rules! impl_node_payload {
    (
        $ty:ty,
        key = $key:expr,
        name = $name:expr,
        level = $level:expr
        $(, attributes = $attributes:expr)? $(,)?
    ) => {
        impl $crate::traits::NodePayload for $ty {
            type Key = String;

            fn key(&self) -> &Self::Key {
                let f: fn(&Self) -> &String = $key;
                f(self)
            }

            fn name(&self) -> &str {
                let f: fn(&Self) -> &str = $name;
                f(self)
            }

            fn entity_level(&self) -> &str {
                let f: fn(&Self) -> &str = $level;
                f(self)
            }

            $(
                fn attributes(&self) -> serde_json::Value {
                    let f: fn(&Self) -> serde_json::Value = $attributes;
                    f(self)
                }
            )?
        }
    };
}
