# Known Issues and Future Work

## Current Status: ✅ DEMO FULLY FUNCTIONAL!

The merk-collab-demo is **working end-to-end**! You can run the server and client, open multiple browser tabs, and see real-time collaborative editing in action.

### ✅ What's Working

- **Server**: Rust backend with Axum + Merk compiles and runs successfully
- **Client**: TypeScript/Vite frontend with real-time WebSocket updates
- **Multi-client sync**: Type in one browser tab, see updates in all tabs
- **Client-generated UUIDs**: Optimistic updates with no server delay
- **Real Merkle proofs**: Server generates authentic cryptographic proofs
- **Changelog**: Server writes append-only audit trail with proofs
- **Root hash tracking**: Clients maintain authoritative state via root hash
- **Broadcast system**: All operations broadcast to all connected clients
- **Beautiful UI**: Modern interface with connection status and statistics
- **Complete documentation**: README, BUILD, IMPLEMENTATION, QUICKSTART, and STATUS guides

### Architecture Notes

**Signature-Based Trust Model** (Placeholder Comments):
- In a production system, clients would sign operations with user keys (Ed25519 or similar)
- Other clients would verify signatures to authenticate authorship
- This demo uses placeholder comments showing where crypto would go
- Focus is on Merk list-mode functionality, not user authentication

**Client-Side Simplicity**:
- Clients do NOT verify Merkle proofs (no WASM complexity)
- Clients trust server for operation ordering (server is authoritative)
- Clients track root hash for consistency reference
- Much simpler architecture than full client-side verification

**Audit Trail**:
- Server generates Merkle proofs for all operations
- Proofs written to `changelog.jsonl` file
- External auditors can verify server behavior
- Proofs ensure server hasn't tampered with tree
- Enables independent verification without client complexity

### 🎯 Design Decisions

**Why No Client-Side Proof Verification?**
1. **Simpler architecture**: No WASM complexity, no build issues
2. **Realistic trust model**: Clients trust server ordering (most systems work this way)
3. **Audit trail preserved**: Proofs still generated for external verification
4. **Performance**: No client-side crypto overhead
5. **User authentication**: Signatures prove authorship, not tree membership

**Why Changelog Instead of Client Verification?**
1. **Independent audit**: External verifiers can check server behavior
2. **Compliance**: Audit trail for regulatory requirements
3. **Tamper detection**: Merkle proofs catch server corruption
4. **Scalability**: Auditors run offline, don't impact real-time performance

### ⚠️ Current Limitations & Design Decisions

#### 1. **Pure Reference-Based Operations with Position Discovery**

**Current approach**: 
- Protocol uses reference-based operations (`InsertAfterKeyWithKey`, `UpdateValueByKey`)
- No position conversion needed - operations work directly with UUIDs
- After operations complete, server discovers actual tree position for proof generation

**Implementation**:
- `InsertAfterKeyWithKey`: Direct insertion after target UUID
- `UpdateValueByKey`: In-place value update for tombstones (no structural changes)
- Position discovery: After insertion, iterate positions 0-N to find where UUID landed
- Proof generation: Generate proof at discovered position

**Why position discovery?**:
- Tree rebalancing can change positions during insertion
- Cache position (target + 1) may differ from actual tree position
- Solution: Query tree after insertion to find actual position
- Ensures proofs contain correct UUID at correct position

**Benefits**:
- ✅ Pure reference-based (Matt Weidner's design)
- ✅ Reliable after tombstone operations (UpdateValueByKey = no structural change)
- ✅ Correct proofs (position discovered, not predicted)
- ✅ Client generates UUIDs (zero-latency typing)

**Performance considerations**:
- Position discovery is O(n) scan of tree (acceptable for demo scale)
- For production: Add UUID→position index or query API in Merk
- Alternative: Key-based proofs instead of positional proofs

#### 2. **Storage Backend** (Server)

**Current**: Uses `TempStorage` (in-memory, document reset on server restart)

**For production**: Switch to `RocksDbStorage` for persistence

**Why TempStorage for demo**:
- RocksDB transactions aren't `Send`, causing issues with Axum's async handlers
- Used `unsafe impl Send + Sync for Document` as workaround
- TempStorage is simpler and sufficient for demonstration purposes
- Avoids complexity of managing RocksDB lifetime with async code

**To enable RocksDB persistence**:
```rust
// In document.rs, replace TempStorage initialization with:
let storage = RocksDbStorage::default_rocksdb_with_path(path)?;
```

**Note**: You'll need to handle the `Send` trait carefully, options include:
- Use `tokio::spawn_blocking` for all document operations
- Use message-passing architecture (send operations to dedicated thread)
- Accept `unsafe impl Send + Sync` (current approach, safe with Mutex protection)

#### 3. **Signature Verification** (Not Implemented)

**Current**: Placeholder comments show where crypto would go

**For production**: 
- Generate Ed25519 key pairs for each user
- Sign operations with private key: `sign(private_key, operation)`
- Include signature and user ID in messages
- Clients verify: `verify(user_public_key, operation, signature)`
- Server could also verify to prevent spam

**Why not in demo**:
- Focus on Merk list-mode functionality
- Crypto libraries add complexity
- Comments clearly show where it belongs

#### 4. **Multi-Character Operations**

**Current**: Paste is disabled, can only type one character at a time

**Why**: Would need batch operations to insert multiple characters efficiently

**Solution**:
```rust
// Add to ListOp enum:
InsertMultipleAtPosition {
    position: u64,
    keys: Vec<Vec<u8>>,
    values: Vec<Vec<u8>>,
}
```

#### 4. **Auditor Tool** (Optional)

**Current**: Not implemented

**Future**: Standalone Rust tool to verify changelog

**Would do**:
```rust
// Read changelog.jsonl line by line
// For each entry:
//   1. Deserialize: [op_index, op, proof, new_root_hash]
//   2. Verify proof against previous root hash
//   3. Update root hash for next iteration
//   4. Report any mismatches
```

**Benefits**:
- Independent verification of server behavior
- Catch server bugs or tampering
- Compliance/audit requirements
- Demonstrates external verification capability

#### 5. **Conflict Resolution**

**Current**: Last-write-wins (server processes operations sequentially)

**Impact**: If two clients type at same position simultaneously, one may be lost or mispositioned

**Solutions**:
- Operational Transformation (OT)
- CRDT-based conflict resolution
- Position tracking with vector clocks
- For this demo: acceptable as proof-of-concept

### 🎯 Quick Start (Fully Working!)

```bash
# Terminal 1: Start server
cd merk-collab-demo/server
cargo run --release
# Server starts on http://127.0.0.1:3000

# Terminal 2: Start client  
cd merk-collab-demo/client
npm install
npm run dev
# Client at http://localhost:5173

# Open multiple browser tabs and start typing!
```
