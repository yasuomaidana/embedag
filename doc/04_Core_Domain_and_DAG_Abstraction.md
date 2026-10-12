# Core Domain & Directed Acyclic Graph (DAG) Abstraction

## 1. Executive Summary
The core domain abstraction layer of `embedag` defines the foundational contracts, types, and in-memory representation for converting arbitrary user-defined input structs into a Directed Acyclic Graph (DAG). 

Unlike strict trees that enforce a single-parent hierarchy, `embedag` supports full DAG topologies (where a single child node can have multiple inbound parents, such as a product variant belonging to multiple taxonomy categories). The core domain maintains zero-cost memory efficiency, type safety, and strict acyclic invariance without depending on external database engines or LLM network clients.

---

## 2. Core Traits & Trait Contracts

### 2.1 The Input Contract: `AsGraphContext`
The `AsGraphContext` trait is applied to user domain structs that serve as inputs to the graph generation pipeline. It defines how fields are projected into semantic text, deterministic continuous metrics, temporal features, and multimodal binary payloads.

```rust
use std::collections::HashMap;

/// Map of normalized scalar and continuous features.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FeatureMap {
    pub scalars: HashMap<String, f64>,
    pub cyclic_temporal: HashMap<String, [f64; 2]>, // [sin, cos]
    pub spatial_h3: HashMap<String, u64>,            // Discrete H3 cell index
}

/// Binary blob for multimodal assets (images, audio, documents).
#[derive(Debug, Clone, PartialEq)]
pub struct MultimodalBlob {
    pub name: String,
    pub mime_type: String,
    pub data: Vec<u8>,
}

/// The Input Contract: defines how a struct provides context to the graph engine.
pub trait AsGraphContext: Send + Sync + 'static {
    /// Extracts unstructured textual prompts or descriptions for LLM ingestion.
    fn text_context(&self) -> Option<String> {
        None
    }

    /// Normalizes scalar, temporal, or spatial features deterministically.
    fn scalar_features(&self) -> FeatureMap {
        FeatureMap::default()
    }

    /// Optional raw binary/multimodal payloads (e.g. JPEG photos, audio clips).
    fn multimodal_payloads(&self) -> Vec<MultimodalBlob> {
        Vec::new()
    }
}
```

### 2.2 The Output Contract: `NodePayload` & `GraphTarget`
The output contract represents nodes and edges within the target graph. It decouples the internal structure of domain entities from graph topology.

```rust
use std::hash::Hash;

/// Trait implemented by any struct representing a node's domain data.
pub trait NodePayload: Clone + Send + Sync + 'static {
    type Key: Eq + Hash + Clone + Send + Sync + std::fmt::Display;

    /// Primary unique business identifier (e.g. SKU, slug, UUID string).
    fn node_key(&self) -> Self::Key;

    /// Human-readable label for logging and graph rendering.
    fn display_label(&self) -> &str;
}

/// Edge payload capturing relationship metadata, confidence, and provenance.
#[derive(Debug, Clone, PartialEq)]
pub struct EdgePayload {
    pub relationship: String,
    pub confidence: f32, // 1.0 for EXTRACTED, 0.4..0.95 for INFERRED
    pub provenance: ProvenanceKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProvenanceKind {
    AlgorithmicExtracted, // Regex, deterministic rule, AST
    LlmInferred,          // Inferred by LLM reasoning (oxibonsai / cloud)
    UserOverride,         // Manually corrected by human
}

/// The Graph Storage / Mutation Contract.
pub trait GraphTarget<T: NodePayload> {
    type NodeId: Copy + Eq + Hash + Send + Sync;

    /// Inserts a node payload, returning its stable in-memory identifier.
    fn insert_node(&mut self, payload: T) -> Self::NodeId;

    /// Adds a directed edge between two existing nodes.
    fn add_edge(&mut self, from: Self::NodeId, to: Self::NodeId, payload: EdgePayload);

    /// Looks up a node by its business key.
    fn find_existing(&self, key: &T::Key) -> Option<Self::NodeId>;

    /// Retrieves an immutable reference to a node payload.
    fn get_payload(&self, id: Self::NodeId) -> Option<&T>;

    /// Validates whether adding an edge would violate the acyclic constraint.
    fn would_create_cycle(&self, from: Self::NodeId, to: Self::NodeId) -> bool;
}
```

### 2.3 The Model Classifier Contract: `GraphClassifier`
Abstracts how generative models or inference engines translate input context into structured mutation plans.

```rust
#[derive(Debug, Clone)]
pub enum MutationDirective<T: NodePayload> {
    AssignExisting {
        target_key: T::Key,
        relationship: String,
        confidence: f32,
    },
    InsertNode {
        payload: T,
        parents: Vec<(T::Key, String)>, // (parent_key, relationship)
        confidence: f32,
    },
    Unknown {
        unresolved_input: String,
        suggested_parent: Option<T::Key>,
    },
}

#[derive(Debug, Clone)]
pub struct GraphMutationPlan<T: NodePayload> {
    pub directives: Vec<MutationDirective<T>>,
}

#[async_trait::async_trait]
pub trait GraphClassifier<In, Out, Target>: Send + Sync
where
    In: AsGraphContext,
    Out: NodePayload,
    Target: GraphTarget<Out>,
{
    type Error: std::error::Error + Send + Sync + 'static;

    async fn infer_update(
        &self,
        input: &In,
        current_graph: &Target,
    ) -> Result<GraphMutationPlan<Out>, Self::Error>;
}
```

---

## 3. In-Memory DAG Architecture & Index Arenas

### 3.1 Transition from `indextree` to `petgraph`
Strict trees allocate nodes with a single `Option<NodeId>` for parent pointers. In enterprise taxonomies and knowledge graphs:
- A product ("Carlos V Cookies & Cream") can belong to multiple categories ("Chocolates" and "Ice Cream Toppings").
- A component can be referenced by multiple assemblies.

To eliminate entity duplication while avoiding pointer overhead (`Rc<RefCell<T>>`), `embedag` employs **Index-Based DAG Arenas** backed by `petgraph::graph::DiGraph<T, EdgePayload>`:
- **Zero-Cost Indexing**: Nodes and edges are referenced by 32-bit indices (`NodeIndex`, `EdgeIndex`).
- **Contiguous Cache Locality**: Payloads reside in flat continuous vectors, providing maximum CPU prefetching during traversals.
- **Thread Safety**: Flat index arenas implement `Send` and `Sync`, allowing parallel traversals via `rayon`.

### 3.2 In-Memory Cycle Detection Guardrail
Before any mutation plan is committed, `embedag` guarantees the Directed Acyclic invariance using Tarjan's Strongly Connected Components (SCC) or depth-first search cycle detection:

```rust
use petgraph::algo::is_cyclic_directed;
use petgraph::graph::DiGraph;

pub struct MemoryDag<T: NodePayload> {
    graph: DiGraph<T, EdgePayload>,
    key_index: std::collections::HashMap<T::Key, petgraph::graph::NodeIndex>,
}

impl<T: NodePayload> MemoryDag<T> {
    pub fn validate_acyclic(&self) -> Result<(), DagError> {
        if is_cyclic_directed(&self.graph) {
            return Err(DagError::CycleDetected);
        }
        Ok(())
    }
}
```

---

## 4. Lazy Node Evaluation: `LazyNodeRef<T>`
For massive graphs containing tens of thousands of nodes, loading the entire graph into memory upfront causes memory bloat. `embedag` provides `LazyNodeRef<T>`:
- Encapsulates a unique entity UUID or business key.
- Holds an optional in-memory `NodeId`.
- Exposes an asynchronous `.hydrate(&repo)` method that calls the `GraphRepository` to fetch only the requested node and its immediate k-hop neighborhood on-demand.

---

## 5. Declarative Derive Macros: `embedag-derive`
To minimize boilerplate, `embedag-derive` generates the trait implementations at compile time:

```rust
use embedag::{AsGraphContext, GraphTarget};

#[derive(AsGraphContext)]
pub struct InventoryItem {
    #[graph(text)]
    pub title: String,

    #[graph(text)]
    pub description: Option<String>,

    #[graph(scalar, normalize = "0.0..5000.0")]
    pub weight_grams: f64,

    #[graph(time, periodic = "day")]
    pub scan_time: chrono::DateTime<chrono::Utc>,

    #[graph(image, mime = "image/jpeg")]
    pub barcode_photo: Option<Vec<u8>>,
}

#[derive(GraphTarget, Clone)]
pub struct CategoryItem {
    #[node(key)]
    pub sku: String,

    #[node(label)]
    pub name: String,

    pub confidence: f32,
}
```
