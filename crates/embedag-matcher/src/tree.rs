//! Language-agnostic syntax tree representation and structural subtree hashing.

/// A language-agnostic syntax tree node that `tree-sitter` or `go/ast` output can be converted into.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tree {
    /// Syntactic node kind (e.g., `"function_item"`, `"binary_expression"`, `"identifier"`).
    pub kind: String,
    /// Optional lexical label or identifier text, ignored during structural hashing.
    pub label: Option<String>,
    /// Ordered child syntax subtrees.
    pub children: Vec<Tree>,
}

impl Tree {
    /// Creates a new [`Tree`] node with the given `kind`, optional `label`, and `children`.
    #[must_use]
    pub fn new(kind: impl Into<String>, label: Option<String>, children: Vec<Tree>) -> Self {
        Self {
            kind: kind.into(),
            label,
            children,
        }
    }

    /// Creates a branch [`Tree`] node with `label: None` and the given `children`.
    #[must_use]
    pub fn branch(kind: impl Into<String>, children: Vec<Tree>) -> Self {
        Self {
            kind: kind.into(),
            label: None,
            children,
        }
    }

    /// Creates a leaf [`Tree`] node with a `label` and no children.
    #[must_use]
    pub fn leaf(kind: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            label: Some(label.into()),
            children: Vec::new(),
        }
    }

    /// Returns the total number of nodes in this subtree (including `self`).
    #[must_use]
    pub fn size(&self) -> usize {
        1 + self.children.iter().map(Self::size).sum::<usize>()
    }

    /// Computes `(structural_hash, subtree_size)` for every subtree with `size >= min_size`.
    ///
    /// Structural hashes depend only on `kind` and ordered child structural hashes, ignoring `label`.
    /// Each hash is the first 8 bytes of the BLAKE3 digest interpreted as a little-endian `u64`.
    #[must_use]
    pub fn subtree_hashes(&self, min_size: usize) -> Vec<(u64, usize)> {
        let mut out = Vec::new();
        self.collect_hashes(min_size, &mut out);
        out
    }

    fn collect_hashes(&self, min_size: usize, out: &mut Vec<(u64, usize)>) -> (u64, usize) {
        let mut total_size = 1usize;
        let mut child_hashes = Vec::with_capacity(self.children.len());

        for child in &self.children {
            let (ch_hash, ch_size) = child.collect_hashes(min_size, out);
            total_size += ch_size;
            child_hashes.push(ch_hash);
        }

        let mut hasher = blake3::Hasher::new();
        hasher.update(self.kind.as_bytes());
        hasher.update(&[0u8]);
        for ch_hash in child_hashes {
            hasher.update(&ch_hash.to_le_bytes());
        }
        let digest = hasher.finalize();
        let bytes = digest.as_bytes();
        let hash = u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]);

        if total_size >= min_size {
            out.push((hash, total_size));
        }

        (hash, total_size)
    }
}
