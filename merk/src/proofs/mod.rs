//! Merk proofs

#[cfg(feature = "minimal")]
pub mod chunk;
#[cfg(any(feature = "minimal", feature = "verify"))]
pub mod encoding;
#[cfg(feature = "list_mode")]
pub mod positional;
#[cfg(any(feature = "minimal", feature = "verify"))]
pub mod query;
#[cfg(any(feature = "minimal", feature = "verify"))]
pub mod tree;

#[cfg(feature = "minimal")]
pub use encoding::encode_into;
#[cfg(any(feature = "minimal", feature = "verify"))]
pub use encoding::Decoder;
#[cfg(any(feature = "minimal", feature = "verify"))]
pub use query::Query;
#[cfg(feature = "minimal")]
pub use tree::Tree;

#[cfg(any(feature = "minimal", feature = "verify"))]
use crate::{tree::CryptoHash, TreeFeatureType};

#[cfg(any(feature = "minimal", feature = "verify"))]
/// A proof operator, executed to verify the data in a Merkle proof.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    /// Pushes a node on the stack.
    /// Signifies ascending node keys
    Push(Node),

    /// Pushes a node on the stack
    /// Signifies descending node keys
    PushInverted(Node),

    /// Pops the top stack item as `parent`. Pops the next top stack item as
    /// `child`. Attaches `child` as the left child of `parent`. Pushes the
    /// updated `parent` back on the stack.
    Parent,

    /// Pops the top stack item as `child`. Pops the next top stack item as
    /// `parent`. Attaches `child` as the right child of `parent`. Pushes the
    /// updated `parent` back on the stack.
    Child,

    /// Pops the top stack item as `parent`. Pops the next top stack item as
    /// `child`. Attaches `child` as the right child of `parent`. Pushes the
    /// updated `parent` back on the stack.
    ParentInverted,

    /// Pops the top stack item as `child`. Pops the next top stack item as
    /// `parent`. Attaches `child` as the left child of `parent`. Pushes the
    /// updated `parent` back on the stack.
    ChildInverted,
}

#[cfg(any(feature = "minimal", feature = "verify"))]
/// A selected piece of data about a single tree node, to be contained in a
/// `Push` operator in a proof.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Node {
    /// Represents the hash of a tree node.
    Hash(CryptoHash),

    /// Represents the hash of the key/value pair of a tree node.
    KVHash(CryptoHash),

    /// Represents the key/value_hash pair of a tree node
    /// same as the Node::KV but the value is not required by the proof
    KVDigest(Vec<u8>, CryptoHash),

    /// Represents the key and value of a tree node.
    KV(Vec<u8>, Vec<u8>),

    /// Represents the key, value and value_hash of a tree node
    KVValueHash(Vec<u8>, Vec<u8>, CryptoHash),

    /// Represents, the key, value, value_hash and feature_type of a tree node
    /// Used by Sum trees
    KVValueHashFeatureType(Vec<u8>, Vec<u8>, CryptoHash, TreeFeatureType),

    /// Represents the key, value of some referenced node and value_hash of
    /// current tree node
    KVRefValueHash(Vec<u8>, Vec<u8>, CryptoHash),

    // List-mode variants with subtree_size metadata
    /// Represents the hash and subtree_size of a tree node (for list-mode)
    HashWithSubtreeSize(CryptoHash, u64, Option<u8>),

    /// Represents the key/value pair, subtree_size, and optional parent key of a tree node (for list-mode)
    KVWithSubtreeSize(Vec<u8>, Vec<u8>, u64, Option<Vec<u8>>),

    /// Represents the key, value, value_hash, subtree_size, and optional parent key of a tree node (for list-mode)
    KVValueHashWithSubtreeSize(Vec<u8>, Vec<u8>, CryptoHash, u64, Option<Vec<u8>>),

    /// Represents the key, value, value_hash, feature_type, subtree_size, and optional parent key of a tree node (for list-mode)
    KVValueHashFeatureTypeWithSubtreeSize(
        Vec<u8>,
        Vec<u8>,
        CryptoHash,
        TreeFeatureType,
        u64,
        Option<Vec<u8>>,
    ),

    /// Represents the key, referenced value, node value hash, subtree_size, and optional parent key (for list-mode)
    KVRefValueHashWithSubtreeSize(Vec<u8>, Vec<u8>, CryptoHash, u64, Option<Vec<u8>>),

    /// Represents the KV hash of a node plus subtree_size and optional parent key (for list-mode)
    KVHashWithSubtreeSize(CryptoHash, u64, Option<Vec<u8>>),

    /// Represents the key/value_hash pair and subtree_size plus optional parent key of a node (for list-mode)
    KVDigestWithSubtreeSize(Vec<u8>, CryptoHash, u64, Option<Vec<u8>>),
}

use std::fmt;

#[cfg(any(feature = "minimal", feature = "verify"))]
impl fmt::Display for Node {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let node_string = match self {
            Node::Hash(hash) => format!("Hash(HASH[{}])", hex::encode(hash)),
            Node::KVHash(kv_hash) => format!("KVHash(HASH[{}])", hex::encode(kv_hash)),
            Node::KV(key, value) => {
                format!("KV({}, {})", hex_to_ascii(key), hex_to_ascii(value))
            }
            Node::KVValueHash(key, value, value_hash) => format!(
                "KVValueHash({}, {}, HASH[{}])",
                hex_to_ascii(key),
                hex_to_ascii(value),
                hex::encode(value_hash)
            ),
            Node::KVDigest(key, value_hash) => format!(
                "KVDigest({}, HASH[{}])",
                hex_to_ascii(key),
                hex::encode(value_hash)
            ),
            Node::KVRefValueHash(key, value, value_hash) => format!(
                "KVRefValueHash({}, {}, HASH[{}])",
                hex_to_ascii(key),
                hex_to_ascii(value),
                hex::encode(value_hash)
            ),
            Node::KVValueHashFeatureType(key, value, value_hash, feature_type) => format!(
                "KVValueHashFeatureType({}, {}, HASH[{}], {:?})",
                hex_to_ascii(key),
                hex_to_ascii(value),
                hex::encode(value_hash),
                feature_type
            ),
            Node::HashWithSubtreeSize(hash, size, maybe_height) => {
                let height_desc = maybe_height
                    .map(|h| h.to_string())
                    .unwrap_or_else(|| "<unknown>".to_string());
                format!(
                    "HashWithSubtreeSize(HASH[{}], size={}, height={})",
                    hex::encode(hash),
                    size,
                    height_desc
                )
            }
            Node::KVWithSubtreeSize(key, value, size, parent_key) => {
                let parent_desc = parent_key
                    .as_ref()
                    .map(|pk| hex_to_ascii(pk))
                    .unwrap_or_else(|| "<None>".to_string());
                format!(
                    "KVWithSubtreeSize({}, {}, size={}, parent_key={})",
                    hex_to_ascii(key),
                    hex_to_ascii(value),
                    size,
                    parent_desc
                )
            }
            Node::KVValueHashWithSubtreeSize(key, value, value_hash, size, parent_key) => {
                let parent_desc = parent_key
                    .as_ref()
                    .map(|pk| hex_to_ascii(pk))
                    .unwrap_or_else(|| "<None>".to_string());
                format!(
                    "KVValueHashWithSubtreeSize({}, {}, HASH[{}], size={}, parent_key={})",
                    hex_to_ascii(key),
                    hex_to_ascii(value),
                    hex::encode(value_hash),
                    size,
                    parent_desc
                )
            }
            Node::KVValueHashFeatureTypeWithSubtreeSize(
                key,
                value,
                value_hash,
                feature_type,
                size,
                parent_key,
            ) => {
                let parent_desc = parent_key
                    .as_ref()
                    .map(|pk| hex_to_ascii(pk))
                    .unwrap_or_else(|| "<None>".to_string());
                format!(
                    "KVValueHashFeatureTypeWithSubtreeSize({}, {}, HASH[{}], {:?}, size={}, parent_key={})",
                    hex_to_ascii(key),
                    hex_to_ascii(value),
                    hex::encode(value_hash),
                    feature_type,
                    size,
                    parent_desc
                )
            }
            Node::KVRefValueHashWithSubtreeSize(key, value, value_hash, size, parent_key) => {
                let parent_desc = parent_key
                    .as_ref()
                    .map(|pk| hex_to_ascii(pk))
                    .unwrap_or_else(|| "<None>".to_string());
                format!(
                    "KVRefValueHashWithSubtreeSize({}, {}, HASH[{}], size={}, parent_key={})",
                    hex_to_ascii(key),
                    hex_to_ascii(value),
                    hex::encode(value_hash),
                    size,
                    parent_desc
                )
            }
            Node::KVHashWithSubtreeSize(hash, size, parent_key) => {
                let parent_desc = parent_key
                    .as_ref()
                    .map(|pk| hex_to_ascii(pk))
                    .unwrap_or_else(|| "<None>".to_string());
                format!(
                    "KVHashWithSubtreeSize(HASH[{}], size={}, parent_key={})",
                    hex::encode(hash),
                    size,
                    parent_desc
                )
            }
            Node::KVDigestWithSubtreeSize(key, value_hash, size, parent_key) => {
                let parent_desc = parent_key
                    .as_ref()
                    .map(|pk| hex_to_ascii(pk))
                    .unwrap_or_else(|| "<None>".to_string());
                format!(
                    "KVDigestWithSubtreeSize({}, HASH[{}], size={}, parent_key={})",
                    hex_to_ascii(key),
                    hex::encode(value_hash),
                    size,
                    parent_desc
                )
            }
        };
        write!(f, "{}", node_string)
    }
}

fn hex_to_ascii(hex_value: &[u8]) -> String {
    if hex_value.len() == 1 && hex_value[0] < b"0"[0] {
        hex::encode(hex_value)
    } else {
        String::from_utf8(hex_value.to_vec()).unwrap_or_else(|_| hex::encode(hex_value))
    }
}
