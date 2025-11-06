# Quick Reference - Merk Collaborative Editor Demo

## 🚀 One-Command Start (after initial build)

```bash
# Terminal 1 - Server
cd merk-collab-demo/server && cargo run --release

# Terminal 2 - Client  
cd merk-collab-demo/client && npm run dev

# Then open: http://localhost:5173
```

## 📦 Initial Setup (First Time Only)

```bash
# 1. Build WASM module
cd merk-collab-demo/merk-wasm
wasm-pack build --target web --out-dir ../client/public/wasm

# 2. Install client dependencies
cd ../client
npm install
```

## 🏗️ Project Structure

```
merk-collab-demo/
├── merk-wasm/      # Proof verification (Rust → WASM)
├── server/         # Backend (Rust + Axum + Merk)
└── client/         # Frontend (TypeScript + Vite)
```

## 🔍 Key Files

| File | Purpose |
|------|---------|
| `server/src/document.rs` | Merk tree operations + proof generation |
| `server/src/main.rs` | WebSocket server + routing |
| `client/src/DocumentClient.ts` | State management + proof verification |
| `client/src/Editor.ts` | UI and user input handling |
| `merk-wasm/src/lib.rs` | WASM bindings for verification |

## 🧪 Testing Commands

```bash
# Server tests
cd server && cargo test

# WASM tests  
cd merk-wasm && wasm-pack test --node

# Client build check
cd client && npm run build
```

## 🐛 Debug Mode

```bash
# Server with debug logs
cd server && RUST_LOG=debug cargo run

# Client (check browser console)
# Open DevTools with F12
```

## 📊 What to Watch

When running the demo, observe:

1. **Status indicator** - Green pulse when connected
2. **Root hash** - Changes with each operation
3. **Proof count** - Increments as proofs verify
4. **Operation count** - Total insert/delete operations
5. **Multi-tab sync** - Changes appear in all tabs instantly

## 🔧 Common Issues

| Problem | Solution |
|---------|----------|
| Port 3000 in use | Change port in `server/src/main.rs` |
| Port 5173 in use | Change port in `client/vite.config.ts` |
| WASM not loading | Run `wasm-pack build` in `merk-wasm/` |
| Connection refused | Start server before client |
| Text not syncing | Check WebSocket in browser DevTools |

## 📝 Architecture Flow

```
User Types → Editor captures event → DocumentClient sends operation
                                              ↓
Server receives operation → Applies to Merk tree → Generates proof
                                              ↓
Server broadcasts → All clients receive → WASM verifies proof
                                              ↓
Proof valid → Update local state → Update UI
```

## 🎯 Demo Highlights

- **Cryptographic security**: Every operation proven with Merkle proofs
- **Minimal client**: Only stores `Vec<(uuid, char)>`, not tree
- **Small WASM**: ~200-500KB for verification only
- **Real-time**: Instant propagation via WebSocket
- **"Text Without CRDTs"**: Server authoritative + client verification

## 📚 Documentation

- `README.md` - Architecture overview
- `BUILD.md` - Detailed build instructions
- `IMPLEMENTATION.md` - Complete implementation summary
- `../../docs/list_mode_positional_proofs.md` - Proof algorithm details

## 🚦 Development Workflow

```bash
# Watch mode for active development

# Terminal 1 - Auto-reload server
cd server && cargo watch -x run

# Terminal 2 - Hot-reload client
cd client && npm run dev

# Make changes, see results instantly!
```

## 🎨 Customization Ideas

- Change colors in `client/index.html` (gradient backgrounds)
- Add tree visualization with D3.js or Cytoscape
- Show proof structure and size per operation
- Add user avatars and cursor positions
- Implement rich text formatting
- Add document persistence

## ⚡ Performance Tips

- Use `--release` for production builds
- Enable WASM optimizations in `merk-wasm/Cargo.toml`
- Batch operations when pasting text
- Add caching for frequent proof types

## 📞 Getting Help

If stuck:
1. Check `BUILD.md` for detailed instructions
2. Read `IMPLEMENTATION.md` for architecture details
3. Look at Rust examples in `merk/examples/`
4. Review `docs/list_mode_positional_proofs.md`

---

**Ready to start?** Run the setup commands above and open multiple browser tabs to see collaborative editing in action! 🎉
