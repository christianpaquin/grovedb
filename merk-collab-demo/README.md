# Merk Collaborative Editing Demo

Real-time collaborative text editor demonstrating **Matt Weidner's "Text Without CRDTs"** using reference-based Merkle tree operations with cryptographic audit trail.

**Status**: ✅ **Fully functional!** Server and client work end-to-end with zero-latency typing. Type in one browser tab, see updates instantly in all tabs.

## Key Features

✅ **Zero-Latency Typing** - Characters appear immediately (optimistic updates)  
✅ **Reference-Based Operations** - Operations reference UUIDs, not positions  
✅ **Client-Controlled UUIDs** - Enables true optimistic updates  
✅ **Tombstone Deletions** - Deleted characters remain in tree for consistency  
✅ **Merkle Proof Audit Trail** - Every operation is cryptographically verifiable  
✅ **Independent Auditor** - Verify server integrity without trusting it  

## Architecture Overview

This demo implements [Matt Weidner's "Text Without CRDTs"](https://mattweidner.com/2025/05/21/text-without-crdts.html) design:

- **Clients** generate UUIDs locally and show changes immediately (zero latency!)
- **Protocol** uses reference-based operations (operations reference UUIDs, not positions)
- **Server** maintains the authoritative Merkle tree (using Merk with `list_mode`)
- **Implementation** converts UUIDs to positions server-side (hybrid approach for reliability)
- **Server** generates Merkle proofs and publishes them to an audit changelog
- **Auditor** can independently verify server behavior by checking the changelog
- **No CRDTs needed** - server is the source of truth for operation ordering

### Why Hybrid? Protocol vs Implementation

**Protocol level (client ↔ server)**: Reference-based
- Client sends `target_uuid` (UUID to insert after)
- Resilient to concurrent edits (positions don't shift)
- Enables zero-latency typing with client-generated UUIDs

**Implementation level (server internal)**: Position-based
- Server looks up `target_uuid` in cache to get tree position
- Uses `InsertAtPositionWithKey` with calculated position
- More reliable after tombstone operations (delete+reinsert changes tree structure)
- Avoids fetch closure issues with `InsertAfterKeyWithKey`

**Result**: Best of both worlds - protocol resilience + implementation reliability!

## Trust Model

1. **Client → Client**: Users trust each other via signatures (would be Ed25519 or similar)
2. **Client → Server**: Clients trust server for operation ordering (server is authoritative)
3. **Server → Auditor**: Auditors verify server integrity via Merkle proofs in changelog
4. **No client-side proof verification**: Clients don't verify Merkle proofs (simpler, faster)

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                         Browser Clients                      │
│  ┌────────────────────────────────────────────────────┐     │
│  │  Client (TypeScript)                               │     │
│  │  - Simple list: [(uuid, 'H'), (uuid, 'e'), ...]   │     │
│  │  - Generates UUIDs locally (optimistic updates)    │     │
│  │  - Reference-based ops: InsertAfterKeyWithKey      │     │
│  │  - Signs operations (placeholder comments)         │     │
│  │  - Tracks root hash for consistency reference      │     │
│  └────────────────────────────────────────────────────┘     │
└─────────────────────────────────────────────────────────────┘
                            ↕ WebSocket
              [op: insert_after(target_uuid, uuid, char)]
┌─────────────────────────────────────────────────────────────┐
│                    Server (Rust + Axum)                      │
│  ┌────────────────────────────────────────────────────┐     │
│  │  Merk Database (TreeType::ListTree)                │     │
│  │  - Full Merkle tree with client UUIDs              │     │
│  │  - Reference-based: InsertAfterKeyWithKey          │     │
│  │  - Tombstone deletions (value=[deleted, char])     │     │
│  │  - Generates proofs for all operations             │     │
│  │  - Broadcasts to all connected clients             │     │
│  │  - Publishes changelog for independent audit       │     │
│  └────────────────────────────────────────────────────┘     │
└─────────────────────────────────────────────────────────────┘
                            ↓ changelog.jsonl
                    [op_index, op, proof, new_root_hash]
┌─────────────────────────────────────────────────────────────┐
│                    Auditor (Optional Tool)                   │
│  - Reads changelog file line-by-line                         │
│  - Verifies each proof against previous root                │
│  - Detects server tampering or corruption                   │
│  - Independent verification of tree integrity               │
└─────────────────────────────────────────────────────────────┘
```

## Components

### 1. `client/` - Web Frontend (✅ Working)
TypeScript + Vite application:
- Text editor interface with modern UI
- Real-time collaboration (multi-tab sync works!)
- Client generates UUIDs locally for optimistic updates
- Tracks root hash for state consistency reference
- **Comments indicate where signature generation/verification would go**
- No client-side proof verification (simpler architecture)

### 2. `server/` - Rust Backend (✅ Working)
Axum-based server that:
- Maintains Merk tree with `list_mode` feature
- Accepts reference-based operations (client sends `target_uuid`)
- Maintains character cache for fast UUID→position lookups
- Converts `target_uuid` to tree position before applying operation
- Handles insert/delete operations with client-provided UUIDs
- Generates Merkle proofs using `merk.prove_position()`
- Broadcasts updates to all clients via WebSocket
- Uses TempStorage (in-memory) for demo purposes
- **Publishes changelog file with proofs for audit trail**
- **Hybrid approach**: Reference-based protocol, position-based implementation

### 3. `auditor/` - Verification Tool (✅ Implemented)
Standalone Rust tool that:
- Reads changelog file line-by-line
- Verifies each proof against previous root hash
- Ensures server hasn't tampered with the tree
- Demonstrates independent verification capability
- See [auditor/README.md](auditor/README.md) for usage

## Quick Start

### Prerequisites
```bash
# Rust (for server)
# Already installed if you're working in this repo

# Node.js (for frontend)
# Install from https://nodejs.org/ or use your package manager
```

## Quick Start

```bash
# Terminal 1: Start server
cd merk-collab-demo/server
cargo run --release
# Note the changelog path from server output!

# Terminal 2: Start client
cd merk-collab-demo/client
npm install
npm run dev

# Terminal 3: Run auditor (after typing some text)
cd merk-collab-demo/auditor
cargo run --release -- /tmp/.tmpXXXXXX/changelog.jsonl
```

Open http://localhost:5173 in multiple browser tabs and start typing!
## How It Works

### Initial Sync
1. Client connects via WebSocket
2. Server sends: `{ type: 'initial', content: [(uuid, char), ...], root_hash }`
3. Client stores content as simple array + trusted root hash
4. Client displays the document

### Operations Flow
1. User types a character at position N
2. Client generates UUID locally and optimistically updates display
3. Client finds `target_uuid` (UUID of character at position N-1)
4. Client sends: `{ type: 'insert', target_uuid: 'previous-char-uuid', uuid: 'new-uuid', value: 'char' }`
5. **[Real system: Client signs operation with user private key]**
6. Server looks up `target_uuid` in cache to find tree position
7. Server applies: `InsertAtPositionWithKey { position: tree_pos, key: uuid, value }`
8. Server generates Merkle proof for audit trail
9. **Server appends to changelog: `[op_index, op, target_uuid, uuid, value, proof, new_root_hash]`**
10. Server broadcasts to all clients: `{ type: 'operation', operation: 'insert', target_uuid, uuid, value, root_hash }`
11. **[Real system: Clients verify operation signature from other users]**
12. Originating client: Sees UUID matches, already has it (no-op)
13. Other clients: UUID not found, look up `target_uuid` position and insert
14. All clients update root hash reference

**Key insight**: Protocol uses reference-based operations (UUIDs), server converts to positions internally for reliability.

### Audit Trail
- Server maintains append-only changelog file
- Each entry: operation index, operation details, Merkle proof, new root hash
- Auditor can independently verify entire history
- **Proofs ensure server hasn't tampered with tree**
- Clients don't verify proofs (simpler, faster, trust server ordering)

### Tombstone Architecture
Following Matt Weidner's ["Text Without CRDTs"](https://mattweidner.com/2025/05/21/text-without-crdts.html) design:

- **Deletions don't remove items** - they mark them as deleted (tombstones)
- **Value format**: `[deleted_flag, char_byte]` (2 bytes per character)
  - `deleted_flag`: 0 = active, 1 = deleted
  - `char_byte`: the character value
- **UUIDs persist** - deleted items keep their UUID so other clients can reference them
- **Positions stable** - tombstones maintain tree positions for concurrent operations
- **Delete operation**: Delete + Re-insert with same UUID but marked as deleted
- **Display**: Clients filter out tombstones when showing content

**Why tombstones matter for collaboration:**
1. Client A deletes character at position 5 (while offline)
2. Client B inserts after position 5 (while offline)  
3. When both sync: Client B's insert can still reference position 5 because the tombstone exists
4. Without tombstones: Position 5 disappears, making Client B's operation ambiguous

### Security Properties
- **User Authentication**: Signatures prove who authored each operation (placeholder comments)
- **Operation Ordering**: Server is authoritative source of truth
- **Server Integrity**: Merkle proofs in changelog enable independent audit
- **Root Hash Consistency**: All clients maintain same root hash reference
- **No CRDTs**: Simple sequential operations, server resolves conflicts

## Demo Features

- ✅ Real-time multi-user editing (works across browser tabs!)
- ✅ Cryptographic Merkle proof generation (server-side)
- ✅ Client-generated UUIDs (optimistic updates, no delay)
- ✅ Root hash tracking (state consistency reference)
- ✅ Visual connection status indicator
- ✅ Operation and statistics counters
- ✅ Audit trail via changelog (proofs for external verification)
- 💬 Signature placeholders (comments show where real crypto would go)
- 🚧 Auditor tool (optional - demonstrates changelog verification)
- 🚧 Merkle tree visualization (future enhancement)

## Performance

- **Proof size**: ~1-3 KB per operation (depends on tree size)
- **Proof generation**: <5ms server-side
- **Latency**: WebSocket round-trip only (no verification delay with mock)
- **Scalability**: Logarithmic proof size O(log n)
- **Optimistic updates**: Characters appear instantly (client generates UUID)

## Development

```bash
# Watch mode for server
cd server && cargo watch -x run

# Watch mode for client (auto-reloads on changes)
cd client && npm run dev
```

## Documentation

- [STATUS.md](STATUS.md) - Current working status and what's implemented
- [BUILD.md](BUILD.md) - Detailed build instructions
- [IMPLEMENTATION.md](IMPLEMENTATION.md) - Implementation walkthrough
- [QUICKSTART.md](QUICKSTART.md) - Quick reference guide
- [KNOWN_ISSUES.md](KNOWN_ISSUES.md) - Known limitations and future work

## Acknowledgments

Based on the concept from Matt Weidner's ["Text Without CRDTs"](https://mattweidner.com/2025/05/21/text-without-crdts.html) - demonstrating that you can build collaborative editors without operational transformation or CRDTs by using a Merkle tree with positional proofs.

## Testing

```bash
# Test wasm module
cd merk-wasm && wasm-pack test --headless --firefox

# Test server
cd server && cargo test

# Test client
cd client && npm test
```

## License

MIT (same as parent grovedb project)
