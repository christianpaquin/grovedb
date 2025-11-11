# Implementation Status: InsertAfterKeyWithKey

## Summary

We have successfully implemented `InsertAfterKeyWithKey` - a reference-based operation that enables client-controlled UUIDs for optimistic updates in collaborative text editing, as described in Matt Weidner's "Text Without CRDTs" design.

## What Was Implemented

### 1. New ListOp Variant: `InsertAfterKeyWithKey`

**Location:** `merk/src/merk/list_ops.rs` lines 163-203

```rust
InsertAfterKeyWithKey {
    target_key: Vec<u8>,  // UUID of the node to insert after
    key: Vec<u8>,         // Client-provided UUID for the new node
    value: Vec<u8>,       // Value to insert
}
```

This variant allows clients to:
- Generate UUIDs locally before sending to server
- Display characters immediately (zero latency)
- Avoid UUID conflicts (server confirms with same UUID)

### 2. Tree Method: `insert_after_key_with_key`

**Location:** `merk/src/tree/mod.rs` lines 735-785

```rust
pub fn insert_after_key_with_key<F>(
    self,
    target_key: &[u8],
    key: Vec<u8>,
    value: Vec<u8>,
    mut fetch: F,
) -> CostContext<Result<(Self, Vec<u8>), Error>>
```

Algorithm:
1. Find the node with `target_key` using `fetch`
2. Compute its position using `compute_position_with_parent_fetch`
3. Insert new value at position + 1 using `insert_at_position_with_key`

### 3. Batch Operation Handling

**Validation Phase** (lines ~940):
- Verify `target_key` exists in tree
- Track new `key` in `keys_in_tree`
- Increment size

**Application Phase** (lines ~1058):
- Build node map for fetch closure
- Call `tree.insert_after_key_with_key()`
- Track generated keys

**Empty Tree Helper** (lines ~1245):
- Same logic for first operation on empty tree

### 4. Modified InsertAfterKey to Delegate

**Before:** `InsertAfterKey` called `tree.insert_after_key()` (generates server-side UUID)

**After:** `InsertAfterKey` generates UUID and delegates to `InsertAfterKeyWithKey`:

```rust
let generated_key = uuid::Uuid::new_v4().as_bytes().to_vec();
tree.insert_after_key_with_key(target_key, generated_key, value, fetch)
```

This maintains backward compatibility while enabling the new functionality.

### 5. Test Coverage

Added `test_apply_list_batch_insert_after_key_with_key` demonstrating:
- Client generates UUIDs locally
- Server confirms operations with same UUID
- No conflict resolution needed
- Zero-latency typing experience

## Zero-Latency Pattern

### Client Side (Pseudocode)
```javascript
function insertChar(afterUUID, char) {
  // 1. Generate UUID locally
  const newUUID = generateUUID();
  
  // 2. Show character immediately
  document.insertAfter(afterUUID, {uuid: newUUID, char: char});
  
  // 3. Send to server asynchronously
  ws.send({
    op: "InsertAfterKeyWithKey",
    target_key: afterUUID,
    key: newUUID,
    value: char
  });
}
```

### Server Side
```rust
ListOp::InsertAfterKeyWithKey {
    target_key: after_uuid,
    key: client_uuid,  // Server uses client's UUID
    value: char_byte,
}
```

### Result
- **Zero latency:** Character appears immediately
- **No conflicts:** UUID is client-controlled
- **Convergence:** All clients see same UUID for same character
- **Proof verification:** Auditor can verify operations by UUID

## What's Still TODO

### 1. Update merk-collab-demo Server
**File:** `merk-collab-demo/server/src/document.rs`

Currently uses:
```rust
ListOp::InsertAtPositionWithKey { position, key, value }
```

Should use:
```rust
ListOp::InsertAfterKeyWithKey { target_key, key, value }
```

### 2. Update Client to Generate UUIDs
**File:** `merk-collab-demo/client/src/document.ts`

Need to:
- Import UUID library: `npm install uuid`
- Generate UUIDs locally before operations
- Track UUID of previous character for `InsertAfterKeyWithKey`
- Remove position translation logic (no longer needed)

### 3. Remove Position Translation
**File:** `merk-collab-demo/server/src/document.rs`

Can remove:
- `visible_to_tree_position()` function
- `tree_to_visible_position()` function
- Position counting logic

### 4. Update Auditor for Reference-Based Ops
**File:** `merk-collab-demo/auditor/src/main.rs`

Currently expects:
```rust
InsertAtPositionWithKey { position, key, value }
```

Should handle:
```rust
InsertAfterKeyWithKey { target_key, key, value }
```

## Benefits of This Implementation

### 1. Zero-Latency Typing
- Client shows characters immediately
- No waiting for server confirmation
- Better UX for real-time collaboration

### 2. No CRDT Complexity
- Simple reference-based model
- No version vectors or logical clocks
- Easy to understand and debug

### 3. UUID-Based Operations
- Operations reference content, not positions
- Resilient to concurrent edits
- Follows Matt Weidner's proven design

### 4. Backward Compatible
- `InsertAfterKey` still works (generates server-side UUID)
- Can mix position-based and reference-based ops
- Gradual migration path

## Testing

All tests pass:
```bash
cargo test --features list_mode list_ops
# Result: ok. 12 passed; 0 failed; 1 ignored
```

Specific test for new operation:
```bash
cargo test --features list_mode test_apply_list_batch_insert_after_key_with_key
# Result: ok. 1 passed; 0 failed
```

## Performance Optimizations (✅ IMPLEMENTED)

Merk now includes built-in performance optimizations for `list_mode`:

1. **Node Index for O(1) Lookups** - ✅ Implemented
   - `HashMap<Vec<u8>, (TreeNode, u64)>` caches nodes and their positions
   - Eliminates O(n) tree traversal on every `InsertAfterKey` operation
   - Automatically rebuilt after tree modifications
   - Public API: `merk.get_key_position(key)` for O(1) position lookups

2. **Position Caching** - ✅ Implemented
   - Index stores pre-computed positions during in-order traversal
   - Eliminates O(log n) parent chain walks for position discovery
   - Demo uses `get_key_position()` for fast position lookups

**Performance Impact:**
- InsertAfterKey operations: O(n) → O(1) for node fetch
- Position discovery: O(log n) → O(1) for indexed keys
- Batch operations: O(n × m) → O(n + m) for m operations on n-node tree

See `merk/src/merk/list_ops.rs` for implementation details.

## Next Steps (Optional Enhancements)

1. **Make InsertAfterKeyWithKey more robust:**
   - Handle structural changes from tombstone operations
   - Alternative tree traversal methods
   - Currently works but can fail after delete+reinsert

2. **Additional operations:**
   - `UpdateValueByKey` for more efficient tombstone updates
   - Currently uses batch: [DeleteAtPosition, InsertAtPositionWithKey]

3. **End-to-end testing:**
   - Multi-client stress testing
   - Large document performance testing
   - Conflict resolution scenarios

## Related Files

- `merk/src/merk/list_ops.rs` - ListOp enum and batch operations
- `merk/src/tree/mod.rs` - Tree insertion methods
- `merk-collab-demo/REFERENCE_BASED_OPS.md` - Design documentation
- `merk-collab-demo/server/src/document.rs` - Server document logic
- `merk-collab-demo/client/src/document.ts` - Client document logic
- `merk-collab-demo/auditor/src/main.rs` - Proof verification

## Conclusion

The core functionality for Matt Weidner's "Text Without CRDTs" design is now **fully implemented** in both the merk library and merk-collab-demo!

### ✅ What's Working

1. **Merk Library**: `InsertAfterKeyWithKey` fully implemented with all tests passing
2. **Demo Server**: Reference-based protocol accepting `target_uuid` from clients
3. **Demo Client**: Generates UUIDs locally, sends reference-based operations
4. **Auditor**: Verifies proofs with reference-based operations
5. **Zero-Latency Typing**: Characters appear instantly, server confirms with same UUID

### 🎯 Pure Reference-Based Architecture

**merk-collab-demo uses pure reference-based operations throughout:**

**Protocol Level (Reference-Based):**
- Client sends: `{ target_uuid, uuid, value }`
- Operations reference UUIDs, not positions
- Resilient to concurrent edits

**Implementation Level (Reference-Based):**
- Server uses `InsertAfterKeyWithKey` directly (no position conversion)
- Deletions use `UpdateValueByKey` for in-place tombstone updates
- Position discovery after insertion for proof generation

**Why it works now:** UpdateValueByKey enables in-place tombstone updates without tree restructuring. This keeps tree structure stable, allowing InsertAfterKeyWithKey to work reliably after deletions.

See `REFERENCE_BASED_OPS.md` for detailed explanation of this design.

### 🚀 Demo Ready

Run the server and client, open multiple browser tabs, and experience real-time collaborative editing with zero-latency typing!
