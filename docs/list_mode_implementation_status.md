# List Mode Implementation Status

## ✅ Completed (Model 2b - Non-BST with Persisted Parent Pointers)

### Core Data Structures

**Encoding Format:**
- Sentinel: `0xFF` marker for list_mode nodes
- Layout: `0xFF || subtree_size (u64 LE) || parent_flag (0x00/0x01) || optional_parent_uuid (16 bytes) || legacy_inner`
- Parent persistence: Parent UUID pointers are written to storage and included in node hash

**Hashing:**
- Function: `node_hash_list_mode(kv_hash, left_hash, right_hash, subtree_size, parent_key)`
- Domain separator: `0xA5`
- Hash layout: `0xA5 || kv_hash || left_hash || right_hash || subtree_size_le || parent_bytes`
- Parent inclusion: `0x00` (no parent) or `0x01 || parent_key` (16 bytes)

**Tree Operations:**
- `attach(left/right, child)`: Updates `parent_key` and `child_side` metadata
- `detach(left/right)`: Clears parent metadata on detached subtree
- `recompute_subtree_sizes_recursive()`: Recursively updates `subtree_size` from leaves up

### Positional Operations (merk/src/tree/mod.rs)

**Core Functions:**
1. **`insert_at_position(position: u64, value: Vec<u8>)`** (lines ~358-410)
   - Inserts new node at 0-based position
   - Generates random UUID key via `new_list_node()`
   - Uses recursive descent based on `subtree_size` (not key comparisons)
   - Updates all ancestor `subtree_size` values
   - Returns: `CostContext<Result<(TreeNode, Vec<u8>), Error>>`
   - Algorithm: O(log n) with balanced tree, O(n) worst case unbalanced

2. **`delete_at_position(position: u64)`** (lines ~425-508)
   - Deletes node at 0-based position
   - Merges children when deleting internal nodes
   - Updates all ancestor `subtree_size` values
   - Returns: `CostContext<Result<(TreeNode, Vec<u8>, Vec<u8>), Error>>`
   - Algorithm: O(log n) with balanced tree

3. **`insert_after_key(target_key: &[u8], value: Vec<u8>, fetch: F)`** (lines ~524-571)
   - High-level wrapper for collaborative editing
   - Fetches node by UUID using closure: `F: FnMut(&[u8]) -> Option<TreeNode>`
   - Computes position via `compute_position_with_parent_fetch()`
   - Calls `insert_at_position(position + 1, value)`
   - Returns: `CostContext<Result<(TreeNode, Vec<u8>), Error>>`
   - Use case: "Insert character X after UUID Y" in collaborative editor

4. **`compute_position_with_parent_fetch(fetch: F)`** (lines ~276-298)
   - Computes 0-based in-order position by climbing parent chain
   - Accumulates: left subtree sizes + parent contributions
   - Requires storage fetch closure to load parent nodes
   - Returns: `Option<u64>`
   - Algorithm: O(h) where h = tree height ≈ log n

**Client-Controlled Key API (Phase 4 - ✅ COMPLETED):**

5. **`new_list_node_with_key(key: Vec<u8>, value: Vec<u8>)`** (lines ~260-290)
   - Creates list-mode node with client-provided key
   - Enables optimistic local updates without waiting for server
   - Key should be unique identifier (typically 16-byte UUID)
   - Returns: `CostContext<Self>`
   - Use case: Client picks UUID locally before sending to server

6. **`insert_at_position_with_key(position, key, value)`** (lines ~445-525)
   - Inserts at position with client-specified UUID key
   - Same algorithm as `insert_at_position` but preserves client key
   - Returns: `CostContext<Result<(TreeNode, Vec<u8>), Error>>`
   - Returned key matches the provided key (validation)
   - Use case: Optimistic client-side insertion with pre-generated UUID

### Test Coverage

**Unit Tests (merk/src/tree/mod.rs):**
- `test_insert_at_position` (lines ~1846-1863): 
  - Sequential insertions at positions 0, 2, 2
  - Validates in-order traversal: [2, 1, 4, 3]
  - Verifies `subtree_size` updates
  
- `test_delete_at_position` (lines ~1865-1912):
  - Builds tree [2, 1, 4, 3]
  - Deletes from positions 2, 0, 1 in sequence
  - Validates remaining elements and `subtree_size` after each deletion
  
- `test_insert_after_key` (lines ~1914-1974):
  - Creates tree [a, b, c] with parent pointers
  - Uses HashMap-based fetch closure to simulate storage
  - Inserts 'x' after 'b' → [a, b, x, c]
  - Inserts 'y' after 'a' → [a, y, b, x, c]
  - Validates position computation via parent climb

**Integration Test:**
- `test_collaborative_document_editing_simulation` (lines ~1976-2095):
  - Simulates "Text Without CRDTs" collaborative editing approach
  - Two virtual users typing into shared document
  - User A types "Hello", User B types "World", User A inserts "Beautiful"
  - Final result: "Hello Beautiful World" (21 characters)
  - Demonstrates:
    - Sequential character insertion using `insert_after_key`
    - UUID-based character identity (stable across edits)
    - Positional tree structure maintaining document order
    - Parent pointer traversal for position computation
  - Output with `--nocapture` shows step-by-step document evolution

**Client-Controlled Keys (Phase 4 - ✅ COMPLETED):**
- `test_new_list_node_with_key`: Validates creating nodes with client-provided UUIDs
- `test_insert_at_position_with_key`: Tests insertion with specific keys, verifies key preservation
- `test_client_controlled_collaborative_editing`: Simulates optimistic client-side editing with "Hi!" document

**Test Results:**
- **Total: 636 tests passing** (233 in merk, up from 227 baseline)
- **0 failures**
- All existing tests remain green (no regressions)

### Documentation

**Updated Files:**
- `docs/list_mode.md`: Full Model 2b specification with:
  - Encoding format with sentinel and parent pointers
  - Hash structure with domain separation
  - Operations overview
  - Complexity analysis
  - RocksDB performance characteristics (O(1) cached, O(log n) LSM worst case)
  - Trade-off comparison: Model 2a (BST) vs Model 2b (non-BST)

**Code Comments:**
- TODO markers for client-provided UUID keys
- Algorithm descriptions in function docstrings
- Usage examples in test code

---

## 🚧 TODO (Future Work)

### 1. API Enhancements

**Client-Controlled Keys (✅ COMPLETED - Phase 4):**
- ✅ `new_list_node_with_key(key: Vec<u8>, value: Vec<u8>)`: Create node with specific UUID
- ✅ `insert_at_position_with_key(position, key, value)`: Insert with client-provided UUID
- **Use Case**: Clients pick UUID locally, add char to local view, send insertion to server without waiting for response
- **Status**: Implemented and tested (lines ~260-290, ~445-525)
- **Tests**: `test_new_list_node_with_key`, `test_insert_at_position_with_key`, `test_client_controlled_collaborative_editing`

**Batch Operations (Future):**
- `batch_insert_at_positions(Vec<(u64, Vec<u8>)>)`: Atomic multi-insert
- `batch_delete_at_positions(Vec<u64>)`: Atomic multi-delete
- **Benefit**: Reduce redundant tree traversals and `subtree_size` recomputation

**Query Operations (Future):**
- `get_at_position(position) -> Option<(&[u8], &[u8])>`: Fetch (key, value) at position
- `find_key_position(key, fetch) -> Option<u64>`: Get position of existing key
- `slice_at_positions(start, end) -> Vec<(Vec<u8>, Vec<u8>)>`: Get range of elements

### 2. Persistence Integration

**Storage Backend:**
- Integrate with GroveDB batch write system for atomic commits
- Test with actual RocksDB backend (currently uses in-memory HashMap in tests)
- Ensure parent pointers survive serialization round-trip
- Implement storage-backed `fetch` closure for production use

**Transactions:**
- Multi-operation edits within single transaction
- Rollback support for failed operations
- Concurrency control for collaborative editing

### 3. Balancing Strategy

**Decision Required:**
- **Option A**: Implement AVL rotations with parent pointer updates
  - Guarantees O(log n) worst case for all operations
  - Complexity: Must update parent pointers during rotations
  - Must update `subtree_size` values during rotations
  - Maintains Merkle hash integrity through rotation
  
- **Option B**: Defer balancing, document limitations
  - Simpler implementation
  - Acceptable for append-mostly workloads
  - O(n) worst case for pathological insertion patterns
  - Could add periodic rebalancing instead of per-operation

**Recommendation**: Start with Option B, add balancing if benchmarks show degradation

### 4. Proof System

**Proof Generation:**
- `prove_position(position) -> PositionalProof`: Generate proof for element at index
- `prove_range(start, end) -> RangeProof`: Prove contiguous sequence
- `prove_membership_with_position(key) -> MembershipPositionProof`: Joint proof

**Proof Verification:**
- Verify `subtree_size` values along proof path
- Accumulate position during verification
- Validate root hash matches expected value
- Ensure no gaps or duplicates in range proofs

**Serialization:**
- Design compact wire format for proofs
- Include: path nodes, `subtree_size` values, sibling hashes, parent pointers
- Optimize for common cases (single element, small ranges)

### 5. Performance Optimization

**Benchmarking:**
- Large document tests (1000, 10000, 100000 characters)
- Insert throughput: beginning, middle, end positions
- Delete throughput: random positions
- Position lookup latency with varying tree heights
- Memory usage with parent pointers vs. without

**Comparison:**
- Model 2a (BST) vs Model 2b (non-BST) performance
- Parent pointer overhead vs. root descent savings
- Cache hit rates for parent chain traversals

**Optimizations:**
- Cache parent chain for repeated operations
- Batch `subtree_size` recomputation
- Lazy parent pointer updates
- Memory pool for TreeNode allocations

### 6. Collaborative Editing Features

**Conflict Resolution:**
- Timestamp-based ordering for concurrent insertions
- Causal consistency guarantees
- Vector clocks or Lamport timestamps
- Deterministic merge strategies

**Real-World Integration:**
- Server protocol design (WebSocket, gRPC)
- Client-side optimistic updates
- Server-side validation and persistence
- Example application (not just unit test simulation)

**Operational Transformation:**
- Transform operations based on concurrent edits
- Intention preservation
- Convergence guarantees

### 7. Additional Features

**Metadata:**
- Per-character formatting (bold, italic, color)
- Author attribution
- Timestamps
- Tombstones for deleted characters (soft delete)

**Undo/Redo:**
- Operation history tracking
- Reverse operations
- Snapshot management

**Search:**
- Full-text search within document
- Position-aware queries

---

## Timeline Estimate

| Phase | Description | Effort | Status |
|-------|-------------|--------|--------|
| ✅ Phase 1 | Core data structures + encoding + hashing | DONE | Complete |
| ✅ Phase 2 | Positional operations (insert/delete/insert_after) | DONE | Complete |
| ✅ Phase 3 | Unit tests + integration test | DONE | Complete |
| ✅ Phase 4 | Client-controlled keys API | 1-2 days | Complete |
| 🚧 Phase 5 | Persistence integration + storage tests | 2-3 days | Not started |
| 🚧 Phase 6 | Balancing decision + implementation (if needed) | 3-5 days | Not started |
| 🚧 Phase 7 | Proof system design + implementation | 1-2 weeks | Not started |
| 🚧 Phase 8 | Performance benchmarking + optimization | 1 week | Not started |
| 🚧 Phase 9 | Collaborative editing protocol + demo | 2-3 weeks | Not started |

---

## Usage Examples

### Quick Example (Inline)

```rust
use merk::tree::TreeNode;
use std::collections::HashMap;

// Create initial document: "Hi"
let mut doc = TreeNode::new_list_node(vec![b'H']).unwrap();
let key_h = doc.key().to_vec();
let (doc, key_i) = doc.insert_at_position(1, vec![b'i']).unwrap().unwrap();

// Build storage map for fetch closure
let mut storage: HashMap<Vec<u8>, TreeNode> = HashMap::new();
fn collect(node: &TreeNode, map: &mut HashMap<Vec<u8>, TreeNode>) {
    map.insert(node.key().to_vec(), node.clone());
    if let Some(left) = node.child(true) { collect(left, map); }
    if let Some(right) = node.child(false) { collect(right, map); }
}
collect(&doc, &mut storage);

// Insert '!' after 'i' using UUID-based insertion
let fetch = |k: &[u8]| storage.get(k).cloned();
let (doc, key_bang) = doc.insert_after_key(&key_i, vec![b'!'], fetch)
    .unwrap()
    .unwrap();

// Result: "Hi!"
```

### Full Working Example (Collaborative Editing Simulation)

For a complete working example demonstrating collaborative document editing with the "Text Without CRDTs" approach, see:

**Location:** `merk/src/tree/mod.rs` lines ~1976-2095

**Test name:** `test_collaborative_document_editing_simulation`

**Run it:**
```bash
cargo test --features list_mode test_collaborative_document_editing_simulation -- --nocapture
```

This test simulates two users collaboratively editing a document:
- User A types "Hello"
- User B concurrently types "World" 
- User A inserts " Beautiful" in the middle
- Final result: "Hello Beautiful World"

The test demonstrates:
- Sequential character insertion using `insert_after_key`
- UUID-based character identity (stable across edits)
- Positional tree structure maintaining document order
- Parent pointer traversal for position computation
- Storage simulation with HashMap-based fetch closure

---

*Last updated: October 24, 2025*
*Branch: merk-list-mode*
*Status: Experimental - Core operations complete, persistence/proofs pending*
