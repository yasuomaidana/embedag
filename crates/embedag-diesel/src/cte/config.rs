use serde::{Deserialize, Serialize};

/// Configuration describing an arbitrary hierarchical database table.
///
/// Enables developers with existing database migrations and domain models
/// to leverage `embedag`'s recursive CTE capabilities without forcing rigid schema adoption.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HierarchicalTableConfig {
    /// Name of the relational table (e.g., "categories", "taxonomies", "org_units").
    pub table_name: String,
    /// Column name representing the node identifier (e.g., "id" or "category_id").
    pub id_column: String,
    /// Column name representing the optional parent reference (e.g., "parent_id" or "parent_category_id").
    pub parent_column: String,
    /// Column name representing the human-readable entity name (e.g., "name" or "label").
    pub name_column: String,
    /// Optional semantic description or context instructions about the table to feed LLM prompts.
    pub description: Option<String>,
    /// Optional limit on the number of child branches traversed per node at each depth level.
    pub max_breadth_per_node: Option<i32>,
}

impl HierarchicalTableConfig {
    /// Creates a configuration for an adjacency list hierarchy table.
    pub fn new(
        table_name: impl Into<String>,
        id_column: impl Into<String>,
        parent_column: impl Into<String>,
        name_column: impl Into<String>,
    ) -> Self {
        Self {
            table_name: table_name.into(),
            id_column: id_column.into(),
            parent_column: parent_column.into(),
            name_column: name_column.into(),
            description: None,
            max_breadth_per_node: None,
        }
    }

    /// Attaches an optional semantic description of this table to feed LLM extraction prompts.
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Sets an upper bound on child branches traversed per node at each depth.
    pub fn with_max_breadth_per_node(mut self, breadth: i32) -> Self {
        self.max_breadth_per_node = Some(breadth);
        self
    }

    /// Configuration for embedag's default semantic nodes/edges table.
    pub fn default_semantic() -> Self {
        Self::new("semantic_nodes", "id", "parent_id", "name")
    }

    /// Formats this table configuration into structured context suitable for feeding LLM prompts.
    pub fn table_context(&self) -> String {
        let mut ctx = format!(
            "Table: {}\n- Key Column: {}\n- Parent Column: {}\n- Label Column: {}",
            self.table_name, self.id_column, self.parent_column, self.name_column
        );
        if let Some(desc) = &self.description {
            ctx.push_str(&format!("\n- Description: {desc}"));
        }
        ctx
    }
}

/// Represents an item in a traversed hierarchy lineage with its relative depth.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HierarchyItem {
    /// Unique identifier of the node.
    pub id: String,
    /// Relative traversal depth (1 = immediate parent/child, 2 = grandparent, etc.).
    pub depth: i32,
    /// Name or label of the node.
    pub name: String,
}

/// Trait implemented by domain models that represent hierarchical graph nodes.
pub trait HierarchicalNode {
    /// Type of the node identifier.
    type Id: Clone + ToString + std::fmt::Display;

    /// Unique identifier of the current node.
    fn node_id(&self) -> Self::Id;

    /// Parent node identifier, or `None` if this is a root node.
    fn parent_id(&self) -> Option<Self::Id>;

    /// Human-readable label or category name.
    fn node_name(&self) -> &str;
}
