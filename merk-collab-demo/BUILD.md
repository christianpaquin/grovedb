# Building and Running the Merk Collaborative Editor Demo

This guide walks you through building and running all three components of the demo.

## Prerequisites

- Rust (1.75 or later)
- Node.js (18 or later)
- wasm-pack (`cargo install wasm-pack`)

## Step 1: Build the WASM Module (Optional - Currently Using Mock)

The WASM module would provide proof verification in the browser. However, due to dependency constraints (merk's list_mode depends on uuid which requires special WASM configuration), the current implementation uses a **mock verifier** that always returns `true`.

**Current Status:** The demo works without real WASM verification - operations are still cryptographically secured on the server side, but clients trust operations without local verification.

**To enable real WASM verification in the future:**

```bash
cd merk-collab-demo/merk-wasm

# This will currently fail due to uuid/wasm incompatibility
wasm-pack build --target web --out-dir ../client/public/wasm
```

**What needs to be done:**
1. Extract proof verification logic into a separate module without uuid dependencies
2. Create a minimal `merk-verify` crate with only cryptographic verification
3. Build that crate to WASM

**For now, skip this step - the demo works with the mock verifier.**

## Step 2: Build and Run the Server

The server maintains the Merk tree and generates proofs.

```bash
cd merk-collab-demo/server

# Build the server
cargo build --release

# Run the server
cargo run --release

# The server will start on http://127.0.0.1:3000
# You should see logs indicating it's listening
```

The server provides:
- WebSocket endpoint at `ws://127.0.0.1:3000/ws`
- REST endpoint at `http://127.0.0.1:3000/document`
- Health check at `http://127.0.0.1:3000/health`

## Step 3: Run the Client

The client provides the web interface.

```bash
cd merk-collab-demo/client

# Install dependencies
npm install

# Start the development server
npm run dev

# The client will start on http://localhost:5173
```

## Step 4: Test the Demo

1. Open http://localhost:5173 in your browser
2. You should see "Connected" status in the top bar
3. Start typing in the text editor
4. Open another browser tab to the same URL
5. Type in one tab and watch it appear in the other!

### What to Observe

- **Root Hash**: Changes with every operation, shown in the status bar
- **Proof Count**: Increments as proofs are verified
- **Operations**: Total number of insert/delete operations
- **Real-time Sync**: Changes appear instantly in all connected clients

## Troubleshooting

### WASM Module Not Loading

If you see "WASM module not loaded, using mock verifier":

1. Check that `wasm-pack build` completed successfully
2. Verify files exist in `client/public/wasm/`
3. Update `DocumentClient.ts` to import the WASM module:

```typescript
import init, { verify_proof } from '../public/wasm/merk_wasm';

async init() {
  await init(); // Initialize WASM
  // ... rest of init
}
```

### Server Connection Fails

Check that:
- Server is running on port 3000
- No firewall blocking the connection
- WebSocket endpoint is accessible

### Build Errors

If you encounter missing Merk features:

1. Check that `list_mode` feature is enabled in merk
2. Verify positional proof functions are public
3. Ensure dependencies are up to date

## Production Build

To build for production:

```bash
# Build WASM
cd merk-wasm
wasm-pack build --target web --out-dir ../client/public/wasm --release

# Build server
cd ../server
cargo build --release

# Build client
cd ../client
npm run build

# Serve with a static file server
npx serve dist
```

## Development Tips

### Watch Mode

For active development, run these in separate terminals:

```bash
# Terminal 1: Server with auto-reload
cd server && cargo watch -x run

# Terminal 2: Client with hot reload
cd client && npm run dev
```

### Debugging

Enable debug logs:

```bash
# Server
RUST_LOG=debug cargo run

# Client (browser console)
# Open DevTools (F12) and check Console tab
```

### Testing Proof Verification

To verify proofs are working:

1. Add console logs in `DocumentClient.verifyProof()`
2. Check browser console for "Proof verified" messages
3. Try modifying the proof bytes to trigger failures

## Architecture Recap

```
┌─────────────────────────────────────────────────────────────┐
│                         Browser Tab                         │
│  ┌─────────────────────────────────────────────────────┐   │
│  │  Editor UI (TypeScript)                              │   │
│  │  - Textarea for editing                              │   │
│  │  - DocumentClient for state management               │   │
│  │  - Local content: Vec<(uuid, char)>                 │   │
│  └────────────┬────────────────────────────────────────┘   │
│               │                                              │
│               │ WebSocket                                    │
│               │ (operations + proofs)                        │
│               │                                              │
│  ┌────────────▼────────────────────────────────────────┐   │
│  │  WASM Module (Rust → WebAssembly)                   │   │
│  │  - verify_positional_proof()                        │   │
│  │  - Validates operations against root hash           │   │
│  └─────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
                              │
                              │ WebSocket
                              │
┌─────────────────────────────▼───────────────────────────────┐
│                    Server (Rust + Axum)                      │
│  ┌──────────────────────────────────────────────────────┐  │
│  │  Document State                                       │  │
│  │  - Full Merk tree with all operations                │  │
│  │  - Authoritative source of truth                     │  │
│  └──────────────────────────────────────────────────────┘  │
│  ┌──────────────────────────────────────────────────────┐  │
│  │  Proof Generation                                     │  │
│  │  - prove_position() for each operation               │  │
│  │  - Serializes and sends proofs to clients            │  │
│  └──────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

## Next Steps

- **Add tree visualization**: Show the Merkle tree structure in the UI
- **Operation history**: Display recent operations and their proofs
- **Performance metrics**: Track proof size, verification time
- **Conflict resolution**: Handle simultaneous edits better
- **Persistence**: Save documents to disk

## References

- [Merk Documentation](../../merk/README.md)
- [List Mode Documentation](../../docs/list_mode.md)
- [Positional Proofs](../../docs/list_mode_positional_proofs.md)
- [wasm-bindgen Book](https://rustwasm.github.io/docs/wasm-bindgen/)
