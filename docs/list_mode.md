# `list_mode` Design & Implementation (WIP)

> Status: Experimental on branch `merk-list-mode`. Stable for new DB creation only. Backward compatibility with pre-fork databases is intentionally **not** supported.

## 1. Motivation

`list_mode` enables positional (index-based) semantics over a Merk AVL/Merkle tree without storing sequential integer keys, allowing:
- O(log n) index lookup using subtree sizes + parent pointers (ephemeral).
- Insert/delete by position without key re-writing.
- Potential future cryptographic proofs of positions (Merkle binding of size metadata).

## 2. Data Model Summary

Additional per-node metadata:
| Field | Type | Persisted | Hashed | Purpose |
|-------|------|-----------|--------|---------|
| `list_mode` | bool | Yes (encoded sentinel) | Yes (implicit via hash domain separation) | Marks subtree as list structure |
| `subtree_size` | u64 | Yes | Yes | Size of the subtree rooted at node (>=1) |
| `parent_key` | Option<Vec<u8>> | **Yes** (Model 2b) | **Yes** (Model 2b) | Parent UUID for upward traversal; enables position computation without root descent |
| `child_side` | Option<bool> | No | No | Ephemeral: indicates left(true)/right(false) relative to parent during in-memory operations |

### Persisted vs Ephemeral (Model 2b)
- **Persisted & Hashed**: `list_mode`, `subtree_size`, `parent_key`, `left_child`, `right_child`.
- **Ephemeral**: `child_side` (reconstructed during load or mutation).
- Persisting parent pointers enables O(log n) position lookup via upward climb without requiring root-to-leaf descent.
- Parent pointers are included in the node hash to ensure structural integrity and prevent tampering.

## 2.5 Operating Models

There are two conceptual operating modes for list semantics. The current implementation targets **Model 2**.

### Model 1: Order-Statistic with Structured Keys (NOT IMPLEMENTED YET)
Position semantics coexist with meaningful, monotonically ordered keys.
- Keys chosen from a sparse numeric or lexicographically sortable space (e.g. 128-bit integers) so that inserting between two positions can allocate a midpoint key without rewriting neighbors.
- In-order traversal over keys equals positional ordering; position can be inferred from key ordering.
- Enables: key-based range queries, mixed key/position proofs, potential interoperability with existing key-centric APIs.
- Complexity: midpoint key generation, compaction when local key space exhausted, stricter invariants for key monotonicity.

### Model 2: Structure-Only Position (CURRENT)
Keys are meaningless (random UUIDs) and only the tree shape (in-order traversal) defines positional indices.
- Simpler insertion: generate a random key, attach, update subtree sizes.
- Position proofs rely on subtree_size hashing; key order irrelevant.
- No stable key <-> position mapping; keys cannot be used for ordering or range queries.
- Suitable when solely positional access and proofs are required.

#### Model 2a: BST-Ordered (Original)
- Maintains BST invariant: keys are compared during traversal to preserve binary search tree ordering.
- In-order traversal respects key comparisons, enabling O(log n) key lookup.
- Random UUIDs still used (no semantic ordering), but tree structure enforces lexicographic BST property.
- **Challenge**: Positional insertion requires generating keys that fall between predecessor/successor in key space.

#### Model 2b: Non-BST, RocksDB-Backed (Prototype Target)
- **Abandons BST invariant**: tree structure is purely positional; keys are arbitrary UUIDs with no ordering constraint.
- Parent pointers are **persisted** (not ephemeral) to enable upward traversal without root descent.
- Direct key lookup via RocksDB get (amortized near-constant with caching; O(1) best case with cache hit, O(log n) worst case across LSM levels) retrieves node with parent/child UUIDs.
- Positional operations (insert_at, delete_at) use subtree_size to navigate; no key comparisons.
- **Pro**: No key generation constraints; keys never need reassignment or "midpoint" calculation.
- **Con**: Tree traversal requires multiple RocksDB fetches (slower than in-memory BST descent); balancing updates more nodes (parent pointers).
- **Use case**: Collaborative editing where each character/element has a stable, externally-committed UUID (via auxiliary proof system) and position is fluid.

**Current Prototype**: Model 2b (non-BST, UUID keys, persisted parent pointers).

### High-Level Trade-offs
| Aspect | Model 1 (Structured Keys) | Model 2a (Random Keys, BST) | Model 2b (Random Keys, Non-BST) |
|--------|---------------------------|----------------------------|----------------------------------|
| Implementation complexity | Higher | Medium | Lower |
| Positional insert/delete | Yes | Yes | Yes |
| Key-based range queries | Yes | No | No |
| Merkle proofs by position | Yes | Yes | Yes |
| Merkle proofs linking key to position | Yes | Yes (per-root) | Yes (per-root) |
| Direct key lookup efficiency | O(log n) BST | O(log n) BST | O(1)~O(log n) RocksDB† |
| Tree traversal cost | In-memory | In-memory | Multiple RocksDB fetches |
| Key generation constraints | Midpoint allocation | Midpoint allocation | None (arbitrary UUID) |
| Parent pointer persistence | No | No | Yes (required) |
| Rebalancing cost | Medium | Medium | Higher (update parent pointers) |
| Suitable for external key commitments | Weak | Weak | Strong (no reassignment) |
| Risk of future refactor | Lower | Medium | Higher if BST needed later |

† RocksDB lookup: O(1) best case (cache hit), O(log n) worst case (LSM tree levels), amortized near-constant with good cache/bloom filter tuning.

**Current Direction:** Prototype Model 2b for collaborative editing with externally-committed UUIDs; optionally migrate to Model 1 or 2a if key ordering becomes a requirement.

## 3. Encoding Format

For non-list-mode nodes the legacy encoding is unchanged.

For list-mode nodes (Model 2b with persisted parent) the serialized bytes are:
```
+---------+----------------+-------------------+----------------------+ 
| 0xFF    | subtree_sizeLE | parent_key_option | legacy_inner_encoding | 
+---------+----------------+-------------------+----------------------+ 
   1 byte      8 bytes         1 + 0|16 bytes      (TreeNodeInner via ed::Encode)
```
- The sentinel byte `0xFF` marks list-mode.
- `subtree_size` is little-endian `u64`.
- `parent_key_option`: 1 byte (0x00 = None, 0x01 = Some) + optional 16-byte UUID.
- All remaining bytes follow existing `TreeNodeInner` encoding (`left`, `right`, `kv`), where `left` and `right` are now interpreted as child UUIDs (not sorted by key comparison).

### Length Functions
- `TreeNode::encoding_length()` returns `1 + 8 + 1 + (parent ? 16 : 0) + legacy_len` for list-mode nodes.
- Legacy nodes retain prior length calculation.

## 4. Hashing Strategy

List-mode node hash (Model 2b) uses a distinct domain and binds subtree size and parent pointer:
```rust
node_hash_list_mode(
    kv_hash, left_child_hash, right_child_hash, subtree_size, parent_key_opt
)
```
Implementation detail (`hash.rs`):
- Domain separation byte `0xA5` prepended.
- Layout hashed: `0xA5 || kv_hash || left_hash || right_hash || subtree_size_le_bytes || parent_key_opt_bytes`.
- Ensures positional metadata (`subtree_size`) and structural integrity (`parent_key`) are cryptographically committed.
- Parent pointer inclusion prevents tampering with upward links and ensures proof verifiers can validate entire path.

Non-list nodes use original `node_hash()` unchanged.

## 5. Core Operations & Invariants (Model 2b Non-BST)

### Creation
```rust
TreeNode::new_list_node(value: Vec<u8>)
```
- Generates a random UUID key (arbitrary, no ordering constraint).
- Initializes `list_mode = true`, `subtree_size = 1`, `parent_key = None`.

### Key Lookup
- Direct fetch from RocksDB: `storage.get(key)` returns node blob.
- Complexity: O(1) best case (cache hit), O(log n) worst case (LSM tree levels); amortized near-constant with hot cache.
- Decoding yields: `value`, `left`, `right`, `parent_key`, `subtree_size`.
- No tree traversal needed for key-based access.

### Position Lookup
```rust
position_of_key(key, fetch: FnMut(&[u8]) -> Option<TreeNode>) -> Option<u64>
```
Algorithm:
1. Fetch node by key from RocksDB.
2. Initialize `pos = size(left_child)` (elements before current in its subtree).
3. Walk up parent chain via `parent_key`:
   - If current node is right child of parent: `pos += 1 + size(parent.left_child)`.
   - If current node is left child: no change.
4. Continue until root (parent_key = None).
5. Return `pos`.

Complexity: O(h) fetches where h = tree height ≈ log n; each fetch O(1) amortized (cache) to O(log n) worst case (LSM).

### Positional Insert
```rust
insert_at(pos: u64, value: Vec<u8>, fetch: ..., write: ...) -> Result<UUID, Error>
```
Algorithm:
1. Traverse from root using subtree_size:
   - At node: `let L = size(left_child)`.
   - If `pos <= L`: go left.
   - Else if `pos == L + 1` and node has free slot (e.g., no left child if inserting before): attach new leaf here.
   - Else: `pos -= L + 1`; go right.
2. Generate new UUID for new node.
3. Attach new node as left or right child of insertion parent.
4. Update new node: `parent_key = insertion_parent_key`, `subtree_size = 1`.
5. Walk up parent chain, updating `subtree_size += 1` for each ancestor.
6. Write all modified nodes back to RocksDB.
7. Return new UUID.

Complexity: O(log n) fetches + O(log n) writes; each operation amortized near-constant with cache.

### Positional Delete
```rust
delete_at(pos: u64, fetch: ..., write: ...) -> Result<(), Error>
```
Algorithm:
1. Traverse to node at position `pos` using subtree_size descent.
2. Remove node:
   - If leaf (no children): detach from parent.
   - If one child: replace node with child.
   - If two children: find in-order successor, swap, then delete successor.
3. Update parent's child pointer.
4. Walk up parent chain, updating `subtree_size -= 1` for each ancestor.
5. Write modified nodes to RocksDB; delete removed node.

Complexity: O(log n).

### Insert After Key
```rust
insert_after_key(key: &[u8], value: Vec<u8>, ...) -> Result<UUID, Error>
```
1. Fetch node by key.
2. Compute its position via `position_of_key(key)`.
3. Call `insert_at(position + 1, value)`.

### Subtree Size Maintenance
Invariant: For a list-mode node,
```
subtree_size = 1 + size(left) + size(right)
```
Recomputation is triggered after every insert, delete, or structural change; ancestors are updated via parent pointer climb.

### Rotations / Balancing
- Optional: Implement AVL rotations to maintain O(log n) height.
- Rotation must update parent pointers for rotated nodes and their children.
- Deferred in initial prototype (accept linear degradation for simplicity).

## 6. Persistence Workflow

### Commit (`MerkCommitter.write`)
- Uses `TreeNode::encode_into`, which applies sentinel format only if `list_mode = true`.
- Storage cost updated to include sentinel + 8 bytes for list-mode nodes.

### Load (`TreeNode::decode` / `decode_into`)
- Peeks first byte: if `0xFF`, reads the next 8 bytes as `subtree_size`, then decodes the remainder as legacy inner encoding and sets `list_mode = true`.
- Otherwise falls back to legacy decoding (`list_mode = false`).

## 7. Error Handling
- If a list-mode sentinel appears without enough bytes for subtree size (length < 9), decoding returns `ed::Error::UnexpectedByte(0xFF)`.
- `subtree_size` must be ≥ 1 for a valid list-mode node (enforced logically through creation paths; explicit validation may be added later).

## 8. Security & Integrity Considerations
- Subtree size hashing prevents silent positional tampering.
- **Parent pointers are hashed** (Model 2b): ensures structural integrity and prevents malicious relinking attacks.
- Random UUID keys with no ordering constraint eliminate risk of key-space exhaustion or forced reassignment attacks.
- External proof systems can commit to UUID keys independently; tree structure remains verifiable via Merkle proofs without exposing key generation logic.

## 9. Performance Notes
- Additional per-node overhead: +9 bytes (sentinel + size) + 1 + 16 bytes (parent pointer option + UUID) = ~26 bytes for list-mode nodes.
- Hash cost: similar to regular nodes (includes parent pointer bytes in hash input).
- Position lookup complexity: O(h) RocksDB fetches (parent climb, h ≈ log n tree height); each fetch amortized O(1) with cache, O(log n) worst case.
- Insertion/deletion: O(h) RocksDB fetches for traversal + O(h) writes for ancestor subtree_size updates + parent pointer updates.
- RocksDB get performance: typically 1-3 disk I/Os (SSD) per fetch; with block cache hits → near-constant.
- Trade-off: Slower than in-memory BST traversal but eliminates key ordering constraints and enables stable external key commitments.

## 10. Roadmap
| Phase | Goal | Status |
|-------|------|--------|
| 1 | In-memory list_mode fields & subtree sizes | Complete |
| 2 | Persistent sentinel encoding & hash binding | Complete |
| 3 | Positional insert/delete API | Pending |
| 4 | Deletion/shift strategies (gap handling) | Pending |
| 5 | Positional proofs (API surface) | Pending |
| 6 | Benchmarks (access & mutation) | Pending |
| 7 | Optional parent pointer caching layer | Pending |

## 11. Future Enhancements
- Batch positional insertion (minimizing rotations).
- Range proofs by index (combine subtree size commitments).
- Compact encoding variant (varint subtree size when small) for space savings.
- Optional serialization of a root-level list length for O(1) whole-list size queries.

## 12. Referenced Code Locations
| Concern | File | Symbol |
|---------|------|--------|
| List node creation | `merk/src/tree/mod.rs` | `TreeNode::new_list_node` |
| Size recomputation | `merk/src/tree/mod.rs` | `recompute_subtree_size`, `recompute_subtree_sizes_recursive` |
| Conversion | `merk/src/tree/mod.rs` | `convert_subtree_to_list_mode` |
| Positional index | `merk/src/tree/mod.rs` | `compute_position_with_parent_fetch` |
| Hash binding | `merk/src/tree/hash.rs` | `node_hash_list_mode` |
| Encoding | `merk/src/tree/encoding.rs` | `encode`, `decode` |
| Commit write | `merk/src/merk/committer.rs` | `MerkCommitter::write` |

## 13. Testing Coverage
- New test: `encode_decode_list_mode_leaf_with_sentinel` (encoding roundtrip).
- Existing test: `list_mode_parent_pointers_and_positions` ensures subtree sizes & positional indices.
- All legacy tests pass with `list_mode` feature enabled (non-list nodes remain legacy format).

## 14. Open Questions
- Should subtree_size use varint to reduce space when small? (Trade-off: complexity vs 8 static bytes.)
- Do we need invariant revalidation on load for defensive consistency? (Current approach trusts written state.)
- Parent pointer persistence: intentionally excluded—any future need would require cycle prevention & integrity hash strategy.

## 15. Usage Notes
- Enable feature: `--features list_mode`.
- Create list subtree: start with `TreeNode::new_list_node` or convert existing subtree using `convert_subtree_to_list_mode`.
- Position lookup requires providing a fetch function for parent traversal when nodes may have been loaded lazily.

## Appendix: Model 2 (Random Key, Structure-Only Position) — Long-Term Considerations

*Applies to both Model 2a (BST) and Model 2b (non-BST); differences noted where relevant.*

### 1. Key Limitations
- **Key ordering is meaningless**: Keys are random UUIDs, so the in-order traversal defines position, but keys themselves do not encode or preserve order. You cannot reconstruct position from key alone.
- **No stable key for a given position**: Inserting or deleting at a position changes the tree structure, so the node at a given position may have a different key after rebalancing or after a sequence of edits.
- **No efficient range queries by key**: Since keys are random, you cannot use key ranges to fetch a slice of the list; you must traverse the tree by position.
- **Direct key lookup**: Model 2a uses BST traversal (O(log n) in-memory); Model 2b uses RocksDB get (amortized O(1) with cache, O(log n) worst case LSM tree).

### 2. Merkle Proofs
- **Insertion/Deletion Proofs**: You can prove that a node at a given in-order index existed (or was removed) because subtree_size participates in the hashed state.
    - *Insertion proof*: Provide a Merkle path for the new leaf plus sibling hashes and subtree_size values allowing the verifier to recompute the index and new root.
    - *Deletion proof*: Provide the prior path (or absence proof) plus updated sibling subtree_size values showing the resulting root.
- **Key + Position (per root) Proofs**: For a *specific* root you can prove: "the node with key K (and value V) is at position i" by supplying the standard membership path augmented with subtree_size metadata. The verifier recomputes `i` while verifying the path.
- **What is *not* provided**: Stability of that position across later updates; after insertions/deletions elsewhere K may move to a different index and prior proofs become historical only.

### 3. Range Proofs
- **Range by position**: You can construct a proof that a contiguous range of positions (e.g., 10..20) contains a specific set of elements, by traversing the tree and collecting the relevant nodes and their Merkle paths.
- **Range by key is not meaningful**: Since keys are random, range proofs by key are not possible or meaningful in this model.

### 4. Other Long-Term Considerations
- **Rebalancing changes positions**: AVL rotations may change the structure, so the same key may appear at a different position after rebalancing. Only the in-order traversal defines position.
- **Direct key lookup supported**: Both variants support exact key lookup (Model 2a via BST, Model 2b via RocksDB), but no ordering/range semantics from keys.
- **Interoperability**: If you ever need to interoperate with systems that expect key-based ordering or range queries, Model 2 will not be compatible.

### 5. Summary Table
| Feature                        | Model 2a (BST) | Model 2b (Non-BST) |
|-------------------------------|:---------------:|:------------------:|
| Insert/delete by position      | Yes            | Yes                |
| Merkle proof of position       | Yes            | Yes                |
| Merkle proof of key            | Yes (standard membership) | Yes (standard membership) |
| Key+position joint proof       | Yes (per-root; index derived) | Yes (per-root; index derived) |
| Range proof by position        | Yes (construct slice proof) | Yes (construct slice proof) |
| Range proof by key             | No (keys lack ordering meaning) | No (keys lack ordering meaning) |
| Direct key lookup              | Yes (BST O(log n)) | Yes (RocksDB ~O(1) cached) |
| Stable key for position        | No             | No                 |
| Interop with key-ordered trees | No             | No                 |
| Key reassignment needed        | Potentially (midpoint exhaustion) | Never          |

> **If you only need per-root positional & membership proofs and want stable externally-committed keys, Model 2b is optimal. For in-memory performance without key commitment constraints, Model 2a may be faster.**

**Footnote:** *“Position proof” here means verifying that during Merkle verification the accumulated subtree sizes along the provided path yield the claimed in-order index. The key `K` in the leaf is part of the recomputed `kv_hash`, so membership and index are jointly bound to the same root hash.*

---
*This document will evolve as positional mutation APIs and proofs are added.*

## 6. Implementation Status
