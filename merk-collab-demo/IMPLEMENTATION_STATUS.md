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

## Next Steps

1. **Update merk-collab-demo to use `InsertAfterKeyWithKey`:**
   - Modify server to accept client-provided UUIDs
   - Update client to generate UUIDs locally
   - Remove position translation code

2. **Test end-to-end:**
   - Verify zero-latency typing works
   - Test concurrent edits from multiple clients
   - Verify auditor can validate proofs

3. **Performance testing:**
   - Benchmark UUID generation overhead
   - Compare latency vs position-based approach
   - Measure proof size differences

4. **Documentation:**
   - Update README with new operation usage
   - Add examples of zero-latency pattern
   - Document migration guide from position-based ops

## Related Files

- `merk/src/merk/list_ops.rs` - ListOp enum and batch operations
- `merk/src/tree/mod.rs` - Tree insertion methods
- `merk-collab-demo/REFERENCE_BASED_OPS.md` - Design documentation
- `merk-collab-demo/server/src/document.rs` - Server document logic
- `merk-collab-demo/client/src/document.ts` - Client document logic
- `merk-collab-demo/auditor/src/main.rs` - Proof verification

## Conclusion

The core functionality for Matt Weidner's "Text Without CRDTs" design is now implemented in the merk library. The remaining work is to update the merk-collab-demo to use these new operations and demonstrate the zero-latency typing experience.
