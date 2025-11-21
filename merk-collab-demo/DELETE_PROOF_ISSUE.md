# Delete Operation Proof Verification Issue (Resolved)

> **Status (November 21, 2025): RESOLVED.**
>
> Delete proofs now verify end-to-end after the `merk` fixes that recompute hashes for loaded nodes and the server-side switch to pure `UpdateValueByKey` tombstones. The auditor consumes the changelog without mismatches, and the targeted test `test_delete_key_proof_contains_tombstone` passes in CI. The investigation details below are kept for historical context.

---

## Historical Investigation (November 17, 2025)

**Severity**: CRITICAL - Auditor could not verify delete operations at the time

## Problem Summary

When a delete operation is performed on a Merk tree (marking a node as tombstone via `UpdateValueByKey`), the proof verification fails with a root hash mismatch. The server broadcasts one root hash, but the auditor's proof verification computes a different root hash.

## Reproduction

**Simplest case** (from latest test):
1. Client 1 inserts "A" → Auditor ✓ PASSES
2. Client 2 deletes "A" → Auditor ✗ FAILS

**Server logs show**:
```
[DELETE] Root hash after prove: 24f70d6e45698044ec41c168fb99fe283174fbd928ffe0cad4069946ee565f93
Broadcasting: root_hash=24f70d6e45698044ec41c168fb99fe283174fbd928ffe0cad4069946ee565f93
```

**Auditor logs show**:
```
expected_root=24f70d6e45698044ec41c168fb99fe283174fbd928ffe0cad4069946ee565f93
proof_root=08d48f30e2f697277c376c7ba1a7369246d3980a6b0656fe2f0792e16ffb8399
Root hash mismatch!
```

## Technical Details

### What Delete Does

1. **Server** (`delete_by_uuid` in document.rs):
   - Uses `UpdateValueByKey` operation to change value from `[0, char]` to `[1, char]` (tombstone)
   - Applies batch: `merk.apply_list_batch(&[op], &grove_version)`
   - Generates proof: `merk.prove(Query::new_single_key(uuid), None, &grove_version)`
   - Broadcasts root hash from: `merk.root_hash()`

2. **Auditor** (`verify_delete_operation` in auditor/main.rs):
   - Receives proof bytes and expected root hash
   - Executes proof: `query.execute_proof(proof_bytes, None, true)`
   - Extracts root hash from proof result
   - Compares: `proof_root == expected_root`

### The Core Issue

**The server's `root_hash()` and `prove()` are computing different root hashes for the same tree state.**

Both should be looking at the same committed tree after `apply_list_batch()` returns, but they produce different results:
- `root_hash()` → Returns hash X (broadcast)
- `prove()` internally computes → Returns hash Y (in proof)
- X ≠ Y

## Investigation History

### Attempt 1: Tree Reload Theory (Failed)
**Hypothesis**: After commit, we needed to reload the tree from storage to get fresh links.

**Action**: Added `self.reload_tree_from_storage()` after commit.

**Result**: FAILED - Storage layer wasn't flushed, couldn't reload.

**Learning**: Not the right approach; commit should handle link updates in-memory.

---

### Attempt 2: Node Index Timing (Red Herring)
**Hypothesis**: `rebuild_node_index()` was being called before commit, capturing nodes with `Link::Modified` (which have no cached hash). When proof generation used `get_node_from_index()`, it got stale nodes.

**Action**: Moved `rebuild_node_index()` call from within batch loop to after commit (line 1202 in list_ops.rs).

**Result**: Tests pass, but **proof verification still fails** for real delete operations.

**Learning**: **Proof generation doesn't use the node index at all!** It walks the tree directly via `RefWalker`. The node index fix was irrelevant to the actual problem.

---

### Attempt 3: Hash Recomputation on Load (Partial Fix?)
**Hypothesis**: When `RefWalker` walks the tree during proof generation, it encounters `Link::Reference` links (pruned links pointing to nodes in storage). When it calls `tree.load()` to fetch these nodes, the code was **copying the cached hash from the Reference link into the new Loaded link**. If the Reference link had a stale hash (from before an update operation), the proof would compute the wrong root hash.

**Action**: Modified `tree.load()` in tree/mod.rs (around line 2070) to recompute the hash from the loaded tree:

```rust
// OLD CODE (line 2078-2082):
*self.slot_mut(left) = Some(Link::Loaded {
    tree,
    hash: *hash,  // Uses stale hash from Reference link!
    child_heights: *child_heights,
    aggregate_data: *aggregate_data,
});

// NEW CODE:
let recomputed_hash = tree.hash().unwrap_add_cost(&mut cost);
*self.slot_mut(left) = Some(Link::Loaded {
    tree,
    hash: recomputed_hash,  // Use freshly computed hash
    child_heights: *child_heights,
    aggregate_data: *aggregate_data,
});
```

**Result**: All 272 merk unit tests pass, but **proof verification still fails** for real delete operations.

**Learning**: The fix might be correct for the load scenario, but the actual problem with deletes is something else.

---

## Current State (November 17, 2025)

### Latest Test Results

**Single node delete** (Insert A, Delete A):

Server correctly shows:
```
[UPDATE_VALUE_BY_KEY] Before update - left_link: None, right_link: None
[UPDATE_VALUE_BY_KEY] After update - left_link: None, right_link: None
[AFTER UPDATE_VALUE_BY_KEY] left_link: None, right_link: None
[COMMIT] Node key=302f86f6708d4f5d, left_link=None, right_link=None
```

All links remain None (no children), kv.hash is updated correctly:
```
Old kv.hash: 95be46c4ade1d5b3c8bfeaad59053dd76a0c29c68d9e48735e4a23c7554809c2
New kv.hash: ccc0fd9d691d99969a6c6125b7d49fbb9287874a271b1a74e12cc5a40956624e
```

But proof verification **still fails** with different root hash:
```
Server: 24f70d6e45698044ec41c168fb99fe283174fbd928ffe0cad4069946ee565f93
Proof:  08d48f30e2f697277c376c7ba1a7369246d3980a6b0656fe2f0792e16ffb8399
```

### Key Observations

1. **Insert operations work perfectly** - Auditor verifies inserts successfully
2. **Only delete operations fail** - All delete operations fail proof verification
3. **No structural changes** - Delete uses `UpdateValueByKey`, which doesn't change tree structure
4. **Links are correct** - Logs confirm links remain correct throughout the operation
5. **Commit writes correctly** - New kv_hash is written to storage
6. **Single node failure** - Even the simplest case (single node, no children) fails

### The Mystery

For a **single node with no children**, the root hash should be straightforward:
- `tree.hash()` = `node_hash(kv.hash, NULL_HASH, NULL_HASH)` (or similar for list mode)

The kv.hash changes from `95be46c4...` to `ccc0fd9d...` during the update, which is correct.

**But why do `root_hash()` and `prove()` compute different results?**

Both call `tree.hash()` on the same tree:
- `root_hash()` uses `use_tree()` (immutable borrow)
- `prove()` uses `use_tree_mut()` (mutable borrow) 

Both temporarily take the tree from the `Cell`, use it, and put it back. They should see the same tree state.

## Hypotheses to Investigate

### Theory 1: Tree State Mutation During Prove
`prove()` uses `RefWalker` which can mutate the tree (converting Reference→Loaded). Maybe this mutation affects the hash calculation in a way we don't understand?

### Theory 2: List Mode Hash Calculation Issue
The hash function for list mode includes additional data:
```rust
node_hash_list_mode(kv.hash, left_hash, right_hash, subtree_size, parent_key)
```

Maybe subtree_size or parent_key is different between `root_hash()` and `prove()`?

### Theory 3: Proof Encoding/Decoding Issue
Maybe the proof is correctly generated but incorrectly encoded/decoded, causing the auditor to compute the wrong hash?

### Theory 4: Storage vs Memory Inconsistency
Maybe `prove()` is reading from storage (via `RefWalker.load()`) and getting old data, while `root_hash()` is using the in-memory tree?

### Theory 5: Cell/Interior Mutability Bug
The `Cell<Option<TreeNode>>` usage in Merk might have a subtle bug where the tree state is different between `use_tree()` and `use_tree_mut()` calls?

## Files Modified

1. `merk/src/tree/mod.rs`:
   - Line ~2070: Modified `tree.load()` to recompute hash instead of using cached hash
   - Line ~1960: Added diagnostic logging in `commit()`
   - Line ~2030: Fixed logging to handle short keys

2. `merk/src/merk/list_ops.rs`:
   - Line ~906: Already had `rebuild_node_index()` before batch
   - Line ~1202: Added `rebuild_node_index()` after commit (from Attempt 2)
   - Line ~1127: Added diagnostic logging after `update_value_by_key`
   - Line ~73: Added `Link` to imports

3. `merk/src/merk/committer.rs`:
   - Line ~32: Added diagnostic logging, fixed for short keys

4. `merk/src/tree/mod.rs` (update_value_by_key):
   - Line ~830-860: Added extensive diagnostic logging for link types

## Next Steps

1. **Add hash calculation logging**: Log the exact inputs and outputs of the hash functions in both `root_hash()` and during proof generation
2. **Compare proof internals**: Examine what the proof actually contains vs what it should contain
3. **Trace RefWalker**: Add logging to see what RefWalker does during proof generation
4. **Verify subtree_size**: Check if subtree size is being updated correctly during UpdateValueByKey
5. **Test with storage disabled**: See if the issue persists without storage layer involvement

## Code References

- **Server delete**: `merk-collab-demo/server/src/document.rs` lines 245-290
- **Auditor verify**: `merk-collab-demo/auditor/src/main.rs` lines 297-350
- **UpdateValueByKey**: `merk/src/tree/mod.rs` lines 795-890
- **apply_list_batch**: `merk/src/merk/list_ops.rs` lines 895-1210
- **tree.load()**: `merk/src/tree/mod.rs` lines 2049-2090
- **root_hash()**: `merk/src/merk/mod.rs` lines 320-326
- **prove()**: `merk/src/merk/prove.rs` lines 78-110
- **RefWalker**: `merk/src/tree/walk/ref_walker.rs` lines 23-83

## Conclusion

After 3 days of investigation and multiple attempted fixes:
- We understand the Link lifecycle (Modified→Loaded→Reference)
- We fixed the node index timing (though it was irrelevant to proofs)
- We fixed the hash recomputation on load (necessary but insufficient)
- **The core issue remains**: `root_hash()` and `prove()` produce different hashes for the same tree state after a delete operation

The problem is **reproducible 100% of the time** with any delete operation, including the simplest single-node case. This suggests a fundamental issue with how hashes are computed or cached, rather than a complex edge case.
