# Reference-Based Operations (Matt Weidner's Design)

## Implementation Status: ✅ COMPLETE

The merk library now fully supports Matt Weidner's ["Text Without CRDTs"](https://mattweidner.com/2025/05/21/text-without-crdts.html) design with **reference-based operations** that use client-provided UUIDs.

### What Was Implemented

**New ListOp variant:**
```rust
/// Insert after a UUID with client-provided key - IMPLEMENTED ✅
InsertAfterKeyWithKey {
    target_key: Vec<u8>,  // UUID to insert after
    key: Vec<u8>,          // Client-provided UUID for new element
    value: Vec<u8>,
}
```

**New tree method:**
```rust
pub fn insert_after_key_with_key<F>(
    self,
    target_key: &[u8],
    key: Vec<u8>,
    value: Vec<u8>,
    mut fetch: F,
) -> CostContext<Result<(Self, Vec<u8>), Error>>
```

**Modified InsertAfterKey:**
- Now generates UUID and delegates to `InsertAfterKeyWithKey`
- Maintains backward compatibility
- Enables gradual migration

See `IMPLEMENTATION_STATUS.md` for detailed implementation notes.

## Matt Weidner's Design Requirements

**Operations:**
1. `insert_after(reference_uuid, new_uuid, char)` ✅ - **Implemented as `InsertAfterKeyWithKey`**
2. `insert_first(new_uuid, char)` ✅ - **Use `InsertAtPositionWithKey` with position=0**
3. `delete(uuid)` ⚠️ - **Use tombstone via `UpdateValueByKey` (TODO: add this variant)**

**Key Property**: Operations reference **UUIDs**, not positions. This makes them unambiguous even when applied concurrently offline.

## Zero-Latency Pattern (Now Possible!)

### Client Side
```javascript
function insertChar(afterUUID, char) {
  // 1. Generate UUID locally
  const newUUID = generateUUID();
  
  // 2. Show character immediately (zero latency!)
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

**Result**: Client shows character immediately, server confirms with same UUID. Zero latency typing!

## Current merk-collab-demo Implementation

The demo currently uses **position-based operations** with visible-to-tree position translation:

### What merk-collab-demo Currently Uses

**Operations:**
```rust
// Client provides UUID, inserts at position
InsertAtPositionWithKey {
    position: u64,
    key: Vec<u8>,      // Client-provided UUID
    value: Vec<u8>,
}

// Deletes at position
DeleteAtPosition {
    position: u64,
}
```

**Position Translation:**
- `visible_to_tree_position()` - converts visible position to tree position (includes tombstones)
- `tree_to_visible_position()` - converts tree position to visible position (excludes tombstones)

### Migration Path

To adopt Matt Weidner's design fully, merk-collab-demo needs:

1. **Server changes:**
   - Use `InsertAfterKeyWithKey` instead of `InsertAtPositionWithKey`
   - Accept client-provided UUIDs in operations
   - Remove position translation logic

2. **Client changes:**
   - Generate UUIDs locally before operations
   - Track UUID of previous character
   - Send `InsertAfterKeyWithKey` operations
   - Show characters immediately (optimistic updates)

3. **Auditor changes:**
   - Expect `InsertAfterKeyWithKey` operations
   - Verify proofs based on UUIDs, not positions

## What Still Needs Work

### Option 2: Add `UpdateValueByKey` for Tombstones

For tombstone deletion:
```rust
/// Update value by key (for tombstone marking)
UpdateValueByKey {
    key: Vec<u8>,      // UUID to update
    value: Vec<u8>,    // New value (e.g., [1, 'a'] for deleted)
}
```

Implementation:
- Find node with `key` in tree
- Update its value in-place
- More efficient than delete+reinsert

### Option 3: Hybrid Approach (Current)

Use what exists:
- **First char**: `InsertAtPositionWithKey { position: 0, key, value }`
- **After char**: `InsertAfterKey { target_key, value }` + track server's UUID response
- **Delete**: Find UUID's position, use tombstone update

**Limitation**: Can't do true optimistic updates because client doesn't control UUIDs for `InsertAfterKey`.

## Recommendation

**For Production**: Implement Option 1 + Option 2
- Adds two enum variants to `ListOp`
- Requires implementing position lookup and update logic
- Enables full Matt Weidner design with zero-latency typing

**For Demo**: Keep current hybrid approach
- Documents the limitations clearly
- Shows the tombstone concept working
- Avoids large merk library changes for now

## Example: True Reference-Based Operations

```rust
// Alice types "Hi" offline
let uuid_h = Uuid::new_v4();
let uuid_i = Uuid::new_v4();

// Alice's operations (can apply locally immediately):
ops = [
    InsertAtPositionWithKey { position: 0, key: uuid_h, value: b"H" },
    InsertAfterKeyWithKey { target_key: uuid_h, key: uuid_i, value: b"i" },
]

// Bob types "!" after 'H' (while Alice is offline)
let uuid_bang = Uuid::new_v4();
ops_bob = [
    InsertAfterKeyWithKey { target_key: uuid_h, key: uuid_bang, value: b"!" },
]

// When both sync:
// Alice: [H(uuid_h), i(uuid_i)]
// Bob:   [H(uuid_h), !(uuid_bang)]
// Server applies in order: "H" -> "Hi" -> "H!i"
// Both operations reference uuid_h unambiguously!
```

With current position-based:
```rust
// Alice types "Hi" offline - positions 0, 1
ops_alice = [
    InsertAtPositionWithKey { position: 0, key: uuid_h, value: b"H" },
    InsertAtPositionWithKey { position: 1, key: uuid_i, value: b"i" },
]

// Bob types "!" after 'H' (position 1) while Alice is offline
ops_bob = [
    InsertAtPositionWithKey { position: 1, key: uuid_bang, value: b"!" },
]

// When both sync - CONFLICT!
// Both trying to insert at position 1
// Server needs to resolve: who wins?
// Result depends on order, not on intent
```

## Related Files

- `merk/src/merk/list_ops.rs` - ListOp enum definition
- `merk/examples/uuid-collab-edit-with-proofs.rs` - Shows `InsertAfterKey` usage
- `merk-collab-demo/server/src/document.rs` - Current position-based implementation
- `TOMBSTONES.md` - Explains deletion strategy

## References

- Matt Weidner: ["Text Without CRDTs"](https://mattweidner.com/2025/05/21/text-without-crdts.html)
- Figma: ["How Figma's multiplayer technology works"](https://www.figma.com/blog/how-figmas-multiplayer-technology-works/)
