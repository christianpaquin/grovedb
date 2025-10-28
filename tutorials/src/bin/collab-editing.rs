// Collaborative Editing Demo
// 
// This demo simulates multiple users collaboratively editing a document using
// GroveDB Merk's list-mode with batch operations. It demonstrates:
// - Multiple simulated users typing concurrently
// - Efficient batch operations for character insertion
// - UUID-based character identity (stable across edits)
// - Document state visualization after each user action

use grovedb_merk::{Merk, ListOp};
use grovedb_storage::rocksdb_storage::test_utils::TempStorage;
use grovedb_version::version::GroveVersion;

/// Represents a simulated user in the collaborative editing session
struct User {
    name: String,
    color: &'static str, // ANSI color code for terminal output
}

impl User {
    fn new(name: &str, color: &'static str) -> Self {
        User {
            name: name.to_string(),
            color,
        }
    }

    /// Simulate typing a string of text at a specific position
    fn type_text(&self, text: &str, position: u64) -> Vec<ListOp> {
        println!(
            "{}[{}]{} types '{}' at position {}",
            self.color, self.name, "\x1b[0m", text, position
        );

        text.chars()
            .enumerate()
            .map(|(i, ch)| ListOp::InsertAtPosition {
                position: position + i as u64,
                value: vec![ch as u8],
            })
            .collect()
    }

    /// Simulate deleting a range of characters
    fn delete_range(&self, start_pos: u64, count: usize) -> Vec<ListOp> {
        println!(
            "{}[{}]{} deletes {} character(s) at position {}",
            self.color, self.name, "\x1b[0m", count, start_pos
        );

        // Delete in reverse order so positions don't shift
        (0..count)
            .map(|_| ListOp::DeleteAtPosition {
                position: start_pos,
            })
            .collect()
    }
}

/// Shared document state
struct Document {
    merk: Merk<TempStorage>,
    grove_version: GroveVersion,
    // Track content for display (in real app, this would be queried from tree)
    content: Vec<char>,
}

impl Document {
    fn new() -> Self {
        let grove_version = GroveVersion::latest();
        let storage = TempStorage::new();
        let merk = Merk::open_list_mode(storage, None, &grove_version)
            .unwrap()
            .expect("failed to create list-mode merk");

        Document {
            merk,
            grove_version,
            content: Vec::new(),
        }
    }

    /// Apply a batch of operations atomically
    fn apply_operations(&mut self, ops: Vec<ListOp>) -> Result<(), String> {
        if ops.is_empty() {
            return Ok(());
        }

        // Apply to Merk tree
        self.merk
            .apply_list_batch(&ops, &self.grove_version)
            .map_err(|e| format!("batch operation failed: {:?}", e))?;

        // Update our content tracking
        for op in ops {
            match op {
                ListOp::InsertAtPosition { position, value } => {
                    if value.len() == 1 {
                        self.content.insert(position as usize, value[0] as char);
                    }
                }
                ListOp::InsertAtPositionWithKey { position, value, .. } => {
                    if value.len() == 1 {
                        self.content.insert(position as usize, value[0] as char);
                    }
                }
                ListOp::DeleteAtPosition { position } => {
                    if (position as usize) < self.content.len() {
                        self.content.remove(position as usize);
                    }
                }
                ListOp::InsertAfterKey { .. } => {
                    // Not supported in batch yet
                }
            }
        }

        Ok(())
    }

    /// Get the current document content as a string
    fn get_content(&self) -> String {
        self.content.iter().collect()
    }

    /// Get the current document length (character count)
    fn len(&self) -> usize {
        self.content.len()
    }

    /// Display the document with a header
    fn display(&self, label: &str) {
        let content = self.get_content();
        let char_count = self.len();
        println!("\n{}", "=".repeat(60));
        println!("{}", label);
        println!("{}", "-".repeat(60));
        println!("Document: \"{}\"", content);
        println!("Characters: {}", char_count);
        println!("{}", "=".repeat(60));
    }
}

fn main() {
    println!("\nCollaborative Editing Demo with GroveDB Merk List Mode\n");
    println!("This demo simulates multiple users editing a document concurrently");
    println!("using batch operations for efficient multi-character insertion.\n");

    // Create the shared document
    let mut doc = Document::new();

    // Create simulated users with colors
    let alice = User::new("Alice", "\x1b[34m"); // Blue
    let bob = User::new("Bob", "\x1b[32m");     // Green
    let carol = User::new("Carol", "\x1b[35m"); // Magenta

    // === Scenario 1: Initial document creation ===
    println!("\nScenario 1: Alice creates the initial document\n");

    let ops = alice.type_text("Hello World", 0);
    doc.apply_operations(ops).expect("failed to apply");
    doc.display("After Alice's initial text");

    // === Scenario 2: Bob edits in the middle ===
    println!("\nScenario 2: Bob inserts text in the middle\n");

    let ops = bob.type_text(" Beautiful", 5); // After "Hello"
    doc.apply_operations(ops).expect("failed to apply");
    doc.display("After Bob's insertion");

    // === Scenario 3: Carol appends at the end ===
    println!("\nScenario 3: Carol adds to the end\n");

    let current_len = doc.len();
    let ops = carol.type_text("!", current_len as u64);
    doc.apply_operations(ops).expect("failed to apply");
    doc.display("After Carol's addition");

    // === Scenario 4: Alice makes multiple edits ===
    println!("\nScenario 4: Alice makes corrections\n");

    // Delete "Beautiful " (10 chars at position 5)
    let mut ops = alice.delete_range(5, 10);
    doc.apply_operations(ops).expect("failed to delete");
    doc.display("After Alice deletes 'Beautiful '");

    // Insert "Awesome " instead
    ops = alice.type_text("Awesome ", 5);
    doc.apply_operations(ops).expect("failed to insert");
    doc.display("After Alice inserts 'Awesome '");

    // === Scenario 5: Concurrent edits (simulated) ===
    println!("\nScenario 5: Multiple users edit simultaneously (batch applied)\n");

    // Collect operations from multiple users
    let mut all_ops = Vec::new();

    // Bob adds at the end
    let current_len = doc.len();
    println!(
        "{}[Bob]{} prepares to add ' from GroveDB' at position {}",
        bob.color, "\x1b[0m", current_len
    );
    all_ops.extend(bob.type_text(" from GroveDB", current_len as u64));

    // Carol inserts at position 0
    println!(
        "{}[Carol]{} prepares to add '>>> ' at position 0",
        carol.color, "\x1b[0m"
    );
    all_ops.extend(carol.type_text(">>> ", 0));

    println!("\nApplying all operations as a single atomic batch...");
    doc.apply_operations(all_ops).expect("failed to apply batch");
    doc.display("After concurrent batch operations");

    // === Scenario 6: Complex editing workflow ===
    println!("\nScenario 6: Complex editing workflow\n");

    // Alice fixes the prefix
    let ops = alice.delete_range(0, 4); // Delete ">>> "
    doc.apply_operations(ops).expect("failed to delete");
    doc.display("After Alice removes prefix");

    // Bob adds emphasis
    let mut ops = bob.type_text("*** ", 0);
    doc.apply_operations(ops).expect("failed to insert");
    
    let current_len = doc.len();
    ops = bob.type_text(" ***", current_len as u64);
    doc.apply_operations(ops).expect("failed to insert");
    doc.display("After Bob adds emphasis");

    // Carol replaces a word (delete "Awesome", insert "Amazing")
    let mut ops = carol.delete_range(8, 8); // Delete "Awesome " (8 chars at position 4+4)
    doc.apply_operations(ops).expect("failed to delete");
    
    ops = carol.type_text("Amazing ", 8);
    doc.apply_operations(ops).expect("failed to insert");
    doc.display("After Carol's word replacement");

    // === Final summary ===
    println!("\nFinal Document Summary\n");
    println!("The document has been collaboratively edited by:");
    println!("  {}• Alice{} (Blue) - Created initial text, made corrections", alice.color, "\x1b[0m");
    println!("  {}• Bob{} (Green) - Added text, emphasis", bob.color, "\x1b[0m");
    println!("  {}• Carol{} (Magenta) - Made additions, replacements", carol.color, "\x1b[0m");
    println!();
    doc.display("FINAL DOCUMENT");

}
