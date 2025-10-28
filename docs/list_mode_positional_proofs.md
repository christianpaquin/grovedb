# List Mode Positional Proofs Design

## Overview

This document describes the design for Merkle proof generation and verification for positional queries in list-mode trees. Unlike traditional Merk proofs which prove key membership in a BST, positional proofs must prove that a specific element exists at a given 0-based index position.

## Background

### Current Merk Proof System
- **Input**: Query by key(s) (e.g., "prove key 'foo' has value 'bar'")
- **Proof Path**: Navigate tree using BST key comparisons (left if key < node.key, right if key > node.key)
- **Verification**: Recompute hashes along path, verify root hash matches
- **Format**: Sequence of `Push(Node)` and `Parent/Child` operators

### List Mode Requirements
- **Input**: Query by position (e.g., "prove element at index 42")
- **Proof Path**: Navigate tree using `subtree_size` (go left if position < left_subtree_size)
- **Verification**: Must accumulate position while traversing proof, verify final position matches query
- **Additional Data**: Proofs must include `subtree_size` for each node along the path

## Design Challenges

### Challenge 1: Proof Format Extension
The current `Node` enum in `merk/src/proofs/mod.rs` does not include `subtree_size`:

```rust
pub enum Node {
    Hash(CryptoHash),
    KVHash(CryptoHash),
    KVDigest(Vec<u8>, CryptoHash),
    KV(Vec<u8>, Vec<u8>),
    KVValueHash(Vec<u8>, Vec<u8>, CryptoHash),
    KVValueHashFeatureType(Vec<u8>, Vec<u8>, CryptoHash, TreeFeatureType),
    KVRefValueHash(Vec<u8>, Vec<u8>, CryptoHash),
}
```

**Solution Options:**
1. **Add new variants** with subtree_size (e.g., `KVWithSize(Vec<u8>, Vec<u8>, u64)`)
2. **Separate metadata channel** - keep Node as-is, add parallel `subtree_size` array
3. **Hybrid approach** - extend proof format only for list-mode trees

### Challenge 2: Proof Generation
Current proof generation uses `RefWalker::create_proof()` which:
- Takes `query_items: &[QueryItem]` (key-based queries)
- Uses BST navigation (`query_item.within_range(node.key())`)
- Doesn't compute or track positions

**Requirements for Positional Proofs:**
- New method: `prove_position(position: u64) -> PositionalProof`
- Navigation logic: Use `subtree_size` instead of key comparisons
- Include sibling `subtree_size` values in proof for position computation

### Challenge 3: Proof Verification
Current verification in `merk/src/proofs/tree.rs`:
- Processes `Push` and `Parent/Child` operators
- Recomputes node hashes (using `node_hash()`)
- Verifies final root hash

**Requirements for Positional Verification:**
- Track accumulated position during traversal
- Verify `subtree_size` consistency (left + right + 1 = parent)
- Validate that final accumulated position matches queried position
- Use `node_hash_list_mode()` for list-mode nodes

## Proposed Solution: Phased Approach

### Step 1: Extend Proof Format

Add new Node variants for list-mode proofs:

```rust
pub enum Node {
    // ... existing variants ...
    
    /// List-mode node with subtree_size for positional proofs
    KVWithSubtreeSize(Vec<u8>, Vec<u8>, u64),
    
    /// List-mode hash node with subtree_size
    HashWithSubtreeSize(CryptoHash, u64),
    
    /// List-mode KV with value_hash and subtree_size
    KVValueHashWithSubtreeSize(Vec<u8>, Vec<u8>, CryptoHash, u64),
}
```

**Serialization Format:**
- Add type discriminator byte for new variants
- Encode subtree_size as varint (typically 1-2 bytes for reasonable document sizes)
- Maintain backward compatibility with existing proof format

### Step 2: Implement prove_position()

```rust
impl<'db, S> Merk<S>
where
    S: StorageContext<'db>,
{
    /// Generates a Merkle proof for the element at the given position.
    /// 
    /// # Arguments
    /// * `position` - 0-based index of element to prove
    /// * `grove_version` - Version for hash computation
    /// 
    /// # Returns
    /// Proof that can be verified to show:
    /// 1. An element exists at the given position
    /// 2. The element's key and value
    /// 3. The proof path subtree_size values are consistent
    /// 
    /// # Algorithm
    /// 1. Navigate tree using subtree_size (like insert_at_position)
    /// 2. For each node along path, record:
    ///    - Node key/value
    ///    - Subtree_size
    ///    - Sibling hash and subtree_size
    /// 3. Build proof as sequence of Push/Parent/Child ops with subtree_size
    /// 
    /// # Complexity
    /// O(log n) for balanced trees (same as key-based proofs)
    pub fn prove_position(
        &self,
        position: u64,
        grove_version: &GroveVersion,
    ) -> CostResult<PositionalProof, Error> {
        // Implementation would be similar to prove_unchecked but using
        // positional navigation instead of key-based QueryItems
        todo!("Implement positional proof generation")
    }
}
```

**Navigation Algorithm:**
```rust
fn navigate_to_position(tree: &TreeNode, target_position: u64) -> ProofPath {
    let mut current_position = 0u64;
    let mut path = vec![];
    let mut current = tree;
    
    loop {
        let left_size = current.child(true)
            .map(|c| c.subtree_size())
            .unwrap_or(0);
        
        path.push(ProofNode {
            key: current.key().to_vec(),
            value: current.value().to_vec(),
            subtree_size: current.subtree_size(),
            sibling_left: current.child(true).map(|c| (c.hash(), c.subtree_size())),
            sibling_right: current.child(false).map(|c| (c.hash(), c.subtree_size())),
        });
        
        if target_position == current_position + left_size {
            // Found the target at current node
            return path;
        } else if target_position < current_position + left_size {
            // Target is in left subtree
            current = current.child(true).expect("Left child must exist");
        } else {
            // Target is in right subtree
            current_position += left_size + 1;
            current = current.child(false).expect("Right child must exist");
        }
    }
}
```

### Step 3: Implement prove_range()

```rust
impl<'db, S> Merk<S>
where
    S: StorageContext<'db>,
{
    /// Generates a Merkle proof for a range of elements by position.
    /// 
    /// # Arguments
    /// * `start_position` - 0-based start index (inclusive)
    /// * `end_position` - 0-based end index (exclusive)
    /// * `grove_version` - Version for hash computation
    /// 
    /// # Returns
    /// Proof that can be verified to show:
    /// 1. All elements in [start_position, end_position) exist
    /// 2. The elements are contiguous (no gaps)
    /// 3. The elements are in order
    /// 
    /// # Algorithm
    /// 1. Find lowest common ancestor (LCA) of start and end positions
    /// 2. Build proof from root to LCA
    /// 3. Build proof from LCA to start_position
    /// 4. In-order traverse from start to end, including all nodes
    /// 5. Build proof from end_position back to LCA
    /// 
    /// # Complexity
    /// O(log n + k) where k is the number of elements in range
    pub fn prove_range(
        &self,
        start_position: u64,
        end_position: u64,
        grove_version: &GroveVersion,
    ) -> CostResult<RangeProof, Error> {
        todo!("Implement range proof generation")
    }
}
```

### Step 4: Implement Proof Verification

```rust
/// Verifies a positional proof and returns the proven element.
/// 
/// # Arguments
/// * `proof` - The encoded proof bytes
/// * `position` - Expected position to verify
/// * `root_hash` - Expected root hash
/// * `grove_version` - Version for hash computation
/// 
/// # Returns
/// Ok((key, value)) if proof is valid, Err otherwise
/// 
/// # Verification Algorithm
/// 1. Decode proof operators
/// 2. Process Push/Parent/Child operators to rebuild proof tree
/// 3. Track accumulated position during traversal:
///    - When going left: position stays same
///    - At current node: check if position matches (accumulated + left_size)
///    - When going right: add (left_size + 1) to accumulated position
/// 4. Verify subtree_size consistency at each node
/// 5. Recompute root hash using node_hash_list_mode()
/// 6. Verify root hash matches expected value
pub fn verify_positional_proof(
    proof: &[u8],
    position: u64,
    root_hash: &CryptoHash,
    grove_version: &GroveVersion,
) -> Result<(Vec<u8>, Vec<u8>), Error> {
    todo!("Implement positional proof verification")
}
```

## Implementation Priority

Given the complexity of implementing a full positional proof system, we recommend the following prioritization:

### High Priority (Required for Production)
1. **Design documentation** (this document) - Complete
2. **Proof format design** - Extend Node enum with subtree_size variants
3. **Basic prove_position()** - Single element positional proof

### Medium Priority (Important for Usability)
4. **Positional proof verification** - Verify single element proofs
5. **prove_range()** - Range proofs for contiguous elements
6. **Range proof verification** - Verify range proofs

### Low Priority (Nice to Have)
7. **Proof format optimization** - Compress subtree_size encoding
8. **Batch positional proofs** - Prove multiple non-contiguous positions efficiently
9. **Proof size benchmarking** - Compare positional vs key-based proof sizes

## Alternative Approaches

### Approach 1: Hybrid Proofs
Instead of new proof format, augment existing key-based proofs with position metadata:
- Generate normal key-based proof
- Add auxiliary position computation data
- Client verifies key proof normally, then computes position from subtree_size

**Pros:** No changes to core proof format
**Cons:** More complex verification logic, larger proofs

### Approach 2: Position Mapping
Store a separate position→key mapping in GroveDB:
- Key-based proof for element at key K
- Separate proof that position P maps to key K

**Pros:** Reuses existing proof system completely
**Cons:** Requires maintaining separate index, update overhead, two proofs needed

### Approach 3: Defer to Application Layer
Document proofs work on keys only, applications must:
- Track position→key mapping client-side
- Request key-based proofs
- Compute positions locally

**Pros:** No changes to Merk needed
**Cons:** Pushes complexity to applications, no verifiable positions

## Recommendation

Implement the design as documented above. This provides:
- Verifiable positional proofs for list-mode trees
- Consistent with Merk's cryptographic proof approach
- Foundation for future optimizations
- Clear migration path from current system

Full verification can be added based on production needs and user feedback.

## Related Documents
- [List Mode Implementation Status](list_mode_implementation_status.md)
- [Merk Proofs ADR](../adr/merk-proofs.md)
- [Query System](../adr/query-system.md)

## Status

**Current Status:** Design Complete
**Next Steps:** 
1. Implement proof format extension
2. Implement prove_position generation
3. Add verification logic based on use case requirements

```
