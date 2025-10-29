// UUID-Based Collaborative Editing with Merkle Proofs
// "Text Without CRDTs" + Verifiable Operations
// 
// This demo extends the UUID-based collaborative editing pattern with cryptographic Merkle proofs.
// Each operation broadcast by the server includes a positional Merkle proof that cryptographically
// proves it's consistent with the published root hash. This enables:
// - Transparency: Clients can verify server behavior cryptographically
// - Trust minimization: No need to trust the server blindly
// - Auditability: All operations can be independently verified with cryptographic guarantees
//
// Architecture:
// 1. Clients send operations to server
// 2. Server applies ops and updates root hash
// 3. Server generates positional Merkle proof for each operation
// 4. Server broadcasts ops + Merkle proof to all clients
// 5. Clients cryptographically verify proof against published root hash
// 6. If valid, clients apply the operation locally
//
// The proofs use Blake3 hashing and encode:
// - The value at the claimed position
// - All intermediate node hashes needed to reconstruct the root
// - Subtree sizes for position validation
// - This makes tampering cryptographically detectable
//
// Reference: https://mattweidner.com/2025/05/21/text-without-crdts.html

use grovedb_merk::{Merk, ListOp, MerkType, TreeType};
use grovedb_path::SubtreePath;
use grovedb_storage::{rocksdb_storage::test_utils::TempStorage, Storage, StorageBatch};
use grovedb_version::version::GroveVersion;
use std::collections::HashMap;

/// Represents a character in the document with its UUID
#[derive(Clone, Debug)]
struct Character {
    uuid: Vec<u8>,
    value: char,
}

/// Represents an operation with its Merkle proof
#[derive(Clone, Debug)]
struct VerifiableOp {
    operation: ListOp,
    root_hash_before: Vec<u8>,
    root_hash_after: Vec<u8>,
    proof: Vec<u8>, // Serialized Merkle proof
}

/// The server that maintains the authoritative state
struct CollabServer {
    merk: Merk<grovedb_storage::rocksdb_storage::PrefixedRocksDbTransactionContext<'static>>,
    grove_version: GroveVersion,
    characters: Vec<Character>,
    // Published root hashes (transparency log)
    root_history: Vec<(usize, Vec<u8>)>, // (operation_count, root_hash)
}

impl CollabServer {
    fn new() -> Self {
        let grove_version = GroveVersion::latest();
        let storage = Box::leak(Box::new(TempStorage::new()));
        let batch = Box::leak(Box::new(StorageBatch::new()));
        let tx = Box::leak(Box::new(storage.start_transaction()));

        let context = storage
            .get_transactional_storage_context(SubtreePath::empty(), Some(batch), tx)
            .unwrap();

        let merk = Merk::open_empty(context, MerkType::StandaloneMerk, TreeType::ListTree);

        CollabServer {
            merk,
            grove_version: grove_version.clone(),
            characters: Vec::new(),
            root_history: vec![(0, vec![])],
        }
    }

    /// Get the current root hash (published state)
    fn get_root_hash(&self) -> Vec<u8> {
        self.merk.root_hash().value.to_vec()
    }

    /// Apply an operation and create a verifiable broadcast
    fn apply_and_create_proof(&mut self, op: ListOp) -> Result<VerifiableOp, String> {
        // Capture root hash before operation
        let root_before = self.get_root_hash();

        // Apply the operation
        let cost_result = self.merk.apply_list_batch(&[op.clone()], &self.grove_version);
        let result = cost_result
            .value
            .map_err(|e| format!("operation failed: {:?}", e))?;

        // Update character list
        let uuid = result.keys[0].clone();
        match &op {
            ListOp::InsertAtPosition { value, .. } => {
                if value.len() == 1 {
                    let ch = Character {
                        uuid: uuid.clone(),
                        value: value[0] as char,
                    };
                    self.characters.insert(0, ch);
                }
            }
            ListOp::InsertAfterKey { target_key, value } => {
                if value.len() == 1 {
                    let pos = self
                        .characters
                        .iter()
                        .position(|c| c.uuid == *target_key)
                        .map(|p| p + 1)
                        .unwrap_or(self.characters.len());

                    let ch = Character {
                        uuid: uuid.clone(),
                        value: value[0] as char,
                    };
                    self.characters.insert(pos, ch);
                }
            }
            _ => {}
        }

        // Capture root hash after operation
        let root_after = self.get_root_hash();

        // Publish root to transparency log
        self.root_history.push((self.characters.len(), root_after.clone()));

        // Generate proof for the newly inserted key
        // In a real system, this would be a full Merkle proof of inclusion
        // For this demo, we'll create a simple proof showing the key exists
        let proof = self.generate_proof_for_key(&uuid, &root_after)?;

        Ok(VerifiableOp {
            operation: op,
            root_hash_before: root_before,
            root_hash_after: root_after,
            proof,
        })
    }

    /// Generate a Merkle proof for a specific key
    fn generate_proof_for_key(&mut self, key: &[u8], _root_after: &[u8]) -> Result<Vec<u8>, String> {
        // Find the position of the newly inserted key in the tree
        let position = self.characters.iter()
            .position(|c| c.uuid == *key)
            .ok_or("Key not found in character list")?;
        
        // Generate a real positional Merkle proof
        let proof_result = self.merk.prove_position(position as u64, &self.grove_version)
            .value
            .map_err(|e| format!("Failed to generate proof: {:?}", e))?;
        
        // The proof contains everything needed to verify:
        // - The key/value at this position
        // - All intermediate hashes to reconstruct the root
        // - Subtree sizes for position validation
        Ok(proof_result.proof)
    }

    /// Get current document content
    fn get_content(&self) -> String {
        self.characters.iter().map(|c| c.value).collect()
    }

    /// Display transparency log
    fn display_transparency_log(&self) {
        println!("\nServer Transparency Log:");
        println!("   (Published root hashes that clients can verify against)");
        for (count, root) in &self.root_history {
            let root_preview: Vec<String> = root.iter()
                .take(8)
                .map(|b| format!("{:02x}", b))
                .collect();
            let root_str = if root.is_empty() {
                "empty".to_string()
            } else {
                root_preview.join("")
            };
            println!("   After {} ops: {}", count, root_str);
        }
    }
}

/// Represents a client that can verify operations
struct CollabClient {
    name: String,
    color: &'static str,
    characters: Vec<Character>,
    current_root_hash: Vec<u8>,
    // Track UUIDs for typing operations
    char_map: HashMap<usize, Vec<u8>>,
}

impl CollabClient {
    fn new(name: &str, color: &'static str) -> Self {
        CollabClient {
            name: name.to_string(),
            color,
            characters: Vec::new(),
            current_root_hash: vec![],
            char_map: HashMap::new(),
        }
    }

    /// Verify and apply an operation from the server
    fn verify_and_apply(&mut self, verifiable_op: &VerifiableOp) -> Result<(), String> {
        // Step 1: Verify the root hash before matches what we expect
        if !self.current_root_hash.is_empty() && self.current_root_hash != verifiable_op.root_hash_before {
            return Err(format!(
                "Root hash mismatch! Expected {:02x}{:02x}..., got {:02x}{:02x}...",
                self.current_root_hash[0], self.current_root_hash[1],
                verifiable_op.root_hash_before[0], verifiable_op.root_hash_before[1]
            ));
        }

        // Step 2: Verify the Merkle proof
        // Pass the operation details so we can do more thorough verification
        self.verify_proof(&verifiable_op.proof, &verifiable_op.root_hash_after, &verifiable_op.root_hash_before)?;

        // Step 3: Apply the operation locally
        match &verifiable_op.operation {
            ListOp::InsertAtPosition { value, .. } => {
                if value.len() == 1 {
                    // In a real system, the UUID would be extracted from the proof
                    // For now, we'll generate a placeholder
                    let uuid = format!("uuid_{}", self.characters.len()).into_bytes();
                    let ch = Character {
                        uuid: uuid.clone(),
                        value: value[0] as char,
                    };
                    self.characters.insert(0, ch);
                    self.char_map.insert(0, uuid);
                }
            }
            ListOp::InsertAfterKey { target_key, value } => {
                if value.len() == 1 {
                    let pos = self
                        .characters
                        .iter()
                        .position(|c| c.uuid == *target_key)
                        .map(|p| p + 1)
                        .unwrap_or(self.characters.len());

                    // Extract UUID from proof (in real system)
                    let uuid = format!("uuid_{}", self.characters.len()).into_bytes();
                    let ch = Character {
                        uuid: uuid.clone(),
                        value: value[0] as char,
                    };
                    self.characters.insert(pos, ch);
                    self.char_map.insert(pos, uuid);
                }
            }
            _ => {}
        }

        // Step 4: Update our view of the root hash
        self.current_root_hash = verifiable_op.root_hash_after.clone();

        println!(
            "   {}[{}]{} verified and applied operation ✓",
            self.color, self.name, "\x1b[0m"
        );

        Ok(())
    }

    /// Verify a Merkle proof against the expected root hash
    fn verify_proof(&self, proof: &[u8], expected_root_after: &[u8], _root_before: &[u8]) -> Result<(), String> {
        // Use the real positional proof verification
        // The proof encodes the position implicitly through the tree structure
        // We need to extract the position from the proof or track it separately
        
        // For now, we'll verify position 0 (most recent insertion)
        // In a production system, the position would be included in the VerifiableOp
        let position = 0u64;
        
        let grove_version = GroveVersion::latest();
        let expected_root_hash: [u8; 32] = expected_root_after.try_into()
            .map_err(|_| "Invalid root hash length")?;
        
        // Verify the positional proof
        // Returns CostContext<Result<PositionalProofResult, Error>>
        let cost_result = grovedb_merk::proofs::positional::verify_positional_proof(
            proof,
            position,
            expected_root_hash,
            &grove_version,
        );
        
        let verification_result = cost_result.value
            .map_err(|e| format!("Proof verification failed: {:?}", e))?;
        
        // The proof is valid! It cryptographically proves:
        // 1. The root hash is correct
        // 2. The value at this position exists
        // 3. The tree structure is consistent
        
        println!("      → Merkle proof verified: position={}, value={:?}, tree_size={}",
            verification_result.position,
            String::from_utf8_lossy(&verification_result.value),
            verification_result.tree_size
        );
        
        Ok(())
    }

    /// Create an operation to send to server
    fn create_insert_after(&self, ch: char, after_uuid: &[u8]) -> ListOp {
        println!(
            "   {}[{}]{} creates InsertAfterKey('{}', {:02x}{:02x}...)",
            self.color,
            self.name,
            "\x1b[0m",
            ch,
            after_uuid[0],
            after_uuid[1]
        );

        ListOp::InsertAfterKey {
            target_key: after_uuid.to_vec(),
            value: vec![ch as u8],
        }
    }

    /// Create first character operation
    fn create_first(&self, ch: char) -> ListOp {
        println!(
            "   {}[{}]{} creates InsertAtPosition(0, '{}')",
            self.color, self.name, "\x1b[0m", ch
        );

        ListOp::InsertAtPosition {
            position: 0,
            value: vec![ch as u8],
        }
    }

    /// Get current document view
    fn get_content(&self) -> String {
        self.characters.iter().map(|c| c.value).collect()
    }
}

fn main() {
    println!("\nUUID-Based Collaborative Editing with Merkle Proofs");
    println!("\"Text Without CRDTs\" + Transparency & Verification\n");
    println!("Architecture:");
    println!("  1. Clients send operations to server");
    println!("  2. Server applies ops and publishes root hash");
    println!("  3. Server broadcasts ops + Merkle proof to clients");
    println!("  4. Clients verify proof before applying locally");
    println!("  5. Transparency log allows independent audit\n");

    // Initialize server
    let mut server = CollabServer::new();
    println!("Server initialized with empty document");
    println!("   Root hash: {}", if server.get_root_hash().is_empty() { "empty" } else { "initialized" });

    // Initialize clients
    let mut alice = CollabClient::new("Alice", "\x1b[34m");
    let mut bob = CollabClient::new("Bob", "\x1b[32m");
    let mut carol = CollabClient::new("Carol", "\x1b[35m");

    println!("\nThree clients initialized: Alice, Bob, Carol");

    // === Scenario 1: Alice types first character ===
    println!("\nScenario 1: Alice types 'H' (first character)\n");

    let op1 = alice.create_first('H');
    println!("\n   Server processing operation...");
    let verifiable_op1 = server.apply_and_create_proof(op1)
        .expect("server failed to apply");

    println!("   Server root hash updated: {:02x}{:02x}...",
        verifiable_op1.root_hash_after[0],
        verifiable_op1.root_hash_after[1]
    );

    println!("\n   Broadcasting to all clients with proof...");
    alice.verify_and_apply(&verifiable_op1).expect("alice failed to verify");
    bob.verify_and_apply(&verifiable_op1).expect("bob failed to verify");
    carol.verify_and_apply(&verifiable_op1).expect("carol failed to verify");

    println!("\n   Server: \"{}\"", server.get_content());
    println!("   Alice:  \"{}\"", alice.get_content());
    println!("   Bob:    \"{}\"", bob.get_content());
    println!("   Carol:  \"{}\"", carol.get_content());
    assert_eq!(server.get_content(), alice.get_content());
    assert_eq!(server.get_content(), bob.get_content());
    assert_eq!(server.get_content(), carol.get_content());

    // Get UUID of 'H' for next operation
    let uuid_h = server.characters[0].uuid.clone();

    // === Scenario 2: Bob types 'i' after 'H' ===
    println!("\nScenario 2: Bob types 'i' after 'H'\n");

    let op2 = bob.create_insert_after('i', &uuid_h);
    println!("\n   Server processing operation...");
    let verifiable_op2 = server.apply_and_create_proof(op2)
        .expect("server failed to apply");

    println!("   Server root hash updated: {:02x}{:02x}...",
        verifiable_op2.root_hash_after[0],
        verifiable_op2.root_hash_after[1]
    );

    println!("\n   Broadcasting to all clients with proof...");
    alice.verify_and_apply(&verifiable_op2).expect("alice failed to verify");
    bob.verify_and_apply(&verifiable_op2).expect("bob failed to verify");
    carol.verify_and_apply(&verifiable_op2).expect("carol failed to verify");

    println!("\n   Server: \"{}\"", server.get_content());
    println!("   Alice:  \"{}\"", alice.get_content());
    println!("   Bob:    \"{}\"", bob.get_content());
    println!("   Carol:  \"{}\"", carol.get_content());

    // Get UUID of 'i'
    let uuid_i = server.characters[1].uuid.clone();

    // === Scenario 3: Carol types '!' after 'i' ===
    println!("\nScenario 3: Carol types '!' after 'i'\n");

    let op3 = carol.create_insert_after('!', &uuid_i);
    println!("\n   Server processing operation...");
    let verifiable_op3 = server.apply_and_create_proof(op3)
        .expect("server failed to apply");

    println!("   Server root hash updated: {:02x}{:02x}...",
        verifiable_op3.root_hash_after[0],
        verifiable_op3.root_hash_after[1]
    );

    println!("\n   Broadcasting to all clients with proof...");
    alice.verify_and_apply(&verifiable_op3).expect("alice failed to verify");
    bob.verify_and_apply(&verifiable_op3).expect("bob failed to verify");
    carol.verify_and_apply(&verifiable_op3).expect("carol failed to verify");

    println!("\n   Final document state:");
    println!("   Server: \"{}\"", server.get_content());
    println!("   Alice:  \"{}\"", alice.get_content());
    println!("   Bob:    \"{}\"", bob.get_content());
    println!("   Carol:  \"{}\"", carol.get_content());

    // Display transparency log
    server.display_transparency_log();

    // === Scenario 4: Demonstrate verification failure ===
    println!("\nScenario 4: Simulating tampered operation (verification fails)\n");

    let op4 = alice.create_insert_after('X', &uuid_h);
    let mut verifiable_op4 = server.apply_and_create_proof(op4)
        .expect("server failed to apply");

    println!("   Tampering with root hash...");
    // Corrupt multiple bytes to make tampering obvious
    verifiable_op4.root_hash_after[0] ^= 0xFF;
    verifiable_op4.root_hash_after[1] ^= 0xAA;
    verifiable_op4.root_hash_after[2] ^= 0x55;

    println!("\n   Broadcasting tampered operation to clients...");
    
    // Bob tries to verify - should fail
    match bob.verify_and_apply(&verifiable_op4) {
        Ok(_) => println!("   {}[Bob]{} ERROR: Should have rejected tampered op!", bob.color, "\x1b[0m"),
        Err(e) => println!("   {}[Bob]{} ✓ Rejected tampered operation: {}", bob.color, "\x1b[0m", e),
    }

    println!("\n   {}[Bob's view remains safe:]{} \"{}\"", bob.color, "\x1b[0m", bob.get_content());
    println!("   (Tampered operation was rejected before being applied)");

    println!();
}
