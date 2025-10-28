// UUID-Based Collaborative Editing Demo
// "Text Without CRDTs" Pattern
// 
// This demo implements Matt Weidner's "Text Without CRDTs" pattern using GroveDB Merk's
// InsertAfterKey operation. Unlike traditional position-based editing, each character
// has a stable UUID that never changes. Edits reference these UUIDs, enabling true
// distributed collaborative editing without conflict resolution algorithms.
//
// Key Innovation: Operations reference character UUIDs, not positions.
//   Position-based: "Insert 'x' at position 5" (breaks with concurrent edits)
//   UUID-based:     "Insert 'x' after character <uuid-of-H>" (stable!)
//
// Reference: https://mattweidner.com/2025/05/21/text-without-crdts.html

// Note: This demo runs directly against Merk (the tree layer)
// without the full GroveDB stack to avoid dependencies on incomplete
// grovedb features.

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
    /// Local view of character UUIDs (in a real system, this would be sync'ed)
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

    /// Get UUID at a logical position from this user's view
    fn get_uuid_at(&self, position: usize) -> Option<Vec<u8>> {
        self.char_map.get(&position).cloned()
    }

    /// Type a character after a specific UUID
    fn type_after_uuid(&self, ch: char, after_uuid: &[u8]) -> ListOp {
        println!(
            "  {}[{}]{} types '{}' after UUID {:?}",
            self.color,
            self.name,
            "\x1b[0m",
            ch,
            &after_uuid[..4.min(after_uuid.len())]
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
            grove_version,
            characters: Vec::new(),
        }
    }

    /// Apply a batch of UUID-based operations atomically
    fn apply_operations(&mut self, ops: Vec<ListOp>) -> Result<Vec<Vec<u8>>, String> {
        if ops.is_empty() {
            return Ok(Vec::new());
        }

        // Apply to Merk tree and get back the UUIDs
        let result = self
            .merk
            .apply_list_batch(&ops, &self.grove_version)
            .map_err(|e| format!("batch operation failed: {:?}", e))?
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

    /// Get character UUID at a logical position
    fn get_uuid_at(&self, position: usize) -> Option<Vec<u8>> {
        self.characters.get(position).map(|c| c.uuid.clone())
    }

    /// Display the document with UUID information
    fn display(&self, label: &str) {
        let content = self.get_content();
        println!("\n{}", "=".repeat(70));
        println!("{}", label);
        println!("{}", "-".repeat(70));
        println!("Document: \"{}\"", content);
        println!("Character count: {}", self.characters.len());
        
        if !self.characters.is_empty() {
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
    println!("Each character has a permanent UUID. Edits reference UUIDs, not positions.");
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
    println!("⚠️  In position-based editing, these would conflict!");
    println!("✅  With UUID-based editing, both succeed independently!\n");

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

    // === Scenario 4: Batch operations with multiple UUID references ===
    println!("\nScenario 4: Batch operations with UUID references\n");
    println!("Bob and Carol both want to add words concurrently\n");

    // Sync UUIDs to Bob and Carol
    for i in 0..5 {
        if let Some(uuid) = doc.get_uuid_at(i) {
            bob.update_char_map(i, uuid.clone());
            carol.update_char_map(i, uuid.clone());
        }
    }

    // Bob adds " World" after 'o'
    let uuid_o = doc.get_uuid_at(4).unwrap();
    println!("Bob prepares to add ' World' after 'o'");
    let mut bob_batch = vec![bob.type_after_uuid(' ', &uuid_o)];
    
    // Carol adds "!!!" after 'o' (from her perspective, before knowing about Bob's edit)
    println!("Carol prepares to add '!!!' after 'o' (concurrently)");
    let carol_batch = vec![
        carol.type_after_uuid('!', &uuid_o),
    ];

    // Apply Bob's batch
    println!("\nApplying Bob's batch:");
    let bob_uuids = doc.apply_operations(bob_batch).expect("failed");
    let uuid_space = bob_uuids[0].clone();
    
    // Continue Bob's word - now reference the space
    bob_batch = vec![
        bob.type_after_uuid('W', &uuid_space),
    ];
    let bob_uuids = doc.apply_operations(bob_batch).expect("failed");
    let uuid_w = bob_uuids[0].clone();

    bob_batch = vec![
        bob.type_after_uuid('o', &uuid_w),
    ];
    let bob_uuids = doc.apply_operations(bob_batch).expect("failed");
    let uuid_w_o = bob_uuids[0].clone();

    bob_batch = vec![
        bob.type_after_uuid('r', &uuid_w_o),
    ];
    let bob_uuids = doc.apply_operations(bob_batch).expect("failed");
    let uuid_r = bob_uuids[0].clone();

    bob_batch = vec![
        bob.type_after_uuid('l', &uuid_r),
    ];
    let bob_uuids = doc.apply_operations(bob_batch).expect("failed");
    let uuid_l = bob_uuids[0].clone();

    bob_batch = vec![
        bob.type_after_uuid('d', &uuid_l),
    ];
    doc.apply_operations(bob_batch).expect("failed");

    doc.display("After Bob adds ' World'");

    // Apply Carol's batch (which still references the original 'o')
    println!("\nApplying Carol's batch (still referencing original 'o' UUID):");
    doc.apply_operations(carol_batch).expect("failed");
    doc.display("After Carol adds '!'");

    println!("\nResult: Both edits coexist!");
    println!("   Carol's '!' was inserted after 'o', before Bob's ' World'");
    println!("   because it references the UUID of 'o', not a position.\n");

}
