//! Content-addressed Merkle trees and structural delta computation for change detection.

use std::cmp::Ordering;
use std::collections::BTreeMap;

/// A 32-byte BLAKE3 digest used as the content or structural hash of a [`MerkleNode`].
pub type Hash = [u8; 32];

/// A node in a content-addressed Merkle tree.
///
/// Leaves store the hash of their key and raw content (`"l" || key || content`), while
/// branches store the hash of their key and sorted children (`"b" || key || (child.key || child.hash)*`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MerkleNode {
    /// Segment key (e.g., file name, directory name, or AST node identifier).
    pub key: String,
    /// Present (`Some(hash)`) for leaf nodes and `None` for branch nodes.
    pub leaf_hash: Option<Hash>,
    /// Combined BLAKE3 hash of this node and its subtree.
    pub hash: Hash,
    /// Child nodes, maintained in lexicographical order by `key`.
    pub children: Vec<MerkleNode>,
}

impl MerkleNode {
    /// Constructs a leaf [`MerkleNode`] with hash `blake3("l" || key || content)`.
    #[must_use]
    pub fn leaf(key: impl Into<String>, content: &[u8]) -> Self {
        let key = key.into();
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"l");
        hasher.update(key.as_bytes());
        hasher.update(content);
        let hash: Hash = *hasher.finalize().as_bytes();
        Self {
            key,
            leaf_hash: Some(hash),
            hash,
            children: Vec::new(),
        }
    }

    /// Constructs a branch [`MerkleNode`] with sorted children and hash
    /// `blake3("b" || key || for each child: child.key || child.hash)`.
    #[must_use]
    pub fn branch(key: impl Into<String>, mut children: Vec<MerkleNode>) -> Self {
        let key = key.into();
        children.sort_by(|a, b| a.key.cmp(&b.key));
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"b");
        hasher.update(key.as_bytes());
        for child in &children {
            hasher.update(child.key.as_bytes());
            hasher.update(&child.hash);
        }
        let hash: Hash = *hasher.finalize().as_bytes();
        Self {
            key,
            leaf_hash: None,
            hash,
            children,
        }
    }

    /// Builds a directory [`MerkleNode`] tree from `"a/b/c.rs"`-style slash-separated relative paths.
    ///
    /// The resulting tree is completely independent of the iteration order of `files`.
    #[must_use]
    pub fn from_paths<I: IntoIterator<Item = (String, Vec<u8>)>>(
        root: &str,
        files: I,
    ) -> MerkleNode {
        let mut builder = DirBuilder::default();
        for (path, content) in files {
            let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
            if !parts.is_empty() {
                builder.insert(&parts, content);
            }
        }
        builder.into_node(root)
    }
}

#[derive(Default)]
struct DirBuilder {
    files: BTreeMap<String, Vec<u8>>,
    dirs: BTreeMap<String, DirBuilder>,
}

impl DirBuilder {
    fn insert(&mut self, parts: &[&str], content: Vec<u8>) {
        if parts.len() == 1 {
            self.files.insert(parts[0].to_string(), content);
        } else {
            self.dirs
                .entry(parts[0].to_string())
                .or_default()
                .insert(&parts[1..], content);
        }
    }

    fn into_node(self, key: &str) -> MerkleNode {
        let mut children = Vec::with_capacity(self.files.len() + self.dirs.len());
        for (name, content) in self.files {
            children.push(MerkleNode::leaf(name, &content));
        }
        for (name, sub) in self.dirs {
            children.push(sub.into_node(&name));
        }
        MerkleNode::branch(key, children)
    }
}

/// Set of added, removed, and changed paths between two [`MerkleNode`] trees.
///
/// Paths are `'/'`-joined keys from the root.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Delta {
    /// Paths present in `new` that were absent in `old`.
    pub added: Vec<String>,
    /// Paths present in `old` that are absent in `new`.
    pub removed: Vec<String>,
    /// Paths present in both trees whose content (or leaf/branch kind) changed.
    pub changed: Vec<String>,
}

impl Delta {
    /// Returns `true` if no paths were added, removed, or changed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.changed.is_empty()
    }
}

/// Computes the structural [`Delta`] between `old` and `new`, skipping equal subtrees.
#[must_use]
pub fn diff(old: &MerkleNode, new: &MerkleNode) -> Delta {
    let mut visited = 0usize;
    diff_with_counter(old, new, &mut visited)
}

/// Computes the [`Delta`] between `old` and `new` while counting the number of visited nodes.
pub(crate) fn diff_with_counter(old: &MerkleNode, new: &MerkleNode, visited: &mut usize) -> Delta {
    let mut delta = Delta::default();
    diff_node(old, new, "", &mut delta, visited);
    delta
}

fn diff_node(
    old: &MerkleNode,
    new: &MerkleNode,
    prefix: &str,
    delta: &mut Delta,
    visited: &mut usize,
) {
    *visited += 1;
    if old.hash == new.hash {
        return;
    }

    let old_is_leaf = old.leaf_hash.is_some();
    let new_is_leaf = new.leaf_hash.is_some();

    if old_is_leaf || new_is_leaf {
        let path = if prefix.is_empty() {
            new.key.clone()
        } else {
            prefix.to_string()
        };
        delta.changed.push(path);
        return;
    }

    let mut i = 0usize;
    let mut j = 0usize;
    let old_children = &old.children;
    let new_children = &new.children;

    while i < old_children.len() && j < new_children.len() {
        match old_children[i].key.cmp(&new_children[j].key) {
            Ordering::Less => {
                let child = &old_children[i];
                let path = join_path(prefix, &child.key);
                collect_leaves(child, &path, &mut delta.removed, visited);
                i += 1;
            }
            Ordering::Greater => {
                let child = &new_children[j];
                let path = join_path(prefix, &child.key);
                collect_leaves(child, &path, &mut delta.added, visited);
                j += 1;
            }
            Ordering::Equal => {
                let old_child = &old_children[i];
                let new_child = &new_children[j];
                if old_child.hash != new_child.hash {
                    let path = join_path(prefix, &new_child.key);
                    diff_node(old_child, new_child, &path, delta, visited);
                }
                i += 1;
                j += 1;
            }
        }
    }

    collect_all(&old_children[i..], prefix, &mut delta.removed, visited);
    collect_all(&new_children[j..], prefix, &mut delta.added, visited);
}

/// Collects the leaf paths under every one of `children`, which sit below `prefix`.
fn collect_all(children: &[MerkleNode], prefix: &str, out: &mut Vec<String>, visited: &mut usize) {
    for child in children {
        collect_leaves(child, &join_path(prefix, &child.key), out, visited);
    }
}

fn collect_leaves(node: &MerkleNode, path: &str, out: &mut Vec<String>, visited: &mut usize) {
    *visited += 1;
    if node.leaf_hash.is_some() || node.children.is_empty() {
        out.push(path.to_string());
    } else {
        for child in &node.children {
            let child_path = join_path(path, &child.key);
            collect_leaves(child, &child_path, out, visited);
        }
    }
}

fn join_path(prefix: &str, key: &str) -> String {
    if prefix.is_empty() {
        key.to_string()
    } else if key.is_empty() {
        prefix.to_string()
    } else {
        format!("{prefix}/{key}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identical_trees_produce_empty_delta() {
        let files = vec![
            ("src/lib.rs".to_string(), b"pub mod a;".to_vec()),
            ("src/a/mod.rs".to_string(), b"pub fn run() {}".to_vec()),
            (
                "Cargo.toml".to_string(),
                b"[package]\nname = \"x\"".to_vec(),
            ),
        ];
        let t1 = MerkleNode::from_paths("root", files.clone());
        let t2 = MerkleNode::from_paths("root", files);

        let mut visited = 0;
        let delta = diff_with_counter(&t1, &t2, &mut visited);
        assert!(delta.is_empty());
        assert_eq!(visited, 1, "identical root hashes must stop at the root");
    }

    #[test]
    fn test_single_leaf_edit() {
        let t1 = MerkleNode::from_paths(
            "root",
            vec![
                ("src/lib.rs".to_string(), b"v1".to_vec()),
                ("src/main.rs".to_string(), b"fn main() {}".to_vec()),
            ],
        );
        let t2 = MerkleNode::from_paths(
            "root",
            vec![
                ("src/lib.rs".to_string(), b"v2".to_vec()),
                ("src/main.rs".to_string(), b"fn main() {}".to_vec()),
            ],
        );

        let delta = diff(&t1, &t2);
        assert!(!delta.is_empty());
        assert!(delta.added.is_empty());
        assert!(delta.removed.is_empty());
        assert_eq!(delta.changed, vec!["src/lib.rs".to_string()]);
    }

    /// A tree named `name` over files given as (`path`, `content`).
    fn tree(name: &str, files: &[(&str, &str)]) -> MerkleNode {
        MerkleNode::from_paths(
            name,
            files
                .iter()
                .map(|(path, content)| (path.to_string(), content.as_bytes().to_vec()))
                .collect::<Vec<_>>(),
        )
    }

    #[test]
    fn test_add_and_remove_file() {
        let t1 = tree(
            "root",
            &[
                ("a/keep.rs", "keep"),
                ("a/old.rs", "remove me"),
                ("removed_dir/x.rs", "gone"),
            ],
        );
        let t2 = tree(
            "root",
            &[
                ("a/keep.rs", "keep"),
                ("a/new.rs", "added"),
                ("added_dir/y.rs", "fresh"),
            ],
        );

        let delta = diff(&t1, &t2);
        assert_eq!(
            delta.added,
            vec!["a/new.rs".to_string(), "added_dir/y.rs".to_string()]
        );
        assert_eq!(
            delta.removed,
            vec!["a/old.rs".to_string(), "removed_dir/x.rs".to_string()]
        );
        assert!(delta.changed.is_empty());
    }

    #[test]
    fn test_nested_directory_change_skips_unchanged_subtrees() {
        let mut base_files = Vec::new();
        for i in 0..20 {
            base_files.push((
                format!("unchanged_pkg/mod_{i}/file_{i}.rs"),
                format!("content {i}").into_bytes(),
            ));
        }
        base_files.push(("active_pkg/sub/target.rs".to_string(), b"before".to_vec()));
        base_files.push(("active_pkg/sub/sibling.rs".to_string(), b"same".to_vec()));

        let mut updated_files = base_files.clone();
        for (path, content) in &mut updated_files {
            if path == "active_pkg/sub/target.rs" {
                *content = b"after".to_vec();
            }
        }

        let t1 = MerkleNode::from_paths("root", base_files);
        let t2 = MerkleNode::from_paths("root", updated_files);

        let mut visited = 0;
        let delta = diff_with_counter(&t1, &t2, &mut visited);

        assert_eq!(delta.changed, vec!["active_pkg/sub/target.rs".to_string()]);
        assert!(delta.added.is_empty());
        assert!(delta.removed.is_empty());
        // Only root -> active_pkg -> sub -> target.rs should be visited (4 nodes),
        // skipping the entire `unchanged_pkg` subtree (41 nodes) and `sibling.rs` (1 node).
        assert_eq!(visited, 4);
    }

    #[test]
    fn test_from_paths_order_independence() {
        let t_a = tree(
            "repo",
            &[
                ("z/b.rs", "1"),
                ("a/c.rs", "2"),
                ("a/a.rs", "3"),
                ("m.rs", "4"),
            ],
        );
        let t_b = tree(
            "repo",
            &[
                ("m.rs", "4"),
                ("a/a.rs", "3"),
                ("z/b.rs", "1"),
                ("a/c.rs", "2"),
            ],
        );

        assert_eq!(t_a.hash, t_b.hash);
        assert_eq!(t_a, t_b);
        assert!(diff(&t_a, &t_b).is_empty());
    }
}
