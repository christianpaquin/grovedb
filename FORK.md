# GroveDB `list_mode` Fork

**Branch**: `merk-list-mode`  
**Forked from**: `develop` at commit `97b87799` (feat: Release crates automatically #378)  
**Status**: Experimental - Not backward compatible with pre-fork databases

## Summary

This fork adds `list_mode` functionality to Merk, enabling positional (index-based) semantics over Merkle trees with cryptographic proofs. The implementation uses subtree sizes and persisted parent pointers for O(log n) position lookups, random UUID keys, and includes performance optimizations (node index for O(1) lookups). A full collaborative editing demo implements Matt Weidner's "Text Without CRDTs" design with zero-latency typing, reference-based operations, and independent proof verification.

**17 commits** implementing list mode core, positional proofs, performance optimizations, and collaborative demo. All 272+ Merk tests passing.

## Documentation

- **Design & Implementation**: `docs/list_mode.md`, `docs/list_mode_positional_proofs.md`, `docs/list_mode_persistence_decision.md`
- **Collaborative Demo**: `merk-collab-demo/README.md`, `merk-collab-demo/IMPLEMENTATION_STATUS.md`, `merk-collab-demo/REFERENCE_BASED_OPS.md`
- **Reference**: [Matt Weidner's "Text Without CRDTs"](https://mattweidner.com/2025/05/21/text-without-crdts.html)
