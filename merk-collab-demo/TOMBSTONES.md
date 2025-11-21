# Tombstone Architecture

## Overview

This implementation follows Matt Weidner's ["Text Without CRDTs"](https://mattweidner.com/2025/05/21/text-without-crdts.html) design principle: **deleted items are not removed from the tree, but marked as deleted (tombstones)**.

## Why Tombstones?

Tombstones solve the fundamental problem of concurrent operations referencing deleted positions:

### Without Tombstones:
```
Initial state: [a, b, c, d]  (positions 0-3)

Client A (offline): Delete 'b' at position 1
  → Tree becomes: [a, c, d]  (positions 0-2)
  
Client B (offline): Insert 'x' after position 1 (after 'b')
  → References position 1, which no longer exists!
  
When both sync: ❌ Ambiguous - position 1 now refers to 'c', not 'b'
```

### With Tombstones:
```
Initial state: [a, b, c, d]  (positions 0-3)

Client A (offline): Delete 'b' at position 1
  → Tree becomes: [a, b̶, c, d]  (positions 0-3, b̶ is tombstone)
  
Client B (offline): Insert 'x' after position 1 (after 'b')
  → References position 1, which still exists as a tombstone!
  
When both sync: ✅ Unambiguous - position 1 still refers to tombstone 'b̶'
```

## Implementation

### Value Encoding

Each character in the tree is stored as a 2-byte value:
```
[deleted_flag, char_byte]
```

- **Byte 0 (deleted_flag)**: 
  - `0` = Active character (visible)
  - `1` = Tombstone (deleted, invisible)
- **Byte 1 (char_byte)**: The actual character value

### Insert Operation

```rust
// Client provides UUID and character
let op = ListOp::InsertAtPositionWithKey {
    position: position as u64,
    key: uuid,  // Client-generated UUID
    value: vec![0, 'a' as u8],  // [active=0, char='a']
};
```

**Result**: Character inserted with UUID, marked as active

### Delete Operation (Tombstone)

```rust
// Single-op in-place update
let op = ListOp::UpdateValueByKey {
  key: uuid.clone(),
  value: vec![1, 'a' as u8],  // [deleted=1, char='a']
};
```

**Result**: Character keeps the same UUID/position — only the value flips to “deleted”

### Display Logic

```rust
pub fn get_content(&self) -> Vec<(String, char)> {
    self.characters
        .iter()
        .filter(|c| !c.deleted)  // Filter out tombstones
        .map(|c| (uuid_to_string(&c.uuid), c.value))
        .collect()
}
```

**Result**: Clients only see active characters, tombstones are hidden

## Benefits

1. **Stable Positions**: Tombstones maintain tree positions, so concurrent operations work correctly
2. **UUID Persistence**: Deleted UUIDs remain in the tree, enabling late-arriving references
3. **Proof Generation**: Proofs work normally - tombstones are just nodes with different values
4. **Audit Trail**: Changelog shows deletions as value updates, not removals
5. **Conflict-Free**: Multiple clients can reference the same positions even after deletions

## Tree Structure Example

```
Insert 'a', 'b', 'c':
  Tree: [a:uuid1, b:uuid2, c:uuid3]
  Content: "abc"

Delete 'b':
  Tree: [a:uuid1, b̶:uuid2, c:uuid3]  (b̶ is tombstone)
  Content: "ac"  (tombstone filtered out)

Insert 'd' after position 1 (after tombstone):
  Tree: [a:uuid1, b̶:uuid2, d:uuid4, c:uuid3]
  Content: "adc"  (tombstone still filtered out)
```

## Memory Considerations

**Trade-off**: Tombstones consume memory/disk space since deleted items aren't removed.

**Mitigation strategies** (future work):
- Garbage collection: Remove tombstones older than N days
- Compaction: Periodically rebuild tree without tombstones (with migration)
- Archival: Move old tombstones to cold storage
- Client-side: Only sync recent tombstones, archive old ones

For this demo with in-memory storage, tombstones are acceptable.

## Testing

See `server/src/document.rs` for tests:
- `test_delete()` - Verifies tombstone creation and filtering
- `test_tombstones_keep_position()` - Verifies position stability

## Comparison to Traditional Delete

| Aspect | Traditional Delete | Tombstone Delete |
|--------|-------------------|------------------|
| Operation | Remove node from tree | Update node value |
| UUID | Lost forever | Persists in tree |
| Position | Shifts all subsequent items | Maintains positions |
| Concurrent Ops | Can become ambiguous | Always unambiguous |
| Memory | Reclaimed immediately | Grows with deletes |
| Proofs | N/A (node gone) | Normal proof generation |

## Future Enhancements

1. **Tombstone Metadata**: Track deletion timestamp, deleting user
2. **Resurrection**: Allow "undelete" by flipping flag back
3. **Compaction**: Periodic GC of old tombstones
4. **Analytics**: Track deletion patterns for user behavior
5. **Conflict Resolution**: Use tombstones to resolve concurrent insert/delete conflicts

## References

- Matt Weidner's ["Text Without CRDTs"](https://mattweidner.com/2025/05/21/text-without-crdts.html)
- Figma's ["How Figma's multiplayer technology works"](https://www.figma.com/blog/how-figmas-multiplayer-technology-works/)
- Martin Kleppmann's CRDT research
