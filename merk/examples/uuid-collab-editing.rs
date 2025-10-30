// UUID-Based Collaborative Editing Demo
// "Text Without CRDTs" Pattern
// 
// This demo implements Matt Weidner's "Text Without CRDTs" pattern using GroveDB Merk's
// InsertAfterKey operation. Unlike traditional position-based editing, each character
// has a stable UUID that never changes. Edits reference these UUIDs, enabling true
// distributed collaborative editing without conflict resolution algorithms.
//
// Key Innovation: Operations reference character UUIDs, not positions!
//   Position-based: "Insert 'x' at position 5" (breaks with concurrent edits)
//   UUID-based:     "Insert 'x' after character <uuid-of-H>" (stable!)
//
// Reference: https://mattweidner.com/2025/05/21/text-without-crdts.html
//
// See `uuid-collab-edit-with-proofs` for a version with proofs

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

/// Represents a simulated remote user in the collaborative editing session
struct RemoteUser {
    name: String,
    color: &'static str,
    /// Local view of character UUIDs (in a real system, this would be synced)
    char_map: HashMap<usize, Vec<u8>>, // position -> UUID
}

impl RemoteUser {
    fn new(name: &str, color: &'static str) -> Self {
        RemoteUser {
            name: name.to_string(),
            color,
            char_map: HashMap::new(),
        }
    }

    /// Update this user's view of character UUIDs
    fn update_char_map(&mut self, position: usize, uuid: Vec<u8>) {
        self.char_map.insert(position, uuid);
    }

    /// Type a character after a specific UUID
    fn type_after_uuid(&self, ch: char, after_uuid: &[u8]) -> ListOp {
        println!(
            "  {}[{}]{} types '{}' after UUID {:02x}{:02x}{:02x}{:02x}...",
            self.color,
            self.name,
            "\x1b[0m",
            ch,
            after_uuid[0],
            after_uuid[1],
            after_uuid[2],
            after_uuid[3]
        );

        ListOp::InsertAfterKey {
            target_key: after_uuid.to_vec(),
            value: vec![ch as u8],
        }
    }

    /// Type the first character (no predecessor)
    fn type_first(&self, ch: char) -> ListOp {
        println!(
            "  {}[{}]{} types first character '{}'",
            self.color, self.name, "\x1b[0m", ch
        );

        ListOp::InsertAtPosition {
            position: 0,
            value: vec![ch as u8],
        }
    }
}

/// Shared document state (the "server" in a real distributed system)
struct UuidDocument {
    merk: Merk<grovedb_storage::rocksdb_storage::PrefixedRocksDbTransactionContext<'static>>,
    grove_version: GroveVersion,
    characters: Vec<Character>, // Ordered list of characters with UUIDs
}

impl UuidDocument {
    fn new() -> Self {
        let grove_version = GroveVersion::latest();
        let storage = Box::leak(Box::new(TempStorage::new()));
        let batch = Box::leak(Box::new(StorageBatch::new()));
        let tx = Box::leak(Box::new(storage.start_transaction()));

        let context = storage
            .get_transactional_storage_context(SubtreePath::empty(), Some(batch), tx)
            .unwrap();

        let merk = Merk::open_empty(context, MerkType::StandaloneMerk, TreeType::ListTree);

        UuidDocument {
            merk,
            grove_version: grove_version.clone(),
            characters: Vec::new(),
        }
    }

    /// Apply a batch of UUID-based operations atomically
    fn apply_operations(&mut self, ops: Vec<ListOp>) -> Result<Vec<Vec<u8>>, String> {
        if ops.is_empty() {
            return Ok(Vec::new());
        }

        // Apply to Merk tree and get back the UUIDs
        let cost_result = self
            .merk
            .apply_list_batch(&ops, &self.grove_version);
        
        let result = cost_result
            .value
            .map_err(|e| format!("batch operation failed: {:?}", e))?;

        // Update our character list
        for (i, op) in ops.iter().enumerate() {
            match op {
                ListOp::InsertAtPosition { value, .. } => {
                    if value.len() == 1 {
                        let uuid = result.keys[i].clone();
                        let ch = Character {
                            uuid: uuid.clone(),
                            value: value[0] as char,
                        };
                        self.characters.insert(i, ch);
                    }
                }
                ListOp::InsertAfterKey { target_key, value } => {
                    if value.len() == 1 {
                        let uuid = result.keys[i].clone();
                        // Find position of target and insert after it
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
        }

        Ok(result.keys)
    }

    /// Get the current document content as a string
    fn get_content(&self) -> String {
        self.characters.iter().map(|c| c.value).collect()
    }

    /// Display the document with UUID information
    fn display(&self, label: &str) {
        let content = self.get_content();
        println!("\n{}", "=".repeat(70));
        println!("{}", label);
        println!("{}", "-".repeat(70));
        println!("Document: \"{}\"", content);
        println!("Character count: {}", self.characters.len());
        
        if !self.characters.is_empty() && self.characters.len() <= 20 {
            println!("\nCharacter UUIDs (first 8 bytes shown):");
            for (i, ch) in self.characters.iter().enumerate() {
                let uuid_preview: Vec<String> = ch.uuid.iter()
                    .take(8)
                    .map(|b| format!("{:02x}", b))
                    .collect();
                println!("  [{}] '{}' -> {}", i, ch.value, uuid_preview.join(""));
            }
        }
        println!("{}", "=".repeat(70));
    }
}

fn main() {
    println!("\nUUID-Based Collaborative Editing Demo");
    println!("\"Text Without CRDTs\" Pattern\n");
    println!("Each character has a permanent UUID. Edits reference UUIDs, not positions!");
    println!("This enables true distributed collaboration without conflict resolution.\n");

    let mut doc = UuidDocument::new();

    // Create simulated remote users
    let mut alice = RemoteUser::new("Alice", "\x1b[34m"); // Blue
    let mut bob = RemoteUser::new("Bob", "\x1b[32m");     // Green
    let mut carol = RemoteUser::new("Carol", "\x1b[35m"); // Magenta

    // === Scenario 1: Alice creates the initial document ===
    println!("\nScenario 1: Alice creates initial document\n");
    println!("Alice types: \"Hi\"");

    // Alice types 'H' first
    let mut ops = vec![alice.type_first('H')];
    let uuids = doc.apply_operations(ops).expect("failed to apply");
    let uuid_h = uuids[0].clone();
    alice.update_char_map(0, uuid_h.clone());

    // Alice types 'i' after 'H' using UUID reference
    ops = vec![alice.type_after_uuid('i', &uuid_h)];
    let uuids = doc.apply_operations(ops).expect("failed to apply");
    let uuid_i = uuids[0].clone();
    alice.update_char_map(1, uuid_i.clone());

    doc.display("After Alice types 'Hi'");

    // Broadcast UUIDs to other users (simulating network sync)
    bob.update_char_map(0, uuid_h.clone());
    bob.update_char_map(1, uuid_i.clone());
    carol.update_char_map(0, uuid_h.clone());
    carol.update_char_map(1, uuid_i.clone());

    // === Scenario 2: Concurrent edits using UUIDs ===
    println!("\nScenario 2: Concurrent edits by Bob and Carol\n");
    println!("Bob wants to insert '!' after 'i'");
    println!("Carol wants to insert ',' after 'H' (between 'H' and 'i')\n");

    // Bob's operation: Insert '!' after 'i'
    let bob_ops = vec![bob.type_after_uuid('!', &uuid_i)];

    // Carol's operation: Insert ',' after 'H' (concurrently, from her view)
    let carol_ops = vec![carol.type_after_uuid(',', &uuid_h)];

    // Apply Bob's edit first
    println!("Applying Bob's operation:");
    let bob_uuids = doc.apply_operations(bob_ops).expect("failed to apply");
    let uuid_exclaim = bob_uuids[0].clone();
    doc.display("After Bob's edit");

    // Apply Carol's edit (which references the same UUID structure)
    println!("\nApplying Carol's operation:");
    let carol_uuids = doc.apply_operations(carol_ops).expect("failed to apply");
    let uuid_comma = carol_uuids[0].clone();
    doc.display("After Carol's edit");

    println!("\nResult: \"H,i!\" - Both edits applied successfully!");
    println!("   The UUID references ensured correct insertion points.\n");

    // Update all users' views
    alice.update_char_map(1, uuid_comma.clone());
    alice.update_char_map(2, uuid_i.clone());
    alice.update_char_map(3, uuid_exclaim.clone());
    bob.update_char_map(1, uuid_comma.clone());
    bob.update_char_map(2, uuid_i.clone());
    carol.update_char_map(2, uuid_i.clone());
    carol.update_char_map(3, uuid_exclaim.clone());

    // === Scenario 3: Building a word using UUID chaining ===
    println!("\nScenario 3: Alice builds a word using UUID references\n");
    println!("Alice will type \"Hello\" by referencing the previous character's UUID\n");

    // Start fresh for clarity
    doc = UuidDocument::new();
    alice.char_map.clear();

    println!("Alice types: \"Hello\"");

    // Type 'H'
    ops = vec![alice.type_first('H')];
    let mut uuids = doc.apply_operations(ops).expect("failed");
    let mut prev_uuid = uuids[0].clone();
    alice.update_char_map(0, prev_uuid.clone());

    // Type 'e' after 'H'
    ops = vec![alice.type_after_uuid('e', &prev_uuid)];
    uuids = doc.apply_operations(ops).expect("failed");
    prev_uuid = uuids[0].clone();
    alice.update_char_map(1, prev_uuid.clone());

    // Type 'l' after 'e'
    ops = vec![alice.type_after_uuid('l', &prev_uuid)];
    uuids = doc.apply_operations(ops).expect("failed");
    prev_uuid = uuids[0].clone();
    alice.update_char_map(2, prev_uuid.clone());

    // Type 'l' after first 'l'
    ops = vec![alice.type_after_uuid('l', &prev_uuid)];
    uuids = doc.apply_operations(ops).expect("failed");
    prev_uuid = uuids[0].clone();
    alice.update_char_map(3, prev_uuid.clone());

    // Type 'o' after second 'l'
    ops = vec![alice.type_after_uuid('o', &prev_uuid)];
    uuids = doc.apply_operations(ops).expect("failed");
    alice.update_char_map(4, uuids[0].clone());

    doc.display("After Alice types 'Hello'");

    // === Scenario 4: Demonstrating true concurrent edits ===
    println!("\nScenario 4: True concurrent edits\n");
    println!("Bob and Carol both want to edit 'Hello' concurrently\n");

    // Get UUID of 'o' (last character)
    let uuid_o = doc.characters.last().unwrap().uuid.clone();
    let uuid_first_l = doc.characters[2].uuid.clone();

    // Sync to other users
    bob.update_char_map(0, doc.characters[0].uuid.clone());
    bob.update_char_map(1, doc.characters[1].uuid.clone());
    bob.update_char_map(2, uuid_first_l.clone());
    bob.update_char_map(3, doc.characters[3].uuid.clone());
    bob.update_char_map(4, uuid_o.clone());

    carol.update_char_map(0, doc.characters[0].uuid.clone());
    carol.update_char_map(1, doc.characters[1].uuid.clone());
    carol.update_char_map(2, uuid_first_l.clone());
    carol.update_char_map(3, doc.characters[3].uuid.clone());
    carol.update_char_map(4, uuid_o.clone());

    // Bob adds "!!!" after 'o'
    println!("Bob prepares to add '!!!' after 'o'");
    let bob_ops = vec![bob.type_after_uuid('!', &uuid_o)];
    
    // Carol inserts '*' after first 'l' (concurrent!)
    println!("Carol prepares to insert '*' after first 'l' (concurrently)");
    let carol_ops = vec![carol.type_after_uuid('*', &uuid_first_l)];

    // Apply Bob's operation
    println!("\nApplying Bob's operation:");
    doc.apply_operations(bob_ops).expect("failed");
    doc.display("After Bob adds '!'");

    // Apply Carol's operation (referencing old UUID state)
    println!("\nApplying Carol's operation:");
    doc.apply_operations(carol_ops).expect("failed");
    doc.display("After Carol adds '*'");

    println!("\nResult: \"Hel*lo!\" - Both edits coexist!");
    println!("   Carol's '*' was inserted after first 'l', unaffected by Bob's '!'");
    println!("   because each operation references specific character UUIDs.\n");

    println!();
}
