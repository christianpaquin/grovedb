# List Mode Persistence Implementation Decision

**Date:** October 27, 2025  
**Decision:** Implement Option A (Direct Methods) first, with clear upgrade path to Option B (Batch System)

---

## Current Implementation: Option A - Direct Methods

### Rationale

We've chosen to start with **Option A (Direct Methods)** for the following reasons:

1. **Faster time to working persistence** - Can validate serialization/deserialization within 2-3 weeks
2. **Lower implementation risk** - Simpler code path, easier debugging
3. **Validates core assumptions** - Tests that parent pointers and subtree_size persist correctly
4. **Minimal disruption** - Doesn't modify existing batch system
5. **Natural progression** - Can iterate on design before committing to batch architecture

### Implementation Plan

**Current Approach: Direct Methods**
- Add `TreeType::ListTree` enum variant - Complete
- Implement `Merk::insert_at_position()` - Complete
- Implement `Merk::delete_at_position()` - Complete
- Implement storage-backed `Merk::insert_after_key()` - Complete
- Write integration tests with RocksDB - Complete
- Validate parent pointer serialization - Complete

**Future Enhancement: Batch Operations**
- Design `ListOp` enum - Complete
- Implement `Merk::apply_list_batch()` - Complete
- Benchmark performance vs direct methods
- Keep direct methods as convenience wrappers (Hybrid approach)

---

## API Design

### Current: Direct Methods API

```rust
use grovedb_merk::{Merk, TreeType};
use grovedb_version::version::GroveVersion;

let grove_version = GroveVersion::latest();
let mut merk = Merk::open_base(
    storage_context,
    TreeType::ListTree,  // New tree type for list mode
    None,
    &grove_version
)?;

// Insert at position
let key1 = merk.insert_at_position(0, vec![b'H'], &grove_version)?;
let key2 = merk.insert_at_position(1, vec![b'i'], &grove_version)?;

// Insert after a specific key (collaborative editing)
let key3 = merk.insert_after_key(&key2, vec![b'!'], &grove_version)?;

// Delete at position
let (deleted_key, deleted_value) = merk.delete_at_position(1, &grove_version)?;
```

**Characteristics:**
- Each operation commits to storage immediately
- Simple, imperative API
- Perfect for interactive use and small operations
- Easy to understand and debug

---

## Future: Batch System API (Option B)

### When to Migrate

Migrate to Option B when:
1. Performance profiling shows commit overhead is significant
2. Bulk operations become common (>10 operations at once)
3. Need atomic multi-operation transactions
4. Want to mix list ops with key-based ops in one transaction

### Planned Batch API

```rust
use grovedb_merk::{Merk, ListOp};

// Define batch of operations
let batch = vec![
    ListOp::InsertAtPosition { position: 0, value: vec![b'H'] },
    ListOp::InsertAtPosition { position: 1, value: vec![b'e'] },
    ListOp::InsertAtPosition { position: 2, value: vec![b'l'] },
    ListOp::DeleteAtPosition { position: 0 },
];

// Apply all operations atomically in one commit
let generated_keys = merk.apply_list_batch(&batch, None, &grove_version)?;
```

**Characteristics:**
- Multiple operations in one commit
- Atomic transaction semantics
- More efficient for bulk operations
- Consistent with Merk's batch-first philosophy

---

## Upgrade Path: Hybrid API (Option C)

### Best of Both Worlds

Once we have both implementations, we can offer:

```rust
impl<'db, S> Merk<S> {
    // Convenience methods (wraps batch API internally)
    pub fn insert_at_position(...) -> CostResult<Vec<u8>, Error> {
        self.apply_list_batch(&[ListOp::InsertAtPosition { ... }], ...)
            .map(|keys| keys[0].clone())
    }
    
    // Batch methods for performance
    pub fn apply_list_batch(&mut self, batch: &[ListOp], ...) -> CostResult<Vec<Vec<u8>>, Error> {
        // Efficient batch implementation
    }
}
```

This gives users:
- **Simple API** for single operations (direct methods)
- **Efficient API** for bulk operations (batch methods)
- **Consistent implementation** (direct methods delegate to batch)

---

## Implementation Files

### Current Phase (Option A)

**Files to modify:**
1. `merk/src/tree_type.rs` - Add `TreeType::ListTree` variant
2. `merk/src/merk/list_ops.rs` - NEW: Direct method implementations
3. `merk/src/merk/mod.rs` - Export list operations
4. `merk/src/tree/list_mode_persistence_tests.rs` - Integration tests

### Future Phase (Option B)

**Files to create/modify:**
1. `merk/src/tree/ops.rs` - Add `ListOp` enum
2. `merk/src/merk/apply_list.rs` - NEW: Batch application logic
3. `merk/src/merk/mod.rs` - Export batch methods

---

## Technical Considerations

### Storage-Backed Fetch Closure

The `insert_after_key` method requires a fetch closure to load nodes by key. In Option A:

```rust
pub fn insert_after_key(
    &mut self,
    target_key: &[u8],
    value: Vec<u8>,
    grove_version: &GroveVersion,
) -> CostResult<Vec<u8>, Error> {
    // Create storage-backed fetch
    let fetch = |key: &[u8]| -> Option<TreeNode> {
        // Load from self.storage
        // Parse and return TreeNode
    };
    
    // Use TreeNode::insert_after_key with storage fetch
    // ...
}
```

**Challenge:** Borrow checker requires careful handling of `self` within closure.

**Solution:** Use cell-based interior mutability or restructure to avoid self-reference.

### Commit Strategy

**Option A (Current):**
- Each method calls `self.commit()` before returning
- Immediate persistence, simple to reason about
- Higher overhead for multiple operations

**Option B (Future):**
- Batch methods call `self.commit()` once for entire batch
- Deferred persistence, better performance
- More complex state management

---

## Testing Strategy

### Current Implementation Tests

1. **Basic persistence** - Complete
   - Insert at position → commit → reopen → verify

2. **Parent pointers survive roundtrip** - Complete
   - Create tree with parent pointers → persist → reload → verify pointers intact

3. **Subtree sizes persist** - Complete
   - Build tree → persist → reload → verify sizes correct

4. **AVL balance persists** - Complete
   - Perform rotations → persist → reload → verify still balanced

5. **Collaborative editing scenario** - Complete
   - insert_after_key → persist → reload → continue editing

### Batch Operations Tests

1. **Batch atomicity** - All ops succeed or all fail
2. **Performance benchmarks** - Compare batch vs direct methods
3. **Mixed operations** - List ops + key-based ops in one batch

---

## Performance Expectations

### Direct Methods

| Operation | Time | Commits |
|-----------|------|---------|
| Single insert | ~1ms | 1 |
| 100 inserts | ~100ms | 100 |
| 1000 inserts | ~1s | 1000 |

### Batch Operations

| Operation | Time | Commits |
|-----------|------|---------|
| Single insert | ~1ms | 1 |
| 100 inserts | ~15ms | 1 |
| 1000 inserts | ~120ms | 1 |

**Observed improvement:** 5-10x for bulk operations

---

## Decision Log

| Date | Decision | Rationale |
|------|----------|-----------|
| 2025-10-27 | Start with direct methods | Faster validation, lower risk |
| 2025-01 | Implement batch operations | Performance gains for bulk edits |
| TBD | Migrate to Option B | When bulk operations become common |
| TBD | Offer Hybrid (Option C) | Best user experience |

---

## References

- **Detailed Comparison:** `docs/list_mode_persistence_options.md`
- **Implementation Status:** `docs/list_mode_implementation_status.md`
- **Design Document:** Original "Text Without CRDTs" approach (https://mattweidner.com/2025/05/21/text-without-crdts.html)

---

*This document will be updated as implementation progresses and decisions are made.*

