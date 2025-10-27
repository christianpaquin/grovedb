// MIT LICENSE
//
// Copyright (c) 2021 Dash Core Group
//
// Permission is hereby granted, free of charge, to any
// person obtaining a copy of this software and associated
// documentation files (the "Software"), to deal in the
// Software without restriction, including without
// limitation the rights to use, copy, modify, merge,
// publish, distribute, sublicense, and/or sell copies of
// the Software, and to permit persons to whom the Software
// is furnished to do so, subject to the following
// conditions:
//
// The above copyright notice and this permission notice
// shall be included in all copies or substantial portions
// of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
// ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
// TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
// PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
// SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
// CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
// OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
// IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
// DEALINGS IN THE SOFTWARE.

//! List-mode operations for Merk
//!
//! This module implements Option A (Direct Methods) for list-mode persistence.
//! These methods provide a simple API for positional operations on list trees.
//!
//! # Design Decision
//!
//! We chose to implement direct methods first (Option A) for the following reasons:
//! 1. Faster validation of persistence layer
//! 2. Simpler implementation and debugging
//! 3. Easy to use for single operations
//! 4. Clear upgrade path to batch system (Option B)
//!
//! # Future Migration to Batch System (Option B)
//!
//! When performance profiling shows that commit overhead is significant, or when
//! bulk operations become common, these methods can be refactored to internally
//! use a batch system (Option B). The public API can remain unchanged by having
//! these methods delegate to batch operations:
//!
//! ```rust,ignore
//! pub fn insert_at_position(...) -> CostResult<Vec<u8>, Error> {
//!     self.apply_list_batch(&[ListOp::InsertAtPosition { ... }], ...)
//!         .map(|keys| keys[0].clone())
//! }
//! ```
//!
//! See docs/list_mode_persistence_decision.md for full rationale.

#[cfg(feature = "full")]
use grovedb_costs::{CostResult, CostsExt};
#[cfg(feature = "full")]
use grovedb_storage::StorageContext;
#[cfg(feature = "full")]
use grovedb_version::version::GroveVersion;

#[cfg(feature = "full")]
use std::collections::{BTreeSet, LinkedList};

#[cfg(feature = "full")]
use grovedb_costs::{cost_return_on_error, OperationCost};

#[cfg(feature = "full")]
use crate::{
    merk::{defaults::ROOT_KEY_KEY, KeyUpdates},
    tree::{kv::ValueDefinedCostType, TreeNode},
    Error, Merk, TreeType,
};

#[cfg(feature = "full")]
impl<'db, S> Merk<S>
where
    S: StorageContext<'db>,
{
    /// Insert a value at a specific position in a list-mode tree.
    ///
    /// This operation generates a random UUID key for the new node and inserts
    /// it at the specified 0-based position. The tree must be a ListTree type.
    ///
    /// # Arguments
    ///
    /// * `position` - 0-based index where the value should be inserted
    /// * `value` - The value to insert
    /// * `grove_version` - Version information for compatibility
    ///
    /// # Returns
    ///
    /// The generated UUID key for the inserted node, or an error if:
    /// - The tree type is not ListTree
    /// - The position is out of bounds
    /// - Storage errors occur
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use grovedb_merk::{Merk, TreeType};
    /// use grovedb_version::version::GroveVersion;
    ///
    /// let grove_version = GroveVersion::latest();
    /// let mut merk = Merk::open_base(
    ///     storage_context,
    ///     TreeType::ListTree,
    ///     None,
    ///     &grove_version
    /// )?;
    ///
    /// // Insert 'H' at position 0
    /// let key1 = merk.insert_at_position(0, vec![b'H'], &grove_version)?;
    ///
    /// // Insert 'i' at position 1
    /// let key2 = merk.insert_at_position(1, vec![b'i'], &grove_version)?;
    /// ```
    ///
    /// # Performance
    ///
    /// - Time complexity: O(log n) for balanced trees
    /// - Each call commits to storage immediately
    /// - For bulk operations, consider migrating to batch API (Option B)
    ///
    /// # Upgrade Path
    ///
    /// This method is part of Option A (Direct Methods). When bulk operations
    /// become common, it can be refactored to use the batch system internally
    /// without changing the public API.
    pub fn insert_at_position(
        &mut self,
        position: u64,
        value: Vec<u8>,
        grove_version: &GroveVersion,
    ) -> CostResult<Vec<u8>, Error> {
        let mut cost = OperationCost::default();

        // Verify this is a list tree
        if self.tree_type != TreeType::ListTree {
            return Err(Error::InvalidInputError(
                "insert_at_position requires TreeType::ListTree",
            ))
            .wrap_with_cost(Default::default());
        }

        // Get current tree or create empty list tree
        let (old_root_key, tree) = match self.tree.take() {
            Some(tree) => {
                // Tree is loaded, but might be lazy-loaded (only root loaded, children are Link::Reference)
                // Phase 5C: Check if we need to recursively load Link::Reference children
                let needs_full_load = tree.link(true).map_or(false, |l| l.is_reference())
                    || tree.link(false).map_or(false, |l| l.is_reference());
                
                if needs_full_load {
                    // Recursively load the full tree from storage
                    let loaded_tree = cost_return_on_error!(
                        &mut cost,
                        super::load_tree_recursively(tree, &self.storage, grove_version)
                    );
                    let old_key = loaded_tree.key().to_vec();
                    (Some(old_key), loaded_tree)
                } else {
                    // Tree is fully loaded in memory
                    let old_key = tree.key().to_vec();
                    (Some(old_key), tree)
                }
            }
            None => {
                // Tree not loaded - check if root exists in storage
                let root_key_opt = cost_return_on_error!(
                    &mut cost,
                    self.storage.get_root(ROOT_KEY_KEY).map_err(Error::StorageError)
                );
                
                if let Some(root_key) = root_key_opt {
                    // Root exists - load it
                    let root_tree = cost_return_on_error!(
                        &mut cost,
                        TreeNode::get(
                            &self.storage,
                            root_key.clone(),
                            None::<fn(&[u8], &GroveVersion) -> Option<ValueDefinedCostType>>,
                            grove_version
                        )
                    );
                    
                    if let Some(root) = root_tree {
                        // Phase 5C: Recursively load the full tree from storage
                        // This converts all Link::Reference children to Link::Loaded
                        let loaded_tree = cost_return_on_error!(
                            &mut cost,
                            super::load_tree_recursively(root, &self.storage, grove_version)
                        );
                        let old_key = loaded_tree.key().to_vec();
                        (Some(old_key), loaded_tree)
                    } else {
                        return Err(Error::InternalError("Root key exists but root node not found"))
                            .wrap_with_cost(cost);
                    }
                } else {
                    // Empty tree case - position must be 0
                    if position != 0 {
                        return Err(Error::InternalError(
                            "cannot insert at position > 0 in empty tree",
                        ))
                        .wrap_with_cost(cost);
                    }
                    
                    // Create first node
                    let node = TreeNode::new_list_node(value).unwrap_add_cost(&mut cost);
                    let generated_key = node.key().to_vec();
                    self.tree.set(Some(node));

                    // Build key_updates for empty tree insert
                    let mut new_keys = BTreeSet::new();
                    new_keys.insert(generated_key.clone());
                    let key_updates = KeyUpdates::new(
                        new_keys,
                        BTreeSet::default(),
                        LinkedList::default(),
                        None,
                    );

                    // Commit to storage
                    let empty_aux: &[(Vec<u8>, crate::tree::Op, Option<grovedb_costs::storage_cost::key_value_cost::KeyValueStorageCost>)] = &[];
                    cost_return_on_error!(
                        &mut cost,
                        self.commit(
                            key_updates,
                            empty_aux,
                            None,
                            &|_, _| Ok(0) // No specialized costs for list mode
                        )
                    );

                    return Ok(generated_key).wrap_with_cost(cost);
                }
            }
        };

        // Perform positional insert on existing tree
        let insert_result = tree.insert_at_position(position, value).unwrap_add_cost(&mut cost);
        let (new_tree, generated_key) = match insert_result {
            Ok(result) => result,
            Err(e) => return Err(e).wrap_with_cost(cost),
        };
        let new_root_key = new_tree.key().to_vec();

        // Set new root
        self.tree.set(Some(new_tree));

        // Build key_updates
        let mut new_keys = BTreeSet::new();
        new_keys.insert(generated_key.clone());

        let updated_root_key_from = if let Some(old_key) = old_root_key {
            if old_key != new_root_key {
                Some(old_key)
            } else {
                None
            }
        } else {
            None
        };

        let key_updates = KeyUpdates::new(
            new_keys,
            BTreeSet::default(),
            LinkedList::default(),
            updated_root_key_from,
        );

        // Commit to storage
        let empty_aux: &[(Vec<u8>, crate::tree::Op, Option<grovedb_costs::storage_cost::key_value_cost::KeyValueStorageCost>)] = &[];
        cost_return_on_error!(
            &mut cost,
            self.commit(
                key_updates,
                empty_aux,
                None,
                &|_, _| Ok(0) // No specialized costs for list mode
            )
        );

        Ok(generated_key).wrap_with_cost(cost)
    }

    /// Delete a value at a specific position in a list-mode tree.
    ///
    /// This operation removes the node at the specified 0-based position and
    /// returns both its key and value. The tree must be a ListTree type.
    ///
    /// # Arguments
    ///
    /// * `position` - 0-based index of the node to delete
    /// * `grove_version` - Version information for compatibility
    ///
    /// # Returns
    ///
    /// A tuple of (deleted_key, deleted_value), or an error if:
    /// - The tree type is not ListTree
    /// - The position is out of bounds
    /// - Storage errors occur
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Delete the node at position 1
    /// let (key, value) = merk.delete_at_position(1, &grove_version)?;
    /// println!("Deleted key: {:?}, value: {:?}", key, value);
    /// ```
    ///
    /// # Performance
    ///
    /// - Time complexity: O(log n) for balanced trees
    /// - Each call commits to storage immediately
    /// - For bulk deletions, consider migrating to batch API (Option B)
    pub fn delete_at_position(
        &mut self,
        position: u64,
        grove_version: &GroveVersion,
    ) -> CostResult<(Vec<u8>, Vec<u8>), Error> {
        let mut cost = OperationCost::default();

        // Verify this is a list tree
        if self.tree_type != TreeType::ListTree {
            return Err(Error::InvalidInputError(
                "delete_at_position requires TreeType::ListTree",
            ))
            .wrap_with_cost(Default::default());
        }

        // Get current tree and track old root
        let (old_root_key, tree) = match self.tree.take() {
            Some(tree) => {
                // Tree is loaded, but might be lazy-loaded (only root loaded, children are Link::Reference)
                // Phase 5C: Check if we need to recursively load Link::Reference children
                let needs_full_load = tree.link(true).map_or(false, |l| l.is_reference())
                    || tree.link(false).map_or(false, |l| l.is_reference());
                
                if needs_full_load {
                    // Recursively load the full tree from storage
                    let loaded_tree = cost_return_on_error!(
                        &mut cost,
                        super::load_tree_recursively(tree, &self.storage, grove_version)
                    );
                    let old_key = loaded_tree.key().to_vec();
                    (old_key, loaded_tree)
                } else {
                    // Tree is fully loaded in memory
                    let old_key = tree.key().to_vec();
                    (old_key, tree)
                }
            }
            None => {
                // Tree not loaded - check if root exists in storage
                let root_key_opt = cost_return_on_error!(
                    &mut cost,
                    self.storage.get_root(ROOT_KEY_KEY).map_err(Error::StorageError)
                );
                
                if let Some(root_key) = root_key_opt {
                    // Root exists - load it
                    let root_tree = cost_return_on_error!(
                        &mut cost,
                        TreeNode::get(
                            &self.storage,
                            root_key.clone(),
                            None::<fn(&[u8], &GroveVersion) -> Option<ValueDefinedCostType>>,
                            grove_version
                        )
                    );
                    
                    if let Some(root) = root_tree {
                        // Phase 5C: Recursively load the full tree from storage
                        let loaded_tree = cost_return_on_error!(
                            &mut cost,
                            super::load_tree_recursively(root, &self.storage, grove_version)
                        );
                        let old_key = loaded_tree.key().to_vec();
                        (old_key, loaded_tree)
                    } else {
                        return Err(Error::InternalError("Root key exists but root node not found"))
                            .wrap_with_cost(cost);
                    }
                } else {
                    return Err(Error::InternalError("cannot delete from empty tree"))
                        .wrap_with_cost(cost);
                }
            }
        };

        // Perform positional delete
        let delete_result = tree.delete_at_position(position).unwrap_add_cost(&mut cost);
        let (new_tree, deleted_key, deleted_value) = match delete_result {
            Ok(result) => result,
            Err(e) => return Err(e).wrap_with_cost(cost),
        };

        let new_root_key = new_tree.key().to_vec();

        // Set new root
        self.tree.set(Some(new_tree));

        // Build key_updates
        // For deletion, we track:
        // 1. deleted_keys: The key that was deleted
        // 2. updated_root_key_from: If root changed due to tree rebalancing
        let mut deleted_keys = LinkedList::new();
        
        // TODO: Calculate proper deletion costs for resource accounting
        // Currently using default (zeros) which means:
        //   - Deletion works correctly
        //   - But freed storage bytes aren't tracked for quotas/accounting
        // Should calculate removed_bytes based on:
        //   - Key cost: HASH_LENGTH + key_len + required_space(prefixed_key_len)
        //   - Value cost: actual encoded size including list_mode overhead
        // Note: Insert costs are now correctly calculated (including list_mode overhead)
        let deletion_cost = grovedb_costs::storage_cost::key_value_cost::KeyValueStorageCost::default();
        deleted_keys.push_back((deleted_key.clone(), deletion_cost));

        let updated_root_key_from = if old_root_key != new_root_key {
            Some(old_root_key)
        } else {
            None
        };

        let key_updates = KeyUpdates::new(
            BTreeSet::default(),
            BTreeSet::default(),
            deleted_keys,
            updated_root_key_from,
        );

        // Commit to storage
        let empty_aux: &[(Vec<u8>, crate::tree::Op, Option<grovedb_costs::storage_cost::key_value_cost::KeyValueStorageCost>)] = &[];
        cost_return_on_error!(
            &mut cost,
            self.commit(
                key_updates,
                empty_aux,
                None,
                &|_, _| Ok(0) // No specialized costs for list mode
            )
        );

        Ok((deleted_key, deleted_value)).wrap_with_cost(cost)
    }

    /// Insert a value after a specific key in a list-mode tree.
    ///
    /// This is the primary operation for collaborative editing scenarios where
    /// multiple clients need to insert characters relative to existing characters
    /// identified by their UUID keys.
    ///
    /// This method uses a storage-backed fetch closure to load nodes from the
    /// database, enabling efficient parent pointer traversal.
    ///
    /// # Arguments
    ///
    /// * `target_key` - The UUID key of the node after which to insert
    /// * `value` - The value to insert
    /// * `grove_version` - Version information for compatibility
    ///
    /// # Returns
    ///
    /// The generated UUID key for the inserted node, or an error if:
    /// - The tree type is not ListTree
    /// - The target_key is not found
    /// - Storage errors occur
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Insert 'H'
    /// let key_h = merk.insert_at_position(0, vec![b'H'], &grove_version)?;
    ///
    /// // Insert 'i' after 'H'
    /// let key_i = merk.insert_after_key(&key_h, vec![b'i'], &grove_version)?;
    ///
    /// // Insert '!' after 'i'
    /// let key_bang = merk.insert_after_key(&key_i, vec![b'!'], &grove_version)?;
    ///
    /// // Result: "Hi!"
    /// ```
    ///
    /// # Performance
    ///
    /// - Time complexity: O(log n) for balanced trees
    /// - Requires loading nodes from storage via parent chain traversal
    /// - Consider caching frequently accessed nodes
    ///
    /// # Implementation Note
    ///
    /// This method needs careful handling of the fetch closure due to Rust's
    /// borrow checker. The closure needs to access `self.storage` while we're
    /// also mutating `self.tree`. Current implementation is a stub that will
    /// be completed in the next iteration.
    pub fn insert_after_key(
        &mut self,
        target_key: &[u8],
        value: Vec<u8>,
        grove_version: &GroveVersion,
    ) -> CostResult<Vec<u8>, Error> {
        // Verify this is a list tree
        if self.tree_type != TreeType::ListTree {
            return Err(Error::InvalidInputError(
                "insert_after_key requires TreeType::ListTree",
            ))
            .wrap_with_cost(Default::default());
        }

        // TODO: Implement storage-backed fetch closure
        // Challenge: Need to access self.storage within closure while tree is taken
        // Solutions:
        // 1. Use RefCell/Cell for interior mutability
        // 2. Restructure to load nodes before taking tree
        // 3. Use unsafe with careful lifetimes

        // Stub implementation for now
        Err(Error::NotSupported("insert_after_key storage-backed fetch not yet implemented".to_string()))
            .wrap_with_cost(Default::default())
    }
}
