use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::hash::sha256_bytes;

/// A leaf in the Merkle tree representing a single tracked file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileLeaf {
    pub path: String,
    pub hash: String,
}

/// An intermediate node representing all files belonging to one tool (e.g. "tdo").
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolNode {
    pub tool: String,
    pub hash: String,
    pub files: Vec<FileLeaf>,
}

/// A two-level Merkle tree: root → tools → files.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MerkleTree {
    pub root: String,
    pub tools: Vec<ToolNode>,
}

impl MerkleTree {
    /// Build a Merkle tree from a map of `tool -> [(relative_path, hash)]`.
    ///
    /// Files within each tool are sorted by path; tools are sorted by name.
    /// This ensures deterministic hashing.
    pub fn build(input: BTreeMap<String, Vec<(String, String)>>) -> Self {
        let mut tools = Vec::new();

        for (tool_name, mut files) in input {
            // Sort files by path for determinism
            files.sort_by(|a, b| a.0.cmp(&b.0));

            let leaves: Vec<FileLeaf> = files
                .into_iter()
                .map(|(path, hash)| FileLeaf { path, hash })
                .collect();

            // Tool hash = SHA-256 of concatenated file hashes
            let concatenated: String = leaves.iter().map(|l| l.hash.as_str()).collect();
            let tool_hash = sha256_bytes(concatenated.as_bytes());

            tools.push(ToolNode {
                tool: tool_name,
                hash: tool_hash,
                files: leaves,
            });
        }

        // Tools are already sorted because BTreeMap iterates in order

        // Root hash = SHA-256 of concatenated tool hashes
        let root_concat: String = tools.iter().map(|t| t.hash.as_str()).collect();
        let root = sha256_bytes(root_concat.as_bytes());

        MerkleTree { root, tools }
    }

    /// Check if this tree has the same root hash as another.
    pub fn same_root(&self, other: &MerkleTree) -> bool {
        self.root == other.root
    }

    /// Return tool names that differ between this tree and another.
    pub fn differing_tools<'a>(&'a self, other: &'a MerkleTree) -> Vec<&'a str> {
        let self_map: BTreeMap<&str, &str> = self
            .tools
            .iter()
            .map(|t| (t.tool.as_str(), t.hash.as_str()))
            .collect();
        let other_map: BTreeMap<&str, &str> = other
            .tools
            .iter()
            .map(|t| (t.tool.as_str(), t.hash.as_str()))
            .collect();

        let mut differing = Vec::new();

        // Tools in self that differ or are missing from other
        for (tool, hash) in &self_map {
            match other_map.get(tool) {
                Some(other_hash) if other_hash == hash => {}
                _ => differing.push(*tool),
            }
        }

        // Tools only in other
        for tool in other_map.keys() {
            if !self_map.contains_key(tool) {
                differing.push(*tool);
            }
        }

        differing.sort();
        differing.dedup();
        differing
    }

    /// Serialize to JSON bytes.
    pub fn to_json(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec_pretty(self)
    }

    /// Deserialize from JSON bytes.
    pub fn from_json(data: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_input() -> BTreeMap<String, Vec<(String, String)>> {
        let mut m = BTreeMap::new();
        m.insert(
            "tdo".to_string(),
            vec![("store.json".to_string(), "abc123".to_string())],
        );
        m
    }

    #[test]
    fn deterministic_roots() {
        let tree1 = MerkleTree::build(sample_input());
        let tree2 = MerkleTree::build(sample_input());
        assert_eq!(tree1.root, tree2.root);
        assert!(tree1.same_root(&tree2));
    }

    #[test]
    fn diff_detection() {
        let tree1 = MerkleTree::build(sample_input());

        let mut changed = sample_input();
        changed
            .get_mut("tdo")
            .unwrap()
            .push(("notes.md".to_string(), "def456".to_string()));
        let tree2 = MerkleTree::build(changed);

        assert!(!tree1.same_root(&tree2));
        let diffs = tree1.differing_tools(&tree2);
        assert_eq!(diffs, vec!["tdo"]);
    }

    #[test]
    fn diff_detects_new_tool() {
        let tree1 = MerkleTree::build(sample_input());

        let mut with_new = sample_input();
        with_new.insert(
            "nte".to_string(),
            vec![("note.md".to_string(), "xyz789".to_string())],
        );
        let tree2 = MerkleTree::build(with_new);

        let diffs = tree1.differing_tools(&tree2);
        assert!(diffs.contains(&"nte"));
    }

    #[test]
    fn json_round_trip() {
        let tree = MerkleTree::build(sample_input());
        let json = tree.to_json().unwrap();
        let restored = MerkleTree::from_json(&json).unwrap();
        assert_eq!(tree, restored);
    }
}
