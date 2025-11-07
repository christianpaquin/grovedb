//! Merk trees

#[cfg(feature = "minimal")]
mod commit;
#[cfg(feature = "minimal")]
mod debug;
#[cfg(feature = "minimal")]
mod encoding;
#[cfg(feature = "minimal")]
mod fuzz_tests;
#[cfg(any(feature = "minimal", feature = "verify"))]
pub mod hash;
#[cfg(feature = "minimal")]
mod iter;
#[cfg(feature = "minimal")]
mod just_in_time_value_update;
#[cfg(feature = "minimal")]
pub mod kv;
#[cfg(feature = "minimal")]
mod link;
#[cfg(all(feature = "full", feature = "list_mode"))]
mod list_mode_persistence_tests;
#[cfg(feature = "minimal")]
mod ops;
#[cfg(any(feature = "minimal", feature = "verify"))]
mod tree_feature_type;
#[cfg(feature = "minimal")]
mod walk;

#[cfg(feature = "minimal")]
use std::cmp::{max, Ordering};

#[cfg(feature = "minimal")]
pub use commit::{Commit, NoopCommit};
#[cfg(feature = "minimal")]
use ed::{Decode, Encode, Terminated};
#[cfg(feature = "minimal")]
use grovedb_costs::{
    cost_return_on_error, cost_return_on_error_default, cost_return_on_error_no_add,
    storage_cost::{
        key_value_cost::KeyValueStorageCost,
        removal::{StorageRemovedBytes, StorageRemovedBytes::BasicStorageRemoval},
        StorageCost,
    },
    CostContext, CostResult, CostsExt, OperationCost,
};
#[cfg(feature = "minimal")]
use grovedb_version::version::GroveVersion;
#[cfg(any(feature = "minimal", feature = "verify"))]
pub use hash::{
    combine_hash, kv_digest_to_kv_hash, kv_hash, node_hash, node_hash_list_mode, value_hash, CryptoHash, HASH_LENGTH,
    NULL_HASH,
};
#[cfg(feature = "minimal")]
pub use hash::{HASH_BLOCK_SIZE, HASH_BLOCK_SIZE_U32, HASH_LENGTH_U32, HASH_LENGTH_U32_X2};
#[cfg(feature = "minimal")]
use integer_encoding::VarInt;
#[cfg(feature = "minimal")]
use kv::KV;
#[cfg(feature = "minimal")]
pub use link::Link;
#[cfg(feature = "minimal")]
pub use ops::{AuxMerkBatch, BatchEntry, MerkBatch, Op, PanicSource};
#[cfg(feature = "minimal")]
pub use tree_feature_type::AggregateData;
#[cfg(any(feature = "minimal", feature = "verify"))]
pub use tree_feature_type::TreeFeatureType;
#[cfg(feature = "minimal")]
pub use walk::{Fetch, RefWalker, Walker};

#[cfg(feature = "minimal")]
use crate::merk::NodeType;
#[cfg(feature = "minimal")]
use crate::tree::hash::HASH_LENGTH_X2;
#[cfg(feature = "minimal")]
use crate::tree::kv::ValueDefinedCostType;
#[cfg(feature = "minimal")]
use crate::tree::kv::ValueDefinedCostType::{LayeredValueDefinedCost, SpecializedValueDefinedCost};
#[cfg(feature = "minimal")]
use crate::{error::Error, Error::Overflow};
// TODO: remove need for `TreeInner`, and just use `Box<Self>` receiver for
// relevant methods

#[cfg(feature = "minimal")]
/// The fields of the `Tree` type, stored on the heap.
#[derive(Clone, Encode, Decode, Debug, PartialEq)]
pub struct TreeNodeInner {
    pub(crate) left: Option<Link>,
    pub(crate) right: Option<Link>,
    pub(crate) kv: KV,
}

#[cfg(feature = "minimal")]
impl TreeNodeInner {
    /// Get the value as owned of the key value struct
    pub fn value_as_owned(self) -> Vec<u8> {
        self.kv.value
    }

    /// Get the value as owned of the key value struct
    pub fn value_as_owned_with_feature(self) -> (Vec<u8>, TreeFeatureType) {
        (self.kv.value, self.kv.feature_type)
    }

    /// Get the value as slice of the key value struct
    pub fn value_as_slice(&self) -> &[u8] {
        self.kv.value.as_slice()
    }

    /// Get the key as owned of the key value struct
    pub fn key_as_owned(self) -> Vec<u8> {
        self.kv.key
    }

    /// Get the key as slice of the key value struct
    pub fn key_as_slice(&self) -> &[u8] {
        self.kv.key.as_slice()
    }
}

#[cfg(feature = "minimal")]
impl Terminated for Box<TreeNodeInner> {}

#[cfg(feature = "minimal")]
/// A binary AVL tree data structure, with Merkle hashes.
///
/// Trees' inner fields are stored on the heap so that nodes can recursively
/// link to each other, and so we can detach nodes from their parents, then
/// reattach without allocating or freeing heap memory.
#[derive(Clone, PartialEq)]
pub struct TreeNode {
    pub(crate) inner: Box<TreeNodeInner>,
    pub(crate) old_value: Option<Vec<u8>>,
    pub(crate) known_storage_cost: Option<KeyValueStorageCost>,
    #[cfg(feature = "list_mode")]
    /// Parent node key (in-memory only; not persisted yet). None if root.
    pub(crate) parent_key: Option<Vec<u8>>,
    #[cfg(feature = "list_mode")]
    /// Side of this node relative to its parent (true = left, false = right). None if root.
    pub(crate) child_side: Option<bool>,
    #[cfg(feature = "list_mode")]
    /// Whether this node participates in list-mode (positional) semantics.
    pub(crate) list_mode: bool,
    #[cfg(feature = "list_mode")]
    /// Cached subtree size (number of elements in this subtree) when list_mode is active.
    pub(crate) subtree_size: u64,
    #[cfg(feature = "list_mode")]
    /// Whether to use parent pointers for hashing (false for StandaloneMerk, true for nested trees in GroveDB).
    pub(crate) use_parent_pointers: bool,
}

#[cfg(feature = "minimal")]
impl TreeNode {
    /// Creates a new `Tree` with the given key and value, and no children.
    ///
    /// Hashes the key/value pair and initializes the `kv_hash` field.
    pub fn new(
        key: Vec<u8>,
        value: Vec<u8>,
        value_defined_cost: Option<ValueDefinedCostType>,
        feature_type: TreeFeatureType,
    ) -> CostContext<Self> {
        KV::new(key, value, value_defined_cost, feature_type).map(|kv| Self {
            inner: Box::new(TreeNodeInner {
                kv,
                left: None,
                right: None,
            }),
            old_value: None,
            known_storage_cost: None,
            #[cfg(feature = "list_mode")]
            parent_key: None,
            #[cfg(feature = "list_mode")]
            child_side: None,
            #[cfg(feature = "list_mode")]
            list_mode: false,
            #[cfg(feature = "list_mode")]
            subtree_size: 1,
            #[cfg(feature = "list_mode")]
            use_parent_pointers: false, // Default to false for standalone trees
        })
    }

    /// Creates a new `Tree` given an inner tree
    pub fn new_with_tree_inner(inner_tree: TreeNodeInner) -> Self {
        let old_value = inner_tree.kv.value.clone();
        Self {
            inner: Box::new(inner_tree),
            old_value: Some(old_value),
            known_storage_cost: None,
            #[cfg(feature = "list_mode")]
            parent_key: None,
            #[cfg(feature = "list_mode")]
            child_side: None,
            #[cfg(feature = "list_mode")]
            list_mode: false,
            #[cfg(feature = "list_mode")]
            subtree_size: 1,
            #[cfg(feature = "list_mode")]
            use_parent_pointers: false,
        }
    }

    // ===== List mode operations =====
    #[cfg(feature = "list_mode")]
    #[inline]
    pub fn is_list_mode(&self) -> bool { self.list_mode }

    #[cfg(feature = "list_mode")]
    #[inline]
    pub fn subtree_size(&self) -> u64 { if self.list_mode { self.subtree_size } else { 0 } }

    #[cfg(feature = "list_mode")]
    fn child_subtree_size(&self, left: bool) -> u64 {
        self.child(left).map_or(0, |c| c.subtree_size())
    }

    #[cfg(feature = "list_mode")]
    fn recompute_subtree_size(&mut self) {
        if self.list_mode {
            let left = self.child_subtree_size(true);
            let right = self.child_subtree_size(false);
            self.subtree_size = 1 + left + right;
        }
    }

    #[cfg(feature = "list_mode")]
    pub fn recompute_subtree_sizes_recursive(&mut self) {
        if let Some(c) = self.child_mut(true) { c.recompute_subtree_sizes_recursive(); }
        if let Some(c) = self.child_mut(false) { c.recompute_subtree_sizes_recursive(); }
        self.recompute_subtree_size();
    }

    #[cfg(feature = "list_mode")]
    /// Convert an existing (non-list) subtree to list mode recursively.
    /// Safe to call multiple times; idempotent.
    pub fn convert_subtree_to_list_mode(&mut self) {
        if self.list_mode { return; }
        self.list_mode = true;
        if let Some(c) = self.child_mut(true) { c.convert_subtree_to_list_mode(); }
        if let Some(c) = self.child_mut(false) { c.convert_subtree_to_list_mode(); }
        self.recompute_subtree_size();
    }

    #[cfg(feature = "list_mode")]
    /// Create a new list-mode leaf node with a random UUID key (meaningless key for positional editing).
    /// Uses BasicMerkNode feature type; caller can replace value/key later if needed.
    ///
    /// For collaborative scenarios where clients need to pick UUIDs locally, use `new_list_node_with_key`.
    pub fn new_list_node(value: Vec<u8>) -> CostContext<Self> {
        use uuid::Uuid;
        let key = Uuid::new_v4().as_bytes().to_vec();
        Self::new_list_node_with_key(key, value)
    }

    #[cfg(feature = "list_mode")]
    /// Create a new list-mode leaf node with a client-provided key.
    ///
    /// This variant allows clients to pick their own UUID keys locally before sending
    /// insertions to the server, enabling optimistic local updates without waiting for
    /// server response. The key should be a unique identifier (typically a 16-byte UUID).
    ///
    /// # Arguments
    /// * `key` - Client-provided unique key (typically UUID bytes)
    /// * `value` - The value to store in this node
    ///
    /// # Example
    /// ```ignore
    /// use uuid::Uuid;
    /// let my_uuid = Uuid::new_v4().as_bytes().to_vec();
    /// let node = TreeNode::new_list_node_with_key(my_uuid, vec![b'x']).unwrap();
    /// ```
    pub fn new_list_node_with_key(key: Vec<u8>, value: Vec<u8>) -> CostContext<Self> {
        KV::new(key, value, None, TreeFeatureType::BasicMerkNode).map(|kv| Self {
            inner: Box::new(TreeNodeInner {
                kv,
                left: None,
                right: None,
            }),
            old_value: None,
            known_storage_cost: None,
            parent_key: None,
            child_side: None,
            list_mode: true,
            subtree_size: 1,
            use_parent_pointers: true, // List-mode operations require parent pointers for position calculations
        })
    }

    #[cfg(feature = "list_mode")]
    /// Set this node's parent pointer metadata.
    fn set_parent_pointer(&mut self, parent_key: &[u8], side: bool) {
        self.parent_key = Some(parent_key.to_vec());
        self.child_side = Some(side);
    }

    #[cfg(feature = "list_mode")]
    /// Clear parent pointer metadata (used when detaching / making root).
    fn clear_parent_pointer(&mut self) {
        self.parent_key = None;
        self.child_side = None;
    }

    #[cfg(feature = "list_mode")]
    /// Recursively disable parent pointers for this node and all descendants.
    /// This is used for standalone Merk trees where parent pointers would cause
    /// hash mismatches in proofs.
    pub fn disable_parent_pointers_recursive(&mut self) {
        self.use_parent_pointers = false;
        self.parent_key = None;
        self.child_side = None;
        
        // Recursively disable for children (handle all Link types that have trees in memory)
        // We need to handle Uncommitted and Loaded links specially because they have cached hashes
        // that were computed with parent pointers enabled
        match self.inner.left.take() {
            Some(Link::Modified { tree: mut child_tree, pending_writes, child_heights }) => {
                child_tree.disable_parent_pointers_recursive();
                self.inner.left = Some(Link::Modified {
                    tree: child_tree,
                    pending_writes,
                    child_heights,
                });
            }
            Some(Link::Uncommitted { mut tree, child_heights, .. }) => {
                tree.disable_parent_pointers_recursive();
                // Convert Uncommitted to Modified since hash needs recomputation
                self.inner.left = Some(Link::Modified {
                    pending_writes: 1,
                    child_heights,
                    tree,
                });
            }
            Some(Link::Loaded { mut tree, child_heights, .. }) => {
                tree.disable_parent_pointers_recursive();
                // Convert Loaded to Modified since hash needs recomputation
                self.inner.left = Some(Link::Modified {
                    pending_writes: 1,
                    child_heights,
                    tree,
                });
            }
            other => self.inner.left = other, // Reference or None - no action needed
        }
        
        match self.inner.right.take() {
            Some(Link::Modified { tree: mut child_tree, pending_writes, child_heights }) => {
                child_tree.disable_parent_pointers_recursive();
                self.inner.right = Some(Link::Modified {
                    tree: child_tree,
                    pending_writes,
                    child_heights,
                });
            }
            Some(Link::Uncommitted { mut tree, child_heights, .. }) => {
                tree.disable_parent_pointers_recursive();
                // Convert Uncommitted to Modified since hash needs recomputation
                self.inner.right = Some(Link::Modified {
                    pending_writes: 1,
                    child_heights,
                    tree,
                });
            }
            Some(Link::Loaded { mut tree, child_heights, .. }) => {
                tree.disable_parent_pointers_recursive();
                // Convert Loaded to Modified since hash needs recomputation
                self.inner.right = Some(Link::Modified {
                    pending_writes: 1,
                    child_heights,
                    tree,
                });
            }
            other => self.inner.right = other, // Reference or None - no action needed
        }
    }

    #[cfg(feature = "list_mode")]
    /// Recursively enable parent pointers for this node and all descendants.
    /// This is used for GroveDB nested trees where parent pointers are needed for position computation.
    pub fn enable_parent_pointers_recursive(&mut self) {
        self.use_parent_pointers = true;
        
        // Recursively enable for children (but don't set parent_key yet - that happens during attach)
        match &mut self.inner.left {
            Some(Link::Modified { tree, .. }) => {
                tree.enable_parent_pointers_recursive();
            }
            Some(Link::Uncommitted { tree, .. }) => {
                tree.enable_parent_pointers_recursive();
            }
            _ => {}
        }
        match &mut self.inner.right {
            Some(Link::Modified { tree, .. }) => {
                tree.enable_parent_pointers_recursive();
            }
            Some(Link::Uncommitted { tree, .. }) => {
                tree.enable_parent_pointers_recursive();
            }
            _ => {}
        }
    }

    #[cfg(feature = "list_mode")]
    /// Compute the 0-based position of this node (in-order traversal) using parent pointers and subtree sizes.
    /// Requires an external fetch function to load parent nodes (e.g. from persistent storage) by key.
    /// Returns None if any required metadata is missing or a fetch fails.
    pub fn compute_position_with_parent_fetch<F>(&self, mut fetch: F) -> Option<u64>
    where
        F: FnMut(&[u8]) -> Option<Self>,
    {
        if !self.list_mode { return None; }
        let mut pos = self.child_subtree_size(true); // nodes before self in own subtree
        let mut side_opt = self.child_side;
        let mut parent_key_opt = self.parent_key.clone();
        while let (Some(parent_key), Some(side)) = (parent_key_opt.as_ref(), side_opt) {
            let parent = fetch(parent_key.as_slice())?; // abort if parent not found
            if !parent.list_mode { return None; }
            if !side { // we are right child
                pos += 1; // count parent
                pos += parent.child_subtree_size(true); // all nodes in left subtree of parent
            }
            side_opt = parent.child_side;
            parent_key_opt = parent.parent_key.clone();
        }
        Some(pos)
    }

    #[cfg(feature = "list_mode")]
    /// Compute position in-order using just the left subtree size and child_side metadata.
    /// This version doesn't climb the parent chain - assumes parent pointers are ephemeral.
    /// For persistent parent climbing, use compute_position_with_parent_fetch.
    pub fn compute_position(&self) -> Option<u64> {
        if !self.list_mode { return None; }
        // For now just return left subtree size (position within current subtree)
        // Full position requires parent chain which needs storage access
        Some(self.child_subtree_size(true))
    }

    #[cfg(feature = "list_mode")]
    /// Find the node at the given 0-based position using subtree_size descent.
    /// Returns a mutable reference to the node and the path taken (for updating ancestors).
    /// This is a helper for insert_at and other positional operations.
    fn find_node_at_position_mut(&mut self, target_pos: u64) -> Option<&mut Self> {
        if !self.list_mode { return None; }
        let left_size = self.child_subtree_size(true);
        
        if target_pos < left_size {
            // Target is in left subtree
            if let Some(left_child) = self.child_mut(true) {
                return left_child.find_node_at_position_mut(target_pos);
            }
            None
        } else if target_pos == left_size {
            // This is the target node
            Some(self)
        } else {
            // Target is in right subtree  
            // Adjust position: subtract left_size + 1 (current node)
            let right_pos = target_pos - left_size - 1;
            if let Some(right_child) = self.child_mut(false) {
                return right_child.find_node_at_position_mut(right_pos);
            }
            None
        }
    }

    #[cfg(feature = "list_mode")]
    /// Insert a new node at the given 0-based position.
    /// This is a simplified in-memory version that doesn't handle persistence or balancing.
    /// Returns the inserted node's key on success.
    /// 
    /// Algorithm:
    /// 1. Traverse to insertion point using subtree_size
    /// 2. Create new leaf with random UUID key
    /// 3. Attach at appropriate position
    /// 4. Update subtree_sizes upward (requires parent chain or re-traversal)
    /// 
    /// Note: For a full persistence-aware implementation, this would need:
    /// - Storage context for writing updated nodes
    /// - Parent chain updates for all ancestors
    /// - Optional balancing
    ///
    /// For collaborative scenarios where clients need to pick UUIDs locally, use `insert_at_position_with_key`.
    pub fn insert_at_position(self, position: u64, value: Vec<u8>) -> CostContext<Result<(Self, Vec<u8>), Error>> {
        use uuid::Uuid;
        let key = Uuid::new_v4().as_bytes().to_vec();
        self.insert_at_position_with_key(position, key, value)
    }

    #[cfg(feature = "list_mode")]
    /// Insert a new node at the given 0-based position with a client-provided key.
    ///
    /// This variant allows clients to specify their own UUID keys for optimistic local
    /// updates in collaborative editing scenarios. The client can pick a UUID locally,
    /// add the character to their local view, and send the insertion to the server
    /// without waiting for a response to learn the UUID.
    ///
    /// # Arguments
    /// * `position` - 0-based position for insertion (0 <= position <= tree size)
    /// * `key` - Client-provided unique key (typically UUID bytes)
    /// * `value` - The value to insert
    ///
    /// # Returns
    /// * `Ok((updated_tree, key))` - The updated tree and the inserted key (same as provided)
    /// * `Err(...)` - If position is invalid or tree is not in list_mode
    ///
    /// # Example
    /// ```ignore
    /// use uuid::Uuid;
    /// let my_uuid = Uuid::new_v4().as_bytes().to_vec();
    /// let (tree, key) = tree.insert_at_position_with_key(5, my_uuid.clone(), vec![b'x'])
    ///     .unwrap()
    ///     .unwrap();
    /// assert_eq!(key, my_uuid);
    /// ```
    pub fn insert_at_position_with_key(
        self,
        position: u64,
        key: Vec<u8>,
        value: Vec<u8>,
    ) -> CostContext<Result<(Self, Vec<u8>), Error>> {
        if !self.list_mode {
            return Err(Error::InternalError("insert_at_position_with_key requires list_mode"))
                .wrap_with_cost(OperationCost::default());
        }
        
        let total_size = self.subtree_size();
        if position > total_size {
            return Err(Error::InternalError("insert position exceeds tree size"))
                .wrap_with_cost(OperationCost::default());
        }

        // Create new leaf node with client-provided key
        let new_node = TreeNode::new_list_node_with_key(key.clone(), value).unwrap();

        // Simple case: empty tree
        if total_size == 0 {
            return Ok((new_node, key)).wrap_with_cost(OperationCost::default());
        }

        // Recursive insertion helper that balances on the way back up
        fn insert_recursive(node: TreeNode, position: u64, new_node: TreeNode) -> TreeNode {
            let left_size = node.child_subtree_size(true);
            
            if position <= left_size {
                let (node, maybe_left) = node.detach(true);
                let new_left = if let Some(left_child) = maybe_left {
                    insert_recursive(left_child, position, new_node)
                } else {
                    new_node
                };
                let node = node.attach(true, Some(new_left));
                node.balance()  // Balance after attaching
            } else {
                let right_pos = position - left_size - 1;
                let (node, maybe_right) = node.detach(false);
                let new_right = if let Some(right_child) = maybe_right {
                    insert_recursive(right_child, right_pos, new_node)
                } else {
                    new_node
                };
                let node = node.attach(false, Some(new_right));
                node.balance()  // Balance after attaching
            }
        }

        let mut result_tree = insert_recursive(self, position, new_node);
        result_tree.recompute_subtree_sizes_recursive();
        Ok((result_tree, key)).wrap_with_cost(OperationCost::default())
    }

    #[cfg(feature = "list_mode")]
    /// Delete the node at the given position in list_mode.
    ///
    /// Returns the updated tree and the (key, value) of the deleted node.
    ///
    /// Requirements:
    /// - Tree must be in list_mode
    /// - Position must be valid (0 <= position < subtree_size)
    ///
    /// Algorithm:
    /// - Recursively descend using subtree_size to find the target node
    /// - When found, detach it and merge its children
    /// - Update subtree sizes on the way back up
    pub fn delete_at_position(self, position: u64) -> CostContext<Result<(Self, Vec<u8>, Vec<u8>), Error>> {
        if !self.list_mode {
            return Err(Error::InternalError("delete_at_position requires list_mode"))
                .wrap_with_cost(OperationCost::default());
        }
        
        let total_size = self.subtree_size();
        if position >= total_size {
            return Err(Error::InternalError("delete position out of bounds"))
                .wrap_with_cost(OperationCost::default());
        }

        // Recursive deletion helper that balances on the way back up
        fn delete_recursive(node: TreeNode, position: u64) -> (Option<TreeNode>, Vec<u8>, Vec<u8>) {
            let left_size = node.child_subtree_size(true);
            
            if position < left_size {
                // Delete from left subtree
                let (node, maybe_left) = node.detach(true);
                if let Some(left_child) = maybe_left {
                    let (new_left, key, value) = delete_recursive(left_child, position);
                    let updated = if let Some(left) = new_left {
                        let node = node.attach(true, Some(left));
                        node.balance()  // Balance after deletion
                    } else {
                        node
                    };
                    (Some(updated), key, value)
                } else {
                    unreachable!("left_size > 0 but no left child")
                }
            } else if position == left_size {
                // This is the node to delete
                let key = node.key().to_vec();
                let value = node.inner.kv.value_as_slice().to_vec();
                
                // Detach both children
                let (node, maybe_left) = node.detach(true);
                let (_node, maybe_right) = node.detach(false);
                
                // Merge children: if both exist, attach left as the new subtree
                // and re-attach right to the rightmost node of left
                match (maybe_left, maybe_right) {
                    (None, None) => (None, key, value),
                    (Some(left), None) => (Some(left), key, value),
                    (None, Some(right)) => (Some(right), key, value),
                    (Some(left), Some(right)) => {
                        // Attach right to rightmost position in left subtree
                        fn attach_rightmost(node: TreeNode, to_attach: TreeNode) -> TreeNode {
                            let (node, maybe_right) = node.detach(false);
                            if let Some(right_child) = maybe_right {
                                let updated_right = attach_rightmost(right_child, to_attach);
                                let node = node.attach(false, Some(updated_right));
                                node.balance()  // Balance after reattaching
                            } else {
                                node.attach(false, Some(to_attach))
                            }
                        }
                        let merged = attach_rightmost(left, right);
                        (Some(merged), key, value)
                    }
                }
            } else {
                // Delete from right subtree
                let right_pos = position - left_size - 1;
                let (node, maybe_right) = node.detach(false);
                if let Some(right_child) = maybe_right {
                    let (new_right, key, value) = delete_recursive(right_child, right_pos);
                    let updated = if let Some(right) = new_right {
                        let node = node.attach(false, Some(right));
                        node.balance()  // Balance after deletion
                    } else {
                        node
                    };
                    (Some(updated), key, value)
                } else {
                    unreachable!("position > left_size but no right child")
                }
            }
        }

        let (maybe_tree, key, value) = delete_recursive(self, position);
        if let Some(mut tree) = maybe_tree {
            tree.recompute_subtree_sizes_recursive();
            Ok((tree, key, value)).wrap_with_cost(OperationCost::default())
        } else {
            // Tree became empty
            Err(Error::InternalError("cannot delete last node from tree"))
                .wrap_with_cost(OperationCost::default())
        }
    }

    #[cfg(feature = "list_mode")]
    /// Insert a new value immediately after the node with the given key.
    ///
    /// This is a high-level wrapper for collaborative editing scenarios where you want to
    /// insert after a known UUID key (e.g., "insert character after UUID X").
    ///
    /// Requirements:
    /// - Tree must be in list_mode
    /// - Must provide a fetch closure to load nodes by key (for parent chain traversal)
    /// - The target key must exist in the tree
    ///
    /// Algorithm:
    /// 1. Find the node with target_key using fetch
    /// 2. Compute its position using compute_position_with_parent_fetch
    /// 3. Insert new value at position + 1 using insert_at_position
    ///
    /// Returns the updated tree and the new node's UUID key.
    pub fn insert_after_key<F>(
        self,
        target_key: &[u8],
        value: Vec<u8>,
        mut fetch: F,
    ) -> CostContext<Result<(Self, Vec<u8>), Error>>
    where
        F: FnMut(&[u8]) -> Option<Self>,
    {
        if !self.list_mode {
            return Err(Error::InternalError("insert_after_key requires list_mode"))
                .wrap_with_cost(OperationCost::default());
        }

        // Fetch the target node
        let target_node = match fetch(target_key) {
            Some(node) => node,
            None => {
                return Err(Error::InternalError("target key not found in tree"))
                    .wrap_with_cost(OperationCost::default());
            }
        };

        // Compute its position
        let position = match target_node.compute_position_with_parent_fetch(fetch) {
            Some(pos) => pos,
            None => {
                return Err(Error::InternalError("could not compute position for target key"))
                    .wrap_with_cost(OperationCost::default());
            }
        };

        // Insert at position + 1 (after the target)
        self.insert_at_position(position + 1, value)
    }

    #[cfg(feature = "list_mode")]
    /// Insert a new node after the node with target_key, using a client-provided key.
    ///
    /// This is a reference-based operation, allowing the client to provide the UUID,
    /// enabling optimistic local updates: the client can show the new character
    /// immediately while the server confirms the operation asynchronously.
    ///
    /// Requirements:
    /// - Tree must be in list_mode
    /// - target_key must exist in the tree
    /// - fetch must be able to retrieve nodes by key
    ///
    /// Algorithm:
    /// 1. Find the node with target_key using fetch
    /// 2. Compute its position using compute_position_with_parent_fetch
    /// 3. Insert new value at position + 1 using insert_at_position_with_key
    ///
    /// Returns the updated tree and the client-provided key (for consistency with other insert methods).
    pub fn insert_after_key_with_key<F>(
        self,
        target_key: &[u8],
        key: Vec<u8>,
        value: Vec<u8>,
        mut fetch: F,
    ) -> CostContext<Result<(Self, Vec<u8>), Error>>
    where
        F: FnMut(&[u8]) -> Option<Self>,
    {
        if !self.list_mode {
            return Err(Error::InternalError("insert_after_key_with_key requires list_mode"))
                .wrap_with_cost(OperationCost::default());
        }

        // Fetch the target node
        let target_node = match fetch(target_key) {
            Some(node) => node,
            None => {
                return Err(Error::InternalError("target key not found in tree"))
                    .wrap_with_cost(OperationCost::default());
            }
        };

        // Compute its position
        let position = match target_node.compute_position_with_parent_fetch(fetch) {
            Some(pos) => pos,
            None => {
                return Err(Error::InternalError("could not compute position for target key"))
                    .wrap_with_cost(OperationCost::default());
            }
        };

        // Insert at position + 1 (after the target) with client-provided key
        self.insert_at_position_with_key(position + 1, key, value)
    }

    /// the node type
    pub fn node_type(&self) -> NodeType {
        self.inner.kv.feature_type.node_type()
    }

    pub fn storage_cost_for_update(current_value_byte_cost: u32, old_cost: u32) -> StorageCost {
        let mut value_storage_cost = StorageCost {
            ..Default::default()
        };

        // Update `StorageCost` for value
        match old_cost.cmp(&current_value_byte_cost) {
            Ordering::Equal => {
                value_storage_cost.replaced_bytes += old_cost;
            }
            Ordering::Greater => {
                // old size is greater than current size, storage_cost will be freed
                value_storage_cost.replaced_bytes += current_value_byte_cost;
                value_storage_cost.removed_bytes +=
                    BasicStorageRemoval(old_cost - current_value_byte_cost);
            }
            Ordering::Less => {
                // current size is greater than old size, storage_cost will be created
                // this also handles the case where the tree.old_size = 0
                value_storage_cost.replaced_bytes += old_cost;
                value_storage_cost.added_bytes += current_value_byte_cost - old_cost;
            }
        }
        value_storage_cost
    }

    /// Compare current value byte cost with old cost and return
    /// current value byte cost with updated `KeyValueStorageCost`
    pub fn kv_with_parent_hook_size_and_storage_cost_from_old_cost(
        &self,
        current_value_byte_cost: u32,
        old_cost: u32,
    ) -> Result<(u32, KeyValueStorageCost), Error> {
        let key_storage_cost = StorageCost {
            ..Default::default()
        };
        let value_storage_cost = Self::storage_cost_for_update(current_value_byte_cost, old_cost);

        let key_value_storage_cost = KeyValueStorageCost {
            key_storage_cost, // the key storage cost is added later
            value_storage_cost,
            new_node: self.old_value.is_none(),
            needs_value_verification: self.inner.kv.value_defined_cost.is_none(),
        };

        Ok((current_value_byte_cost, key_value_storage_cost))
    }

    /// Get current value byte cost and old value byte cost and
    /// compare and return current value byte cost with updated
    /// `KeyValueStorageCost`
    pub fn kv_with_parent_hook_size_and_storage_cost(
        &self,
        old_tree_cost: &impl Fn(&Vec<u8>, &Vec<u8>) -> Result<u32, Error>,
    ) -> Result<(u32, KeyValueStorageCost), Error> {
        let current_value_byte_cost = self.value_encoding_length_with_parent_to_child_reference();

        let old_cost = if let Some(old_value) = self.old_value.as_ref() {
            old_tree_cost(self.key_as_ref(), old_value)
        } else {
            Ok(0) // there was no old value, hence old cost would be 0
        }?;

        self.kv_with_parent_hook_size_and_storage_cost_from_old_cost(
            current_value_byte_cost,
            old_cost,
        )
    }

    /// The point of this function is to get the cost change when we create a
    /// temp value that's a partial merger between the old value and the new
    /// value. Basically it is the new value with the old values flags
    /// For example if we had an old value "Sam" with 40 bytes of flags
    /// and a new value "Samuel" with 2 bytes of flags, the cost is probably
    /// going to go up, As when we merge we will have Samuel with at least
    /// 40 bytes of flags/
    pub fn kv_with_parent_hook_size_and_storage_cost_change_for_value(
        &self,
        old_tree_cost: &impl Fn(&Vec<u8>, &Vec<u8>) -> Result<u32, Error>,
        value: Option<Vec<u8>>,
    ) -> Result<(u32, KeyValueStorageCost), Error> {
        let current_value_byte_cost = if let Some(value_cost) = &self.inner.kv.value_defined_cost {
            self.inner.kv.predefined_value_byte_cost_size(value_cost)
        } else if let Some(value) = value {
            let key_len = self.inner.kv.key.len() as u32;
            let value_len =
                HASH_LENGTH_X2 + value.len() + self.inner.kv.feature_type.encoding_cost();
            KV::value_byte_cost_size_for_key_and_value_lengths(
                key_len,
                value_len as u32,
                self.inner.kv.feature_type.node_type(),
            )
        } else {
            self.inner.kv.value_byte_cost_size()
        };

        let old_cost = if let Some(old_value) = self.old_value.as_ref() {
            old_tree_cost(self.key_as_ref(), old_value)
        } else {
            Ok(0) // there was no old value, hence old cost would be 0
        }?;

        self.kv_with_parent_hook_size_and_storage_cost_from_old_cost(
            current_value_byte_cost,
            old_cost,
        )
    }

    /// Creates a new `Tree` with the given key, value and value hash, and no
    /// children.
    ///
    /// Hashes the key/value pair and initializes the `kv_hash` field.
    pub fn new_with_value_hash(
        key: Vec<u8>,
        value: Vec<u8>,
        value_hash: CryptoHash,
        feature_type: TreeFeatureType,
    ) -> CostContext<Self> {
        KV::new_with_value_hash(key, value, value_hash, feature_type).map(|kv| Self {
            inner: Box::new(TreeNodeInner {
                kv,
                left: None,
                right: None,
            }),
            old_value: None,
            known_storage_cost: None,
            #[cfg(feature = "list_mode")]
            parent_key: None,
            #[cfg(feature = "list_mode")]
            child_side: None,
            #[cfg(feature = "list_mode")]
            list_mode: false,
            #[cfg(feature = "list_mode")]
            subtree_size: 1,
            #[cfg(feature = "list_mode")]
            use_parent_pointers: false,
        })
    }

    /// Creates a new `Tree` with the given key, value and value hash, and no
    /// children.
    /// Sets the tree's value_hash = hash(value, supplied_value_hash)
    pub fn new_with_combined_value_hash(
        key: Vec<u8>,
        value: Vec<u8>,
        value_hash: CryptoHash,
        feature_type: TreeFeatureType,
    ) -> CostContext<Self> {
        KV::new_with_combined_value_hash(key, value, value_hash, feature_type).map(|kv| Self {
            inner: Box::new(TreeNodeInner {
                kv,
                left: None,
                right: None,
            }),
            old_value: None,
            known_storage_cost: None,
            #[cfg(feature = "list_mode")]
            parent_key: None,
            #[cfg(feature = "list_mode")]
            child_side: None,
            #[cfg(feature = "list_mode")]
            list_mode: false,
            #[cfg(feature = "list_mode")]
            subtree_size: 1,
            #[cfg(feature = "list_mode")]
            use_parent_pointers: false,
        })
    }

    /// Creates a new `Tree` with the given key, value, value cost and value
    /// hash, and no children.
    /// Sets the tree's value_hash = hash(value, supplied_value_hash)
    pub fn new_with_layered_value_hash(
        key: Vec<u8>,
        value: Vec<u8>,
        value_cost: u32,
        value_hash: CryptoHash,
        feature_type: TreeFeatureType,
    ) -> CostContext<Self> {
        KV::new_with_layered_value_hash(key, value, value_cost, value_hash, feature_type).map(
            |kv| Self {
                inner: Box::new(TreeNodeInner {
                    kv,
                    left: None,
                    right: None,
                }),
                old_value: None,
                known_storage_cost: None,
                #[cfg(feature = "list_mode")]
                parent_key: None,
                #[cfg(feature = "list_mode")]
                child_side: None,
                #[cfg(feature = "list_mode")]
                list_mode: false,
                #[cfg(feature = "list_mode")]
                subtree_size: 1,
                #[cfg(feature = "list_mode")]
                use_parent_pointers: false,
            },
        )
    }

    /// Creates a `Tree` by supplying all the raw struct fields (mainly useful
    /// for testing). The `kv_hash` and `Link`s are not ensured to be correct.
    pub fn from_fields(
        key: Vec<u8>,
        value: Vec<u8>,
        kv_hash: CryptoHash,
        left: Option<Link>,
        right: Option<Link>,
        feature_type: TreeFeatureType,
    ) -> CostContext<Self> {
        value_hash(value.as_slice()).map(|vh| Self {
            inner: Box::new(TreeNodeInner {
                kv: KV::from_fields(key, value, kv_hash, vh, feature_type),
                left,
                right,
            }),
            old_value: None,
            known_storage_cost: None,
            #[cfg(feature = "list_mode")]
            parent_key: None,
            #[cfg(feature = "list_mode")]
            child_side: None,
            #[cfg(feature = "list_mode")]
            list_mode: false,
            #[cfg(feature = "list_mode")]
            subtree_size: 1,
            #[cfg(feature = "list_mode")]
            use_parent_pointers: false,
        })
    }

    /// Returns the root node's key as a slice.
    #[inline]
    pub fn key(&self) -> &[u8] {
        self.inner.kv.key()
    }

    /// Returns the root node's feature type
    #[inline]
    pub fn feature_type(&self) -> TreeFeatureType {
        self.inner.kv.feature_type
    }

    /// Returns the root node's key as a slice.
    #[inline]
    pub fn key_as_ref(&self) -> &Vec<u8> {
        self.inner.kv.key_as_ref()
    }

    /// Set key of Tree
    pub fn set_key(&mut self, key: Vec<u8>) {
        self.inner.kv.key = key;
    }

    /// Set value of Tree
    pub fn set_value(&mut self, value: Vec<u8>) {
        self.inner.kv.value = value;
    }

    /// Consumes the tree and returns its root node's key, without having to
    /// clone or allocate.
    #[inline]
    pub fn take_key(self) -> Vec<u8> {
        self.inner.kv.take_key()
    }

    /// Returns the root node's value as a ref.
    #[inline]
    pub fn value_ref(&self) -> &Vec<u8> {
        self.inner.kv.value.as_ref()
    }

    /// Returns the root node's value as a ref.
    #[inline]
    pub fn value_mut_ref(&mut self) -> &mut Vec<u8> {
        &mut self.inner.kv.value
    }

    /// Returns the root node's value as a slice.
    #[inline]
    pub fn value_as_slice(&self) -> &[u8] {
        self.inner.kv.value_as_slice()
    }

    /// Returns the hash of the root node's key/value pair.
    #[inline]
    pub const fn kv_hash(&self) -> &CryptoHash {
        self.inner.kv.hash()
    }

    /// Returns the hash of the node's valu
    #[inline]
    pub const fn value_hash(&self) -> &CryptoHash {
        self.inner.kv.value_hash()
    }

    /// Returns a reference to the root node's `Link` on the given side, if any.
    /// If there is no child, returns `None`.
    #[inline]
    pub const fn link(&self, left: bool) -> Option<&Link> {
        if left {
            self.inner.left.as_ref()
        } else {
            self.inner.right.as_ref()
        }
    }

    /// Returns a mutable reference to the root node's `Link` on the given side,
    /// if any. If there is no child, returns `None`.
    #[inline]
    pub fn link_mut(&mut self, left: bool) -> Option<&mut Link> {
        if left {
            self.inner.left.as_mut()
        } else {
            self.inner.right.as_mut()
        }
    }

    /// Returns a the size of node's child key and sum on the given side, if
    /// any. If there is no child, returns `None`.
    pub fn child_ref_and_sum_size(&self, left: bool) -> Option<(u32, u32)> {
        self.link(left).map(|link| {
            (
                // 36 = 32 Hash + 1 key length + 2 child heights + 1 feature type
                link.key().len() as u32 + 36,
                match link.aggregate_data() {
                    AggregateData::NoAggregateData => 0,
                    AggregateData::Sum(s) => s.encode_var_vec().len() as u32,
                    AggregateData::BigSum(_) => 16_u32,
                    AggregateData::Count(c) => c.encode_var_vec().len() as u32,
                    AggregateData::CountAndSum(c, s) => {
                        s.encode_var_vec().len() as u32 + c.encode_var_vec().len() as u32
                    }
                },
            )
        })
    }

    /// Returns a reference to the root node's child on the given side, if any.
    /// If there is no child, returns `None`.
    #[inline]
    pub const fn child(&self, left: bool) -> Option<&Self> {
        match self.link(left) {
            None => None,
            Some(link) => link.tree(),
        }
    }

    /// Returns a mutable reference to the root node's child on the given side,
    /// if any. If there is no child, returns `None`.
    #[inline]
    pub fn child_mut(&mut self, left: bool) -> Option<&mut Self> {
        match self.slot_mut(left).as_mut() {
            None => None,
            Some(Link::Reference { .. }) => None,
            Some(Link::Modified { tree, .. }) => Some(tree),
            Some(Link::Uncommitted { tree, .. }) => Some(tree),
            Some(Link::Loaded { tree, .. }) => Some(tree),
        }
    }

    /// Returns the hash of the root node's child on the given side, if any. If
    /// there is no child, returns the null hash (zero-filled).
    #[inline]
    pub const fn child_hash(&self, left: bool) -> &CryptoHash {
        match self.link(left) {
            Some(link) => link.hash(),
            _ => &NULL_HASH,
        }
    }

    /// Returns the sum of the root node's child on the given side, if any. If
    /// there is no child, returns 0.
    #[inline]
    pub fn child_aggregate_sum_data_as_i64(&self, left: bool) -> Result<i64, Error> {
        match self.link(left) {
            Some(link) => match link.aggregate_data() {
                AggregateData::NoAggregateData => Ok(0),
                AggregateData::Sum(s) => Ok(s),
                AggregateData::BigSum(_) => Err(Error::BigSumTreeUnderNormalSumTree(
                    "for aggregate data as i64".to_string(),
                )),
                AggregateData::Count(_) => Ok(0),
                AggregateData::CountAndSum(_, s) => Ok(s),
            },
            _ => Ok(0),
        }
    }

    /// Returns the sum of the root node's child on the given side, if any. If
    /// there is no child, returns 0.
    #[inline]
    pub fn child_aggregate_count_data_as_u64(&self, left: bool) -> Result<u64, Error> {
        match self.link(left) {
            Some(link) => match link.aggregate_data() {
                AggregateData::NoAggregateData => Ok(0),
                AggregateData::Sum(_) => Ok(0),
                AggregateData::BigSum(_) => Ok(0),
                AggregateData::Count(c) => Ok(c),
                AggregateData::CountAndSum(c, _) => Ok(c),
            },
            _ => Ok(0),
        }
    }

    /// Returns the sum of the root node's child on the given side, if any. If
    /// there is no child, returns 0.
    #[inline]
    pub fn child_aggregate_sum_data_as_i128(&self, left: bool) -> i128 {
        match self.link(left) {
            Some(link) => match link.aggregate_data() {
                AggregateData::NoAggregateData => 0,
                AggregateData::Sum(s) => s as i128,
                AggregateData::BigSum(s) => s,
                AggregateData::Count(_) => 0,
                AggregateData::CountAndSum(_, s) => s as i128,
            },
            _ => 0,
        }
    }

    /// Computes and returns the hash of the root node.
    #[inline]
    pub fn hash(&self) -> CostContext<CryptoHash> {
        #[cfg(feature = "list_mode")]
        if self.list_mode {
            // Only include parent_key in hash if use_parent_pointers is enabled
            let parent_key_for_hash = if self.use_parent_pointers {
                &self.parent_key
            } else {
                &None
            };
            
            return node_hash_list_mode(
                self.inner.kv.hash(),
                self.child_hash(true),
                self.child_hash(false),
                self.subtree_size(),
                parent_key_for_hash,
            );
        }
        node_hash(
            self.inner.kv.hash(),
            self.child_hash(true),
            self.child_hash(false),
        )
    }

    /// Computes and returns the hash of the root node.
    #[inline]
    pub fn aggregate_data(&self) -> Result<AggregateData, Error> {
        match self.inner.kv.feature_type {
            TreeFeatureType::BasicMerkNode => Ok(AggregateData::NoAggregateData),
            TreeFeatureType::SummedMerkNode(value) => {
                let left = self.child_aggregate_sum_data_as_i64(true)?;
                let right = self.child_aggregate_sum_data_as_i64(false)?;
                value
                    .checked_add(left)
                    .and_then(|a| a.checked_add(right))
                    .ok_or(Overflow("sum is overflowing"))
                    .map(AggregateData::Sum)
            }
            TreeFeatureType::BigSummedMerkNode(value) => value
                .checked_add(self.child_aggregate_sum_data_as_i128(true))
                .and_then(|a| a.checked_add(self.child_aggregate_sum_data_as_i128(false)))
                .ok_or(Overflow("big sum is overflowing"))
                .map(AggregateData::BigSum),
            TreeFeatureType::CountedMerkNode(value) => {
                let left = self.child_aggregate_count_data_as_u64(true)?;
                let right = self.child_aggregate_count_data_as_u64(false)?;
                value
                    .checked_add(left)
                    .and_then(|a| a.checked_add(right))
                    .ok_or(Overflow("count is overflowing"))
                    .map(AggregateData::Count)
            }
            TreeFeatureType::CountedSummedMerkNode(count_value, sum_value) => {
                let left_count = self.child_aggregate_count_data_as_u64(true)?;
                let right_count = self.child_aggregate_count_data_as_u64(false)?;
                let left_sum = self.child_aggregate_sum_data_as_i64(true)?;
                let right_sum = self.child_aggregate_sum_data_as_i64(false)?;
                let aggregated_count_value = count_value
                    .checked_add(left_count)
                    .and_then(|a| a.checked_add(right_count))
                    .ok_or(Overflow("count is overflowing"))?;

                let aggregated_sum_value = sum_value
                    .checked_add(left_sum)
                    .and_then(|a| a.checked_add(right_sum))
                    .ok_or(Overflow("count is overflowing"))?;

                Ok(AggregateData::CountAndSum(
                    aggregated_count_value,
                    aggregated_sum_value,
                ))
            }
        }
    }

    /// Returns the number of pending writes for the child on the given side, if
    /// any. If there is no child, returns 0.
    #[inline]
    pub const fn child_pending_writes(&self, left: bool) -> usize {
        match self.link(left) {
            Some(Link::Modified { pending_writes, .. }) => *pending_writes,
            _ => 0,
        }
    }

    /// Returns the height of the child on the given side, if any. If there is
    /// no child, returns 0.
    #[inline]
    pub const fn child_height(&self, left: bool) -> u8 {
        match self.link(left) {
            Some(child) => child.height(),
            _ => 0,
        }
    }

    #[inline]
    /// Return the child heights of self
    pub const fn child_heights(&self) -> (u8, u8) {
        (self.child_height(true), self.child_height(false))
    }

    /// Returns the height of the tree (the number of levels). For example, a
    /// single node has height 1, a node with a single descendant has height 2,
    /// etc.
    #[inline]
    pub fn height(&self) -> u8 {
        1 + max(self.child_height(true), self.child_height(false))
    }

    /// Returns the balance factor of the root node. This is the difference
    /// between the height of the right child (if any) and the height of the
    /// left child (if any). For example, a balance factor of 2 means the right
    /// subtree is 2 levels taller than the left subtree.
    #[inline]
    pub const fn balance_factor(&self) -> i8 {
        let left_height = self.child_height(true) as i8;
        let right_height = self.child_height(false) as i8;
        right_height - left_height
    }

    #[cfg(feature = "list_mode")]
    /// Perform a right rotation around this node (for list_mode).
    ///
    /// This is used when the left subtree is too tall (balance factor < -1).
    ///
    /// Before:
    ///       y
    ///      / \
    ///     x   C
    ///    / \
    ///   A   B
    ///
    /// After:
    ///       x
    ///      / \
    ///     A   y
    ///        / \
    ///       B   C
    ///
    /// Returns the new root (x).
    /// Updates parent pointers and subtree_size if in list_mode.
    pub fn rotate_right(self) -> Self {
        // Detach left child (x)
        let (y, maybe_x) = self.detach(true);
        let x = maybe_x.expect("rotate_right requires left child");
        
        // Detach B from x
        let (x, maybe_b) = x.detach(false);
        
        // Attach B to y's left
        let y = y.attach(true, maybe_b);
        
        // Attach y to x's right
        let x = x.attach(false, Some(y));
        
        x
    }

    #[cfg(feature = "list_mode")]
    /// Perform a left rotation around this node (for list_mode).
    ///
    /// This is used when the right subtree is too tall (balance factor > 1).
    ///
    /// Before:
    ///       x
    ///      / \
    ///     A   y
    ///        / \
    ///       B   C
    ///
    /// After:
    ///       y
    ///      / \
    ///     x   C
    ///    / \
    ///   A   B
    ///
    /// Returns the new root (y).
    /// Updates parent pointers and subtree_size if in list_mode.
    pub fn rotate_left(self) -> Self {
        // Detach right child (y)
        let (x, maybe_y) = self.detach(false);
        let y = maybe_y.expect("rotate_left requires right child");
        
        // Detach B from y
        let (y, maybe_b) = y.detach(true);
        
        // Attach B to x's right
        let x = x.attach(false, maybe_b);
        
        // Attach x to y's left
        let y = y.attach(true, Some(x));
        
        y
    }

    #[cfg(feature = "list_mode")]
    /// Balance this node if needed for list_mode AVL tree.
    ///
    /// Checks the balance factor and performs rotations if necessary to
    /// maintain AVL property (|balance_factor| <= 1).
    ///
    /// Returns the (potentially new) root of this subtree after balancing.
    pub fn balance(self) -> Self {
        if !self.list_mode {
            return self;
        }

        let bf = self.balance_factor();
        
        if bf < -1 {
            // Left-heavy
            let left_bf = self.child(true).map_or(0, |c| c.balance_factor());
            if left_bf > 0 {
                // Left-Right case: rotate left child left first
                let (node, maybe_left) = self.detach(true);
                let left = maybe_left.expect("balance: left child should exist");
                let rotated_left = left.rotate_left();
                let node = node.attach(true, Some(rotated_left));
                node.rotate_right()
            } else {
                // Left-Left case: simple right rotation
                self.rotate_right()
            }
        } else if bf > 1 {
            // Right-heavy
            let right_bf = self.child(false).map_or(0, |c| c.balance_factor());
            if right_bf < 0 {
                // Right-Left case: rotate right child right first
                let (node, maybe_right) = self.detach(false);
                let right = maybe_right.expect("balance: right child should exist");
                let rotated_right = right.rotate_right();
                let node = node.attach(false, Some(rotated_right));
                node.rotate_left()
            } else {
                // Right-Right case: simple left rotation
                self.rotate_left()
            }
        } else {
            // Already balanced
            self
        }
    }

    /// Attaches the child (if any) to the root node on the given side. Creates
    /// a `Link` of variant `Link::Modified` which contains the child.
    ///
    /// Panics if there is already a child on the given side.
    #[inline]
    pub fn attach(mut self, left: bool, maybe_child: Option<Self>) -> Self {
        debug_assert_ne!(
            Some(self.key()),
            maybe_child.as_ref().map(|c| c.key()),
            "Tried to attach tree with same key"
        );

        // let parent = std::str::from_utf8(self.key());
        // if maybe_child.is_some(){
        //     let child = std::str::from_utf8(maybe_child.as_ref().unwrap().key());
        //     println!("attaching {} to {}", child.unwrap(), parent.unwrap());
        // } else {
        //     println!("attaching nothing to {}", parent.unwrap());
        // }

    // Capture parent key before mutable borrow to satisfy borrow checker when setting child pointer
    #[cfg(feature = "list_mode")]
    let parent_key_snapshot = self.key().to_vec();
    let slot = self.slot_mut(left);

        if slot.is_some() {
            panic!(
                "Tried to attach to {} tree slot, but it is already Some",
                side_to_str(left)
            );
        }
    #[cfg(feature = "list_mode")]
    let maybe_child = maybe_child.map(|mut child| { 
        if child.use_parent_pointers {
            child.set_parent_pointer(&parent_key_snapshot, left);
        }
        child 
    });
        *slot = Link::maybe_from_modified_tree(maybe_child);
    #[cfg(feature = "list_mode")]
    if self.list_mode { self.recompute_subtree_size(); }

        self
    }

    /// Detaches the child on the given side (if any) from the root node, and
    /// returns `(root_node, maybe_child)`.
    ///
    /// One will usually want to reattach (see `attach`) a child on the same
    /// side after applying some operation to the detached child.
    #[inline]
    pub fn detach(mut self, left: bool) -> (Self, Option<Self>) {
    let mut maybe_child = match self.slot_mut(left).take() {
            None => None,
            Some(Link::Reference { .. }) => None,
            Some(Link::Modified { tree, .. }) => Some(tree),
            Some(Link::Uncommitted { tree, .. }) => Some(tree),
            Some(Link::Loaded { tree, .. }) => Some(tree),
        };
        #[cfg(feature = "list_mode")]
        if let Some(c) = maybe_child.as_mut() { c.clear_parent_pointer(); }
        // println!("detaching {}",
        // std::str::from_utf8(maybe_child.as_ref().unwrap().key()).unwrap());

        (self, maybe_child)
    }

    /// Detaches the child on the given side from the root node, and
    /// returns `(root_node, child)`.
    ///
    /// Panics if there is no child on the given side.
    ///
    /// One will usually want to reattach (see `attach`) a child on the same
    /// side after applying some operation to the detached child.
    #[inline]
    pub fn detach_expect(self, left: bool) -> (Self, Self) {
        let (parent, maybe_child) = self.detach(left);

        if let Some(child) = maybe_child {
            (parent, child)
        } else {
            panic!(
                "Expected tree to have {} child, but got None",
                side_to_str(left)
            );
        }
    }

    /// Detaches the child on the given side and passes it into `f`, which must
    /// return a new child (either the same child, a new child to take its
    /// place, or `None` to explicitly keep the slot empty).
    ///
    /// This is the same as `detach`, but with the function interface to enforce
    /// at compile-time that an explicit final child value is returned. This is
    /// less error prone that detaching with `detach` and reattaching with
    /// `attach`.
    #[inline]
    pub fn walk<F>(self, left: bool, f: F) -> Self
    where
        F: FnOnce(Option<Self>) -> Option<Self>,
    {
        let (tree, maybe_child) = self.detach(left);
        // apply f to detached child; then reattach
        tree.attach(left, f(maybe_child))
    }

    /// Like `walk`, but panics if there is no child on the given side.
    #[inline]
    pub fn walk_expect<F>(self, left: bool, f: F) -> Self
    where
        F: FnOnce(Self) -> Option<Self>,
    {
        let (tree, child) = self.detach_expect(left);
        tree.attach(left, f(child))
    }

    /// Returns a mutable reference to the child slot for the given side.
    #[inline]
    pub(crate) fn slot_mut(&mut self, left: bool) -> &mut Option<Link> {
        if left {
            &mut self.inner.left
        } else {
            &mut self.inner.right
        }
    }

    /// Replaces the root node's value with the given value and returns the
    /// modified `Tree`.
    #[inline]
    pub fn put_value(
        mut self,
        value: Vec<u8>,
        feature_type: TreeFeatureType,
        old_specialized_cost: &impl Fn(&Vec<u8>, &Vec<u8>) -> Result<u32, Error>,
        get_temp_new_value_with_old_flags: &impl Fn(
            &Vec<u8>,
            &Vec<u8>,
        ) -> Result<Option<Vec<u8>>, Error>,
        update_tree_value_based_on_costs: &mut impl FnMut(
            &StorageCost,
            &Vec<u8>,
            &mut Vec<u8>,
        ) -> Result<
            (bool, Option<ValueDefinedCostType>),
            Error,
        >,
        section_removal_bytes: &mut impl FnMut(
            &Vec<u8>,
            u32,
            u32,
        ) -> Result<
            (StorageRemovedBytes, StorageRemovedBytes),
            Error,
        >,
    ) -> CostResult<Self, Error> {
        let mut cost = OperationCost::default();

        self.inner.kv = self.inner.kv.put_value_no_update_of_hashes(value);
        self.inner.kv.feature_type = feature_type;

        if self.old_value.is_some() {
            // we are replacing a value
            // in this case there is a possibility that the client would want to update the
            // element flags based on the change of values
            cost_return_on_error_no_add!(
                cost,
                self.just_in_time_tree_node_value_update(
                    old_specialized_cost,
                    get_temp_new_value_with_old_flags,
                    update_tree_value_based_on_costs,
                    section_removal_bytes
                )
            );
        }

        self.inner.kv = self.inner.kv.update_hashes().unwrap_add_cost(&mut cost);
        Ok(self).wrap_with_cost(cost)
    }

    /// Replaces the root node's value with the given value and returns the
    /// modified `Tree`.
    #[inline]
    pub fn put_value_with_fixed_cost(
        mut self,
        value: Vec<u8>,
        value_fixed_cost: u32,
        feature_type: TreeFeatureType,
        old_specialized_cost: &impl Fn(&Vec<u8>, &Vec<u8>) -> Result<u32, Error>,
        get_temp_new_value_with_old_flags: &impl Fn(
            &Vec<u8>,
            &Vec<u8>,
        ) -> Result<Option<Vec<u8>>, Error>,
        update_tree_value_based_on_costs: &mut impl FnMut(
            &StorageCost,
            &Vec<u8>,
            &mut Vec<u8>,
        ) -> Result<
            (bool, Option<ValueDefinedCostType>),
            Error,
        >,
        section_removal_bytes: &mut impl FnMut(
            &Vec<u8>,
            u32,
            u32,
        ) -> Result<
            (StorageRemovedBytes, StorageRemovedBytes),
            Error,
        >,
    ) -> CostResult<Self, Error> {
        let mut cost = OperationCost::default();
        self.inner.kv = self.inner.kv.put_value_with_fixed_cost_no_update_of_hashes(
            value,
            SpecializedValueDefinedCost(value_fixed_cost),
        );
        self.inner.kv.feature_type = feature_type;

        if self.old_value.is_some() {
            // we are replacing a value
            // in this case there is a possibility that the client would want to update the
            // element flags based on the change of values
            cost_return_on_error_no_add!(
                cost,
                self.just_in_time_tree_node_value_update(
                    old_specialized_cost,
                    get_temp_new_value_with_old_flags,
                    update_tree_value_based_on_costs,
                    section_removal_bytes
                )
            );
        }

        self.inner.kv = self.inner.kv.update_hashes().unwrap_add_cost(&mut cost);
        Ok(self).wrap_with_cost(cost)
    }

    /// Replaces the root node's value with the given value and value hash
    /// and returns the modified `Tree`.
    #[inline]
    pub fn put_value_and_reference_value_hash(
        mut self,
        value: Vec<u8>,
        value_hash: CryptoHash,
        feature_type: TreeFeatureType,
        old_specialized_cost: &impl Fn(&Vec<u8>, &Vec<u8>) -> Result<u32, Error>,
        get_temp_new_value_with_old_flags: &impl Fn(
            &Vec<u8>,
            &Vec<u8>,
        ) -> Result<Option<Vec<u8>>, Error>,
        update_tree_value_based_on_costs: &mut impl FnMut(
            &StorageCost,
            &Vec<u8>,
            &mut Vec<u8>,
        ) -> Result<
            (bool, Option<ValueDefinedCostType>),
            Error,
        >,
        section_removal_bytes: &mut impl FnMut(
            &Vec<u8>,
            u32,
            u32,
        ) -> Result<
            (StorageRemovedBytes, StorageRemovedBytes),
            Error,
        >,
    ) -> CostResult<Self, Error> {
        let mut cost = OperationCost::default();

        self.inner.kv = self.inner.kv.put_value_no_update_of_hashes(value);
        self.inner.kv.feature_type = feature_type;

        if self.old_value.is_some() {
            // we are replacing a value
            // in this case there is a possibility that the client would want to update the
            // element flags based on the change of values
            cost_return_on_error_no_add!(
                cost,
                self.just_in_time_tree_node_value_update(
                    old_specialized_cost,
                    get_temp_new_value_with_old_flags,
                    update_tree_value_based_on_costs,
                    section_removal_bytes
                )
            );
        }

        self.inner.kv = self
            .inner
            .kv
            .update_hashes_using_reference_value_hash(value_hash)
            .unwrap_add_cost(&mut cost);
        Ok(self).wrap_with_cost(cost)
    }

    /// Replaces the root node's value with the given value and value hash
    /// and returns the modified `Tree`.
    #[inline]
    pub fn put_value_with_reference_value_hash_and_value_cost(
        mut self,
        value: Vec<u8>,
        value_hash: CryptoHash,
        value_cost: u32,
        feature_type: TreeFeatureType,
        old_specialized_cost: &impl Fn(&Vec<u8>, &Vec<u8>) -> Result<u32, Error>,
        get_temp_new_value_with_old_flags: &impl Fn(
            &Vec<u8>,
            &Vec<u8>,
        ) -> Result<Option<Vec<u8>>, Error>,
        update_tree_value_based_on_costs: &mut impl FnMut(
            &StorageCost,
            &Vec<u8>,
            &mut Vec<u8>,
        ) -> Result<
            (bool, Option<ValueDefinedCostType>),
            Error,
        >,
        section_removal_bytes: &mut impl FnMut(
            &Vec<u8>,
            u32,
            u32,
        ) -> Result<
            (StorageRemovedBytes, StorageRemovedBytes),
            Error,
        >,
    ) -> CostResult<Self, Error> {
        let mut cost = OperationCost::default();

        self.inner.kv = self.inner.kv.put_value_with_fixed_cost_no_update_of_hashes(
            value,
            LayeredValueDefinedCost(value_cost),
        );
        self.inner.kv.feature_type = feature_type;

        if self.old_value.is_some() {
            // we are replacing a value
            // in this case there is a possibility that the client would want to update the
            // element flags based on the change of values
            cost_return_on_error_no_add!(
                cost,
                self.just_in_time_tree_node_value_update(
                    old_specialized_cost,
                    get_temp_new_value_with_old_flags,
                    update_tree_value_based_on_costs,
                    section_removal_bytes
                )
            );
        }

        self.inner.kv = self
            .inner
            .kv
            .update_hashes_using_reference_value_hash(value_hash)
            .unwrap_add_cost(&mut cost);
        Ok(self).wrap_with_cost(cost)
    }

    // TODO: add compute_hashes method

    /// Called to finalize modifications to a tree, recompute its hashes, and
    /// write the updated nodes to a backing store.
    ///
    /// Traverses through the tree, computing hashes for all modified links and
    /// replacing them with `Link::Loaded` variants, writes out all changes to
    /// the given `Commit` object's `write` method, and calls the its `prune`
    /// method to test whether or not to keep or prune nodes from memory.
    pub fn commit<C: Commit>(
        &mut self,
        c: &mut C,
        old_specialized_cost: &impl Fn(&Vec<u8>, &Vec<u8>) -> Result<u32, Error>,
    ) -> CostResult<(), Error> {
        // TODO: make this method less ugly
        // TODO: call write in-order for better performance in writing batch to db?

        // println!("about to commit {}", std::str::from_utf8(self.key()).unwrap());
        let mut cost = OperationCost::default();

        if let Some(Link::Modified { .. }) = self.inner.left {
            // println!("left is modified");
            if let Some(Link::Modified {
                mut tree,
                child_heights,
                ..
            }) = self.inner.left.take()
            {
                // println!("key is {}", std::str::from_utf8(tree.key()).unwrap());
                cost_return_on_error!(&mut cost, tree.commit(c, old_specialized_cost,));
                let aggregate_data = cost_return_on_error_default!(tree.aggregate_data());

                self.inner.left = Some(Link::Loaded {
                    hash: tree.hash().unwrap_add_cost(&mut cost),
                    tree,
                    child_heights,
                    aggregate_data,
                });
            } else {
                unreachable!()
            }
        }

        if let Some(Link::Modified { .. }) = self.inner.right {
            // println!("right is modified");
            if let Some(Link::Modified {
                mut tree,
                child_heights,
                ..
            }) = self.inner.right.take()
            {
                // println!("key is {}", std::str::from_utf8(tree.key()).unwrap());
                cost_return_on_error!(&mut cost, tree.commit(c, old_specialized_cost,));
                let aggregate_data = cost_return_on_error_default!(tree.aggregate_data());
                self.inner.right = Some(Link::Loaded {
                    hash: tree.hash().unwrap_add_cost(&mut cost),
                    tree,
                    child_heights,
                    aggregate_data,
                });
            } else {
                unreachable!()
            }
        }

        cost_return_on_error_no_add!(cost, c.write(self, old_specialized_cost,));

        // println!("done committing {}", std::str::from_utf8(self.key()).unwrap());

        let (prune_left, prune_right) = c.prune(self);
        if prune_left {
            self.inner.left = self.inner.left.take().map(|link| link.into_reference());
        }
        if prune_right {
            self.inner.right = self.inner.right.take().map(|link| link.into_reference());
        }

        Ok(()).wrap_with_cost(cost)
    }

    /// Fetches the child on the given side using the given data source, and
    /// places it in the child slot (upgrading the link from `Link::Reference`
    /// to `Link::Loaded`).
    pub fn load<S: Fetch, V>(
        &mut self,
        left: bool,
        source: &S,
        value_defined_cost_fn: Option<&V>,
        grove_version: &GroveVersion,
    ) -> CostResult<(), Error>
    where
        V: Fn(&[u8], &GroveVersion) -> Option<ValueDefinedCostType>,
    {
        // TODO: return Err instead of panic?
        let link = self.link(left).expect("Expected link");
        let (child_heights, hash, aggregate_data) = match link {
            Link::Reference {
                child_heights,
                hash,
                aggregate_data,
                ..
            } => (child_heights, hash, aggregate_data),
            _ => panic!("Expected Some(Link::Reference)"),
        };

        let mut cost = OperationCost::default();
        let tree = cost_return_on_error!(
            &mut cost,
            source.fetch(link, value_defined_cost_fn, grove_version)
        );
        debug_assert_eq!(tree.key(), link.key());
        *self.slot_mut(left) = Some(Link::Loaded {
            tree,
            hash: *hash,
            child_heights: *child_heights,
            aggregate_data: *aggregate_data,
        });
        Ok(()).wrap_with_cost(cost)
    }
}

#[cfg(feature = "minimal")]
/// Convert side (left or right) to string
pub const fn side_to_str(left: bool) -> &'static str {
    if left {
        "left"
    } else {
        "right"
    }
}

#[cfg(feature = "minimal")]
#[cfg(test)]
mod test {

    use super::{commit::NoopCommit, hash::NULL_HASH, AggregateData, TreeNode};
    use crate::tree::{
        tree_feature_type::TreeFeatureType::SummedMerkNode, TreeFeatureType::BasicMerkNode,
    };

    #[test]
    fn build_tree() {
        let tree = TreeNode::new(vec![1], vec![101], None, BasicMerkNode).unwrap();
        assert_eq!(tree.key(), &[1]);
        assert_eq!(tree.value_as_slice(), &[101]);
        assert!(tree.child(true).is_none());
        assert!(tree.child(false).is_none());

        let tree = tree.attach(true, None);
        assert!(tree.child(true).is_none());
        assert!(tree.child(false).is_none());

        let tree = tree.attach(
            true,
            Some(TreeNode::new(vec![2], vec![102], None, BasicMerkNode).unwrap()),
        );
        assert_eq!(tree.key(), &[1]);
        assert_eq!(tree.child(true).unwrap().key(), &[2]);
        assert!(tree.child(false).is_none());

        let tree = TreeNode::new(vec![3], vec![103], None, BasicMerkNode)
            .unwrap()
            .attach(false, Some(tree));
        assert_eq!(tree.key(), &[3]);
        assert_eq!(tree.child(false).unwrap().key(), &[1]);
        assert!(tree.child(true).is_none());
    }

    #[should_panic]
    #[test]
    fn attach_existing() {
        TreeNode::new(vec![0], vec![1], None, BasicMerkNode)
            .unwrap()
            .attach(
                true,
                Some(TreeNode::new(vec![2], vec![3], None, BasicMerkNode).unwrap()),
            )
            .attach(
                true,
                Some(TreeNode::new(vec![4], vec![5], None, BasicMerkNode).unwrap()),
            );
    }

    #[test]
    fn modify() {
        let tree = TreeNode::new(vec![0], vec![1], None, BasicMerkNode)
            .unwrap()
            .attach(
                true,
                Some(TreeNode::new(vec![2], vec![3], None, BasicMerkNode).unwrap()),
            )
            .attach(
                false,
                Some(TreeNode::new(vec![4], vec![5], None, BasicMerkNode).unwrap()),
            );

        let tree = tree.walk(true, |left_opt| {
            assert_eq!(left_opt.as_ref().unwrap().key(), &[2]);
            None
        });
        assert!(tree.child(true).is_none());
        assert!(tree.child(false).is_some());

        let tree = tree.walk(true, |left_opt| {
            assert!(left_opt.is_none());
            Some(TreeNode::new(vec![2], vec![3], None, BasicMerkNode).unwrap())
        });
        assert_eq!(tree.link(true).unwrap().key(), &[2]);

        let tree = tree.walk_expect(false, |right| {
            assert_eq!(right.key(), &[4]);
            None
        });
        assert!(tree.child(true).is_some());
        assert!(tree.child(false).is_none());
    }

    #[test]
    fn child_and_link() {
        let mut tree = TreeNode::new(vec![0], vec![1], None, BasicMerkNode)
            .unwrap()
            .attach(
                true,
                Some(TreeNode::new(vec![2], vec![3], None, BasicMerkNode).unwrap()),
            );
        assert!(tree.link(true).expect("expected link").is_modified());
        assert!(tree.child(true).is_some());
        assert!(tree.link(false).is_none());
        assert!(tree.child(false).is_none());

        tree.commit(&mut NoopCommit {}, &|_, _| Ok(0))
            .unwrap()
            .expect("commit failed");
        assert!(tree.link(true).expect("expected link").is_stored());
        assert!(tree.child(true).is_some());

        // tree.link(true).prune(true);
        // assert!(tree.link(true).expect("expected link").is_pruned());
        // assert!(tree.child(true).is_none());

        let tree = tree.walk(true, |_| None);
        assert!(tree.link(true).is_none());
        assert!(tree.child(true).is_none());
    }

    #[test]
    fn child_hash() {
        let mut tree = TreeNode::new(vec![0], vec![1], None, BasicMerkNode)
            .unwrap()
            .attach(
                true,
                Some(TreeNode::new(vec![2], vec![3], None, BasicMerkNode).unwrap()),
            );
        tree.commit(&mut NoopCommit {}, &|_, _| Ok(0))
            .unwrap()
            .expect("commit failed");
        assert_eq!(
            tree.child_hash(true),
            &[
                132, 211, 39, 192, 19, 164, 57, 106, 128, 9, 35, 145, 86, 12, 57, 192, 239, 69,
                113, 148, 33, 220, 206, 207, 237, 199, 214, 241, 97, 144, 224, 185
            ]
        );
        assert_eq!(tree.child_hash(false), &NULL_HASH);
    }

    #[test]
    fn hash() {
        let tree = TreeNode::new(vec![0], vec![1], None, BasicMerkNode).unwrap();
        assert_eq!(
            tree.hash().unwrap(),
            [
                10, 108, 153, 163, 54, 173, 62, 155, 228, 204, 102, 172, 158, 203, 197, 126, 230,
                234, 97, 110, 227, 208, 64, 21, 65, 8, 82, 2, 241, 122, 66, 207
            ]
        );
    }

    #[test]
    fn child_pending_writes() {
        let tree = TreeNode::new(vec![0], vec![1], None, BasicMerkNode).unwrap();
        assert_eq!(tree.child_pending_writes(true), 0);
        assert_eq!(tree.child_pending_writes(false), 0);

        let tree = tree.attach(
            true,
            Some(TreeNode::new(vec![2], vec![3], None, BasicMerkNode).unwrap()),
        );
        assert_eq!(tree.child_pending_writes(true), 1);
        assert_eq!(tree.child_pending_writes(false), 0);
    }

    #[test]
    fn height_and_balance() {
        let tree = TreeNode::new(vec![0], vec![1], None, BasicMerkNode).unwrap();
        assert_eq!(tree.height(), 1);
        assert_eq!(tree.child_height(true), 0);
        assert_eq!(tree.child_height(false), 0);
        assert_eq!(tree.balance_factor(), 0);

        let tree = tree.attach(
            true,
            Some(TreeNode::new(vec![2], vec![3], None, BasicMerkNode).unwrap()),
        );
        assert_eq!(tree.height(), 2);
        assert_eq!(tree.child_height(true), 1);
        assert_eq!(tree.child_height(false), 0);
        assert_eq!(tree.balance_factor(), -1);

        let (tree, maybe_child) = tree.detach(true);
        let tree = tree.attach(false, maybe_child);
        assert_eq!(tree.height(), 2);
        assert_eq!(tree.child_height(true), 0);
        assert_eq!(tree.child_height(false), 1);
        assert_eq!(tree.balance_factor(), 1);
    }

    #[test]
    fn commit() {
        let mut tree = TreeNode::new(vec![0], vec![1], None, BasicMerkNode)
            .unwrap()
            .attach(
                false,
                Some(TreeNode::new(vec![2], vec![3], None, BasicMerkNode).unwrap()),
            );
        tree.commit(&mut NoopCommit {}, &|_, _| Ok(0))
            .unwrap()
            .expect("commit failed");

        assert!(tree.link(false).expect("expected link").is_stored());
    }

    #[test]
    fn sum_tree() {
        let mut tree = TreeNode::new(vec![0], vec![1], None, SummedMerkNode(3))
            .unwrap()
            .attach(
                false,
                Some(TreeNode::new(vec![2], vec![3], None, SummedMerkNode(5)).unwrap()),
            );
        tree.commit(&mut NoopCommit {}, &|_, _| Ok(0))
            .unwrap()
            .expect("commit failed");

        assert_eq!(
            AggregateData::Sum(8),
            tree.aggregate_data()
                .expect("expected to get sum from tree")
        );
    }

    #[cfg(feature = "list_mode")]
    #[test]
    fn list_mode_parent_pointers_and_positions() {
        use std::collections::HashMap;

        // Create three list nodes
        let n1 = TreeNode::new_list_node(vec![b'a']).unwrap();
        let n2 = TreeNode::new_list_node(vec![b'b']).unwrap();
        let n3 = TreeNode::new_list_node(vec![b'c']).unwrap();

        // Build a left-skewed tree: n3(root) -> left n2 -> left n1
        let root = n3.attach(true, Some(n2.attach(true, Some(n1))));

        // Collect nodes by key for fetch closure (parent traversal)
        let mut map: HashMap<Vec<u8>, TreeNode> = HashMap::new();
        // Walk to insert nodes (simple DFS) since we consumed nodes building root
        fn insert_nodes(node: &TreeNode, map: &mut HashMap<Vec<u8>, TreeNode>) {
            map.insert(node.key().to_vec(), node.clone());
            if let Some(c) = node.child(true) { insert_nodes(c, map); }
            if let Some(c) = node.child(false) { insert_nodes(c, map); }
        }
        insert_nodes(&root, &mut map);

        // Closure to fetch parent by key
        let mut fetch = |k: &[u8]| map.get(k).cloned();

        // Access nodes again
        let root_ref = map.get(root.key()).unwrap();
        let mid_ref = root_ref.child(true).unwrap();
        let left_ref = mid_ref.child(true).unwrap();

        // In-order traversal positions should be: left_ref=0, mid_ref=1, root_ref=2
        assert_eq!(left_ref.compute_position_with_parent_fetch(&mut fetch), Some(0));
        assert_eq!(mid_ref.compute_position_with_parent_fetch(&mut fetch), Some(1));
        assert_eq!(root_ref.compute_position_with_parent_fetch(&mut fetch), Some(2));

        // subtree sizes
        assert_eq!(root_ref.subtree_size(), 3);
        assert_eq!(mid_ref.subtree_size(), 2);
        assert_eq!(left_ref.subtree_size(), 1);
    }

    #[test]
    #[cfg(feature = "list_mode")]
    fn test_insert_at_position() {
        // Test inserting nodes at specific positions in a list_mode tree
        let mut tree = TreeNode::new_list_node(vec![1]).unwrap();

        // Insert at position 0 (before the first node)
        let (new_tree, key_x) = tree.insert_at_position(0, vec![2]).unwrap().unwrap();
        tree = new_tree;
        
        // Insert at position 2 (at the end)
        let (new_tree, key_z) = tree.insert_at_position(2, vec![3]).unwrap().unwrap();
        tree = new_tree;
        
        // Insert at position 2 (between position 1 and 2)
        let (new_tree, key_y) = tree.insert_at_position(2, vec![4]).unwrap().unwrap();
        tree = new_tree;

        // Verify the in-order traversal gives us: value 2, value 1, value 4, value 3
        let mut values = Vec::new();
        fn collect_values(node: &TreeNode, values: &mut Vec<Vec<u8>>) {
            if let Some(left) = node.child(true) {
                collect_values(left, values);
            }
            values.push(node.inner.kv.value_as_slice().to_vec());
            if let Some(right) = node.child(false) {
                collect_values(right, values);
            }
        }
        collect_values(&tree, &mut values);
        
        assert_eq!(values, vec![vec![2], vec![1], vec![4], vec![3]]);
        assert_eq!(tree.subtree_size(), 4);
    }

    #[test]
    #[cfg(feature = "list_mode")]
    fn test_delete_at_position() {
        // Test deleting nodes at specific positions in a list_mode tree
        // Build tree with values: [2, 1, 4, 3] at positions 0, 1, 2, 3
        let mut tree = TreeNode::new_list_node(vec![1]).unwrap();
        let (tree, _) = tree.insert_at_position(0, vec![2]).unwrap().unwrap();
        let (tree, _) = tree.insert_at_position(2, vec![3]).unwrap().unwrap();
        let (mut tree, _) = tree.insert_at_position(2, vec![4]).unwrap().unwrap();

        // Initial state: [2, 1, 4, 3]
        assert_eq!(tree.subtree_size(), 4);

        // Delete at position 2 (value 4)
        let (new_tree, key, value) = tree.delete_at_position(2).unwrap().unwrap();
        tree = new_tree;
        assert_eq!(value, vec![4]);
        assert_eq!(tree.subtree_size(), 3);

        // Verify remaining: [2, 1, 3]
        let mut values = Vec::new();
        fn collect_values(node: &TreeNode, values: &mut Vec<Vec<u8>>) {
            if let Some(left) = node.child(true) {
                collect_values(left, values);
            }
            values.push(node.inner.kv.value_as_slice().to_vec());
            if let Some(right) = node.child(false) {
                collect_values(right, values);
            }
        }
        collect_values(&tree, &mut values);
        assert_eq!(values, vec![vec![2], vec![1], vec![3]]);

        // Delete at position 0 (value 2)
        let (new_tree, _, value) = tree.delete_at_position(0).unwrap().unwrap();
        tree = new_tree;
        assert_eq!(value, vec![2]);
        assert_eq!(tree.subtree_size(), 2);

        // Verify remaining: [1, 3]
        values.clear();
        collect_values(&tree, &mut values);
        assert_eq!(values, vec![vec![1], vec![3]]);

        // Delete at position 1 (value 3)
        let (new_tree, _, value) = tree.delete_at_position(1).unwrap().unwrap();
        tree = new_tree;
        assert_eq!(value, vec![3]);
        assert_eq!(tree.subtree_size(), 1);

        // Verify remaining: [1]
        values.clear();
        collect_values(&tree, &mut values);
        assert_eq!(values, vec![vec![1]]);
    }

    #[test]
    #[cfg(feature = "list_mode")]
    fn test_insert_after_key() {
        use std::collections::HashMap;

        // Build a tree with 3 nodes: [a, b, c]
        let n1 = TreeNode::new_list_node(vec![b'a']).unwrap();
        let key_a = n1.key().to_vec();
        let n2 = TreeNode::new_list_node(vec![b'b']).unwrap();
        let key_b = n2.key().to_vec();
        let n3 = TreeNode::new_list_node(vec![b'c']).unwrap();

        // Build tree: n3(root) -> left n2 -> left n1
        let root = n3.attach(true, Some(n2.attach(true, Some(n1))));

        // Collect nodes in a map for fetch closure
        let mut map: HashMap<Vec<u8>, TreeNode> = HashMap::new();
        fn collect_nodes(node: &TreeNode, map: &mut HashMap<Vec<u8>, TreeNode>) {
            map.insert(node.key().to_vec(), node.clone());
            if let Some(left) = node.child(true) {
                collect_nodes(left, map);
            }
            if let Some(right) = node.child(false) {
                collect_nodes(right, map);
            }
        }
        collect_nodes(&root, &mut map);

        // Insert 'x' after node with key_b (which is at position 1)
        // Expected result: [a, b, x, c] at positions 0, 1, 2, 3
        let fetch = |k: &[u8]| map.get(k).cloned();
        let (tree, key_x) = root.insert_after_key(&key_b, vec![b'x'], fetch)
            .unwrap()
            .unwrap();

        // Verify in-order traversal: [a, b, x, c]
        let mut values = Vec::new();
        fn collect_values(node: &TreeNode, values: &mut Vec<Vec<u8>>) {
            if let Some(left) = node.child(true) {
                collect_values(left, values);
            }
            values.push(node.inner.kv.value_as_slice().to_vec());
            if let Some(right) = node.child(false) {
                collect_values(right, values);
            }
        }
        collect_values(&tree, &mut values);
        assert_eq!(values, vec![vec![b'a'], vec![b'b'], vec![b'x'], vec![b'c']]);
        assert_eq!(tree.subtree_size(), 4);

        // Now insert 'y' after 'a' (position 0)
        // Update map with new tree
        map.clear();
        collect_nodes(&tree, &mut map);
        let fetch2 = |k: &[u8]| map.get(k).cloned();
        let (tree, key_y) = tree.insert_after_key(&key_a, vec![b'y'], fetch2)
            .unwrap()
            .unwrap();

        // Verify in-order traversal: [a, y, b, x, c]
        values.clear();
        collect_values(&tree, &mut values);
        assert_eq!(values, vec![vec![b'a'], vec![b'y'], vec![b'b'], vec![b'x'], vec![b'c']]);
        assert_eq!(tree.subtree_size(), 5);
    }

    #[test]
    #[cfg(feature = "list_mode")]
    fn test_collaborative_document_editing_simulation() {
        use std::collections::HashMap;

        // Simulate a collaborative document editing scenario using the "Text Without CRDTs" approach
        // from https://mattweidner.com/2025/05/21/text-without-crdts.html
        //
        // Scenario: Two users editing a shared document
        // - Initial state: empty document
        // - User A types "Hello"
        // - User B concurrently types "World" at the end
        // - User A inserts space and "Beautiful" after "Hello"
        // - Final result should be: "Hello Beautiful World" (or similar valid interleaving)

        println!("\n=== Collaborative Document Editing Simulation ===");

        // Start with empty document (single node as placeholder, or we could start truly empty)
        let mut doc = TreeNode::new_list_node(vec![b'H']).unwrap();
        let key_h = doc.key().to_vec();
        println!("Initial: H");

        // User A: Insert 'e' after 'H'
        let (new_doc, key_e) = doc.insert_at_position(1, vec![b'e']).unwrap().unwrap();
        doc = new_doc;
        println!("After insert 'e' at position 1: He");

        // User A: Insert 'l' after 'e'
        let mut map: HashMap<Vec<u8>, TreeNode> = HashMap::new();
        fn update_map(tree: &TreeNode, map: &mut HashMap<Vec<u8>, TreeNode>) {
            map.clear();
            fn collect(node: &TreeNode, map: &mut HashMap<Vec<u8>, TreeNode>) {
                map.insert(node.key().to_vec(), node.clone());
                if let Some(left) = node.child(true) {
                    collect(left, map);
                }
                if let Some(right) = node.child(false) {
                    collect(right, map);
                }
            }
            collect(tree, map);
        }
        update_map(&doc, &mut map);
        let fetch = |k: &[u8]| map.get(k).cloned();
        let (new_doc, key_l1) = doc.insert_after_key(&key_e, vec![b'l'], fetch).unwrap().unwrap();
        doc = new_doc;
        println!("After insert 'l' after 'e': Hel");

        // User A: Insert another 'l' after first 'l'
        update_map(&doc, &mut map);
        let fetch = |k: &[u8]| map.get(k).cloned();
        let (new_doc, key_l2) = doc.insert_after_key(&key_l1, vec![b'l'], fetch).unwrap().unwrap();
        doc = new_doc;
        println!("After insert 'l' after 'l': Hell");

        // User A: Insert 'o' after second 'l'
        update_map(&doc, &mut map);
        let fetch = |k: &[u8]| map.get(k).cloned();
        let (new_doc, key_o1) = doc.insert_after_key(&key_l2, vec![b'o'], fetch).unwrap().unwrap();
        doc = new_doc;
        println!("After insert 'o' after 'l': Hello");

        // User B: Concurrently inserts ' ' (space) at the end
        let (new_doc, key_space) = doc.insert_at_position(5, vec![b' ']).unwrap().unwrap();
        doc = new_doc;
        println!("After insert ' ' at position 5: Hello ");

        // User B: Insert 'W'
        let (new_doc, key_w) = doc.insert_at_position(6, vec![b'W']).unwrap().unwrap();
        doc = new_doc;
        println!("After insert 'W' at position 6: Hello W");

        // User B: Continue typing "orld"
        update_map(&doc, &mut map);
        let fetch = |k: &[u8]| map.get(k).cloned();
        let (new_doc, key_o2) = doc.insert_after_key(&key_w, vec![b'o'], fetch).unwrap().unwrap();
        doc = new_doc;

        update_map(&doc, &mut map);
        let fetch = |k: &[u8]| map.get(k).cloned();
        let (new_doc, key_r) = doc.insert_after_key(&key_o2, vec![b'r'], fetch).unwrap().unwrap();
        doc = new_doc;

        update_map(&doc, &mut map);
        let fetch = |k: &[u8]| map.get(k).cloned();
        let (new_doc, key_l3) = doc.insert_after_key(&key_r, vec![b'l'], fetch).unwrap().unwrap();
        doc = new_doc;

        update_map(&doc, &mut map);
        let fetch = |k: &[u8]| map.get(k).cloned();
        let (new_doc, key_d) = doc.insert_after_key(&key_l3, vec![b'd'], fetch).unwrap().unwrap();
        doc = new_doc;
        println!("After User B types 'World': Hello World");

        // User A: Insert " Beautiful" between "Hello" and " World"
        // Insert space after 'o' in "Hello"
        update_map(&doc, &mut map);
        let fetch = |k: &[u8]| map.get(k).cloned();
        let (new_doc, key_space2) = doc.insert_after_key(&key_o1, vec![b' '], fetch).unwrap().unwrap();
        doc = new_doc;

        // Insert "Beautiful"
        update_map(&doc, &mut map);
        let fetch = |k: &[u8]| map.get(k).cloned();
        let (new_doc, key_b) = doc.insert_after_key(&key_space2, vec![b'B'], fetch).unwrap().unwrap();
        doc = new_doc;

        let chars = vec![b'e', b'a', b'u', b't', b'i', b'f', b'u', b'l'];
        let mut last_key = key_b;
        for ch in chars {
            update_map(&doc, &mut map);
            let fetch = |k: &[u8]| map.get(k).cloned();
            let (new_doc, new_key) = doc.insert_after_key(&last_key, vec![ch], fetch).unwrap().unwrap();
            doc = new_doc;
            last_key = new_key;
        }

        // Extract final document text
        let mut chars_vec = Vec::new();
        fn collect_chars(node: &TreeNode, chars: &mut Vec<u8>) {
            if let Some(left) = node.child(true) {
                collect_chars(left, chars);
            }
            chars.push(node.inner.kv.value_as_slice()[0]);
            if let Some(right) = node.child(false) {
                collect_chars(right, chars);
            }
        }
        collect_chars(&doc, &mut chars_vec);
        let final_text = String::from_utf8(chars_vec).unwrap();
        
        println!("Final document: {}", final_text);
        println!("Final tree size: {}", doc.subtree_size());

        // Verify the final document contains all characters in the right order
        assert_eq!(final_text, "Hello Beautiful World");
        assert_eq!(doc.subtree_size(), 21); // 21 characters total

        println!("\n=== Simulation Complete ===");
        println!("Successfully demonstrated:");
        println!("- Sequential character insertion using insert_after_key");
        println!("- UUID-based character identity (stable across edits)");
        println!("- Positional tree structure maintaining document order");
        println!("- Parent pointer traversal for position computation");
    }

    #[test]
    #[cfg(all(feature = "list_mode", feature = "uuid"))]
    fn test_new_list_node_with_key() {
        // Test creating a list node with a client-provided key
        use uuid::Uuid;
        
        let my_uuid = Uuid::new_v4();
        let my_key = my_uuid.as_bytes().to_vec();
        let node = TreeNode::new_list_node_with_key(my_key.clone(), vec![b'x']).unwrap();
        
        // Verify the key matches what we provided
        assert_eq!(node.key(), my_key.as_slice());
        assert_eq!(node.inner.kv.value_as_slice(), &[b'x']);
        assert_eq!(node.subtree_size(), 1);
        assert!(node.is_list_mode());
    }

    #[test]
    #[cfg(all(feature = "list_mode", feature = "uuid"))]
    fn test_insert_at_position_with_key() {
        use uuid::Uuid;
        
        // Build initial tree with server-generated keys: [a, b, c]
        let mut tree = TreeNode::new_list_node(vec![b'a']).unwrap();
        let (tree, _) = tree.insert_at_position(1, vec![b'b']).unwrap().unwrap();
        let (tree, _) = tree.insert_at_position(2, vec![b'c']).unwrap().unwrap();
        
        // Client picks UUID for 'x' and inserts at position 1
        let client_uuid_x = Uuid::new_v4();
        let key_x = client_uuid_x.as_bytes().to_vec();
        let (tree, returned_key) = tree
            .insert_at_position_with_key(1, key_x.clone(), vec![b'x'])
            .unwrap()
            .unwrap();
        
        // Verify returned key matches what client provided
        assert_eq!(returned_key, key_x);
        
        // Verify in-order traversal: [a, x, b, c]
        let mut values = Vec::new();
        let mut keys = Vec::new();
        fn collect(node: &TreeNode, values: &mut Vec<Vec<u8>>, keys: &mut Vec<Vec<u8>>) {
            if let Some(left) = node.child(true) {
                collect(left, values, keys);
            }
            values.push(node.inner.kv.value_as_slice().to_vec());
            keys.push(node.key().to_vec());
            if let Some(right) = node.child(false) {
                collect(right, values, keys);
            }
        }
        collect(&tree, &mut values, &mut keys);
        
        assert_eq!(values, vec![vec![b'a'], vec![b'x'], vec![b'b'], vec![b'c']]);
        assert_eq!(keys[1], key_x); // 'x' is at position 1
        assert_eq!(tree.subtree_size(), 4);
        
        // Client picks another UUID for 'y' and inserts at position 0
        let client_uuid_y = Uuid::new_v4();
        let key_y = client_uuid_y.as_bytes().to_vec();
        let (tree, returned_key) = tree
            .insert_at_position_with_key(0, key_y.clone(), vec![b'y'])
            .unwrap()
            .unwrap();
        
        assert_eq!(returned_key, key_y);
        
        // Verify in-order traversal: [y, a, x, b, c]
        values.clear();
        keys.clear();
        collect(&tree, &mut values, &mut keys);
        
        assert_eq!(values, vec![vec![b'y'], vec![b'a'], vec![b'x'], vec![b'b'], vec![b'c']]);
        assert_eq!(keys[0], key_y); // 'y' is at position 0
        assert_eq!(tree.subtree_size(), 5);
    }

    #[test]
    #[cfg(all(feature = "list_mode", feature = "uuid"))]
    fn test_client_controlled_collaborative_editing() {
        use uuid::Uuid;
        
        // Simulate scenario where client picks UUIDs locally for optimistic updates
        println!("\n=== Client-Controlled Collaborative Editing ===");
        
        // Client A: Initialize document with 'H' (client picks UUID)
        let uuid_h = Uuid::new_v4();
        let key_h = uuid_h.as_bytes().to_vec();
        let mut doc = TreeNode::new_list_node_with_key(key_h.clone(), vec![b'H']).unwrap();
        println!("Client A creates 'H' with UUID: {}", uuid_h);
        
        // Client A: Add 'i' after 'H' (client picks UUID)
        let uuid_i = Uuid::new_v4();
        let key_i = uuid_i.as_bytes().to_vec();
        let (doc, _) = doc
            .insert_at_position_with_key(1, key_i.clone(), vec![b'i'])
            .unwrap()
            .unwrap();
        println!("Client A inserts 'i' with UUID: {}", uuid_i);
        
        // Client B: Concurrently inserts '!' at end (client picks UUID)
        let uuid_bang = Uuid::new_v4();
        let key_bang = uuid_bang.as_bytes().to_vec();
        let (doc, _) = doc
            .insert_at_position_with_key(2, key_bang.clone(), vec![b'!'])
            .unwrap()
            .unwrap();
        println!("Client B inserts '!' with UUID: {}", uuid_bang);
        
        // Extract document text
        let mut chars = Vec::new();
        fn collect_chars(node: &TreeNode, chars: &mut Vec<u8>) {
            if let Some(left) = node.child(true) {
                collect_chars(left, chars);
            }
            chars.push(node.inner.kv.value_as_slice()[0]);
            if let Some(right) = node.child(false) {
                collect_chars(right, chars);
            }
        }
        collect_chars(&doc, &mut chars);
        let text = String::from_utf8(chars).unwrap();
        
        println!("Final document: {}", text);
        assert_eq!(text, "Hi!");
        assert_eq!(doc.subtree_size(), 3);
        
        // Verify all client-provided keys are present
        let mut found_keys = Vec::new();
        fn collect_keys(node: &TreeNode, keys: &mut Vec<Vec<u8>>) {
            if let Some(left) = node.child(true) {
                collect_keys(left, keys);
            }
            keys.push(node.key().to_vec());
            if let Some(right) = node.child(false) {
                collect_keys(right, keys);
            }
        }
        collect_keys(&doc, &mut found_keys);
        
        assert!(found_keys.contains(&key_h));
        assert!(found_keys.contains(&key_i));
        assert!(found_keys.contains(&key_bang));
        
        println!("=== All client-provided UUIDs preserved ===");
    }

    #[test]
    #[cfg(feature = "list_mode")]
    fn test_rotation_right_single() {
        // Create a left-heavy tree that needs right rotation
        //       3
        //      /
        //     2
        //    /
        //   1
        // Should become:
        //     2
        //    / \
        //   1   3
        
        let mut tree = TreeNode::new_list_node(vec![3]).unwrap();
        let (new_tree, _) = tree.insert_at_position(0, vec![2]).unwrap().unwrap();
        tree = new_tree;
        let (new_tree, _) = tree.insert_at_position(0, vec![1]).unwrap().unwrap();
        tree = new_tree;
        
        // Check the tree is balanced (root should be 2)
        let root_value = tree.value_as_slice().to_vec();
        assert_eq!(root_value, vec![2]);
        
        // Check structure
        assert!(tree.child(true).is_some());
        assert!(tree.child(false).is_some());
        assert_eq!(tree.child(true).unwrap().value_as_slice(), &[1]);
        assert_eq!(tree.child(false).unwrap().value_as_slice(), &[3]);
        
        // Check balance factor is within [-1, 1]
        let bf = tree.balance_factor();
        assert!(bf >= -1 && bf <= 1, "Balance factor {} out of range", bf);
        
        println!("=== Right rotation test passed ===");
    }

    #[test]
    #[cfg(feature = "list_mode")]
    fn test_rotation_left_single() {
        // Create a right-heavy tree that needs left rotation
        //   1
        //    \
        //     2
        //      \
        //       3
        // Should become:
        //     2
        //    / \
        //   1   3
        
        let mut tree = TreeNode::new_list_node(vec![1]).unwrap();
        let (new_tree, _) = tree.insert_at_position(1, vec![2]).unwrap().unwrap();
        tree = new_tree;
        let (new_tree, _) = tree.insert_at_position(2, vec![3]).unwrap().unwrap();
        tree = new_tree;
        
        // Check the tree is balanced (root should be 2)
        let root_value = tree.value_as_slice().to_vec();
        assert_eq!(root_value, vec![2]);
        
        // Check structure
        assert!(tree.child(true).is_some());
        assert!(tree.child(false).is_some());
        assert_eq!(tree.child(true).unwrap().value_as_slice(), &[1]);
        assert_eq!(tree.child(false).unwrap().value_as_slice(), &[3]);
        
        // Check balance factor
        let bf = tree.balance_factor();
        assert!(bf >= -1 && bf <= 1, "Balance factor {} out of range", bf);
        
        println!("=== Left rotation test passed ===");
    }

    #[test]
    #[test]
    #[cfg(feature = "list_mode")]
    fn test_double_rotation_left_right() {
        // Create a tree requiring LR double rotation
        //     3
        //    /
        //   1
        //    \
        //     2
        // Should become:
        //     2
        //    / \
        //   1   3
        
        let mut tree = TreeNode::new_list_node(vec![3]).unwrap();
        let (new_tree, _) = tree.insert_at_position(0, vec![1]).unwrap().unwrap();
        tree = new_tree;
        let (new_tree, _) = tree.insert_at_position(1, vec![2]).unwrap().unwrap();
        tree = new_tree;
        
        // Check the tree is balanced (root should be 2)
        let root_value = tree.value_as_slice().to_vec();
        assert_eq!(root_value, vec![2]);
        
        // Check structure
        assert!(tree.child(true).is_some());
        assert!(tree.child(false).is_some());
        assert_eq!(tree.child(true).unwrap().value_as_slice(), &[1]);
        assert_eq!(tree.child(false).unwrap().value_as_slice(), &[3]);
        
        // Check balance factor
        let bf = tree.balance_factor();
        assert!(bf >= -1 && bf <= 1, "Balance factor {} out of range", bf);
        
        println!("=== LR double rotation test passed ===");
    }

    #[test]
    #[cfg(feature = "list_mode")]
    fn test_double_rotation_right_left() {
        // Create a tree requiring RL double rotation
        //   1
        //    \
        //     3
        //    /
        //   2
        // Should become:
        //     2
        //    / \
        //   1   3
        
        let mut tree = TreeNode::new_list_node(vec![1]).unwrap();
        let (new_tree, _) = tree.insert_at_position(1, vec![3]).unwrap().unwrap();
        tree = new_tree;
        let (new_tree, _) = tree.insert_at_position(1, vec![2]).unwrap().unwrap();
        tree = new_tree;
        
        // Check the tree is balanced (root should be 2)
        let root_value = tree.value_as_slice().to_vec();
        assert_eq!(root_value, vec![2]);
        
        // Check structure
        assert!(tree.child(true).is_some());
        assert!(tree.child(false).is_some());
        assert_eq!(tree.child(true).unwrap().value_as_slice(), &[1]);
        assert_eq!(tree.child(false).unwrap().value_as_slice(), &[3]);
        
        // Check balance factor
        let bf = tree.balance_factor();
        assert!(bf >= -1 && bf <= 1, "Balance factor {} out of range", bf);
        
        println!("=== RL double rotation test passed ===");
    }

    #[test]
    #[cfg(feature = "list_mode")]
    fn test_balanced_sequential_insertions() {
        // Insert elements sequentially at the end (worst case for non-AVL)
        // With AVL balancing, the tree height should stay O(log n)
        
        let mut tree = TreeNode::new_list_node(vec![0]).unwrap();
        
        // Insert 15 more elements sequentially
        for i in 1..16 {
            let (new_tree, _) = tree.insert_at_position(i, vec![i as u8]).unwrap().unwrap();
            tree = new_tree;
        }
        
        // Check tree has all elements
        assert_eq!(tree.subtree_size(), 16);
        
        // Check height is logarithmic (for 16 nodes, height should be at most 5)
        // Without balancing, sequential insertions would create height 16
        let height = tree.height();
        assert!(height <= 5, "Height {} too large for 16 nodes, AVL balancing not working", height);
        
        // Verify we can traverse all positions
        let mut values = Vec::new();
        fn collect_values(node: &TreeNode, values: &mut Vec<Vec<u8>>) {
            if let Some(left) = node.child(true) {
                collect_values(left, values);
            }
            values.push(node.value_as_slice().to_vec());
            if let Some(right) = node.child(false) {
                collect_values(right, values);
            }
        }
        collect_values(&tree, &mut values);
        
        // Should have all values in order
        assert_eq!(values.len(), 16);
        for (idx, val) in values.iter().enumerate() {
            assert_eq!(*val, vec![idx as u8]);
        }
        
        println!("=== Sequential insertions stayed balanced: height {} for 16 nodes ===", height);
    }

    #[test]
    #[cfg(feature = "list_mode")]
    fn test_parent_pointers_after_rotation() {
        // Verify parent pointers remain correct after rotations
        
        let mut tree = TreeNode::new_list_node(vec![1]).unwrap();
        tree.enable_parent_pointers_recursive(); // Enable parent pointers for this test
        
        let (new_tree, _) = tree.insert_at_position(1, vec![2]).unwrap().unwrap();
        let mut tree = new_tree;
        tree.enable_parent_pointers_recursive();
        
        let (new_tree, _) = tree.insert_at_position(2, vec![3]).unwrap().unwrap();
        let mut tree = new_tree;
        tree.enable_parent_pointers_recursive();
        
        // After balancing, tree structure is:
        //     2
        //    / \
        //   1   3
        
        // Root should have no parent
        assert!(tree.parent_key.is_none(), "Root should have no parent");
        
        // Left child should point to root
        let left = tree.child(true).unwrap();
        assert!(left.parent_key.is_some(), "Left child should have parent");
        assert_eq!(left.parent_key.as_ref().unwrap(), tree.key());
        assert_eq!(left.child_side, Some(true), "Left child should know it's on left");
        
        // Right child should point to root
        let right = tree.child(false).unwrap();
        assert!(right.parent_key.is_some(), "Right child should have parent");
        assert_eq!(right.parent_key.as_ref().unwrap(), tree.key());
        assert_eq!(right.child_side, Some(false), "Right child should know it's on right");
        
        println!("=== Parent pointers correct after rotation ===");
    }

    #[test]
    #[cfg(feature = "list_mode")]
    fn test_subtree_size_after_rotation() {
        // Verify subtree sizes remain correct after rotations
        
        let mut tree = TreeNode::new_list_node(vec![1]).unwrap();
        let (new_tree, _) = tree.insert_at_position(1, vec![2]).unwrap().unwrap();
        tree = new_tree;
        let (new_tree, _) = tree.insert_at_position(2, vec![3]).unwrap().unwrap();
        tree = new_tree;
        
        // Total size should be 3
        assert_eq!(tree.subtree_size(), 3);
        
        // Left subtree size should be 1
        let left = tree.child(true).unwrap();
        assert_eq!(left.subtree_size(), 1);
        
        // Right subtree size should be 1
        let right = tree.child(false).unwrap();
        assert_eq!(right.subtree_size(), 1);
        
        // Add more nodes and verify sizes
        let (new_tree, _) = tree.insert_at_position(0, vec![0]).unwrap().unwrap();
        tree = new_tree;
        let (new_tree, _) = tree.insert_at_position(4, vec![4]).unwrap().unwrap();
        tree = new_tree;
        
        assert_eq!(tree.subtree_size(), 5);
        
        println!("=== Subtree sizes correct after rotations ===");
    }

    #[test]
    #[cfg(feature = "list_mode")]
    fn test_deletion_maintains_balance() {
        // Build a balanced tree and delete elements, checking balance is maintained
        
        let mut tree = TreeNode::new_list_node(vec![0]).unwrap();
        
        // Insert 7 elements (will create a balanced tree)
        for i in 1..8 {
            let (new_tree, _) = tree.insert_at_position(i, vec![i as u8]).unwrap().unwrap();
            tree = new_tree;
        }
        
        let initial_height = tree.height();
        println!("Initial height with 8 nodes: {}", initial_height);
        
        // Delete some elements
        let (new_tree, _, _) = tree.delete_at_position(0).unwrap().unwrap();
        tree = new_tree;
        let (new_tree, _, _) = tree.delete_at_position(0).unwrap().unwrap();
        tree = new_tree;
        let (new_tree, _, _) = tree.delete_at_position(0).unwrap().unwrap();
        tree = new_tree;
        
        // 5 nodes remain
        assert_eq!(tree.subtree_size(), 5);
        
        // Height should still be balanced (at most 3 for 5 nodes)
        let final_height = tree.height();
        assert!(final_height <= 3, "Height {} too large for 5 nodes after deletions", final_height);
        
        // Check all balance factors are valid
        fn check_balance_factors(node: &TreeNode) {
            let bf = node.balance_factor();
            assert!(bf >= -1 && bf <= 1, "Balance factor {} out of range", bf);
            
            if let Some(left) = node.child(true) {
                check_balance_factors(left);
            }
            if let Some(right) = node.child(false) {
                check_balance_factors(right);
            }
        }
        check_balance_factors(&tree);
        
        println!("=== Deletions maintained balance: height {} for 5 nodes ===", final_height);
    }

    #[test]
    #[cfg(feature = "list_mode")]
    fn test_large_tree_stays_balanced() {
        // Insert 100 elements and verify height stays O(log n)
        
        let mut tree = TreeNode::new_list_node(vec![0]).unwrap();
        
        // Insert 99 more elements at various positions to stress-test rotations
        for i in 1..100 {
            // Insert at end sometimes, at beginning sometimes, in middle sometimes
            let pos = match i % 3 {
                0 => 0,                      // Insert at beginning
                1 => tree.subtree_size(),    // Insert at end
                _ => tree.subtree_size() / 2, // Insert in middle
            };
            let (new_tree, _) = tree.insert_at_position(pos, vec![i as u8]).unwrap().unwrap();
            tree = new_tree;
        }
        
        assert_eq!(tree.subtree_size(), 100);
        
        // Height for 100 nodes should be at most 8
        // Perfect binary tree: 2^7 = 128 nodes at height 7, so 100 nodes should be around 7-8
        // Without balancing, could be up to 100
        let height = tree.height();
        assert!(height <= 8, "Height {} too large for 100 nodes, AVL balancing insufficient", height);
        
        // Verify all balance factors are valid
        fn check_all_balance_factors(node: &TreeNode) {
            let bf = node.balance_factor();
            assert!(bf >= -1 && bf <= 1, "Balance factor {} out of range at node", bf);
            
            if let Some(left) = node.child(true) {
                check_all_balance_factors(left);
            }
            if let Some(right) = node.child(false) {
                check_all_balance_factors(right);
            }
        }
        check_all_balance_factors(&tree);
        
        println!("=== Large tree (100 nodes) stayed balanced: height {} ===", height);
    }
}








