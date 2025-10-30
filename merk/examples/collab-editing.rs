// Collaborative Editing Demo
// 
// This demo simulates multiple users collaboratively editing a document using
// Merk's list-mode batch operations API. It demonstrates:
// - Multiple simulated users typing concurrently
// - Efficient batch operations for character insertion  
// - Document state visualization after each user action
// - Atomic multi-operation commits
//
// Run with: cargo run --example collab-editing
//
// See `uuid-collab-editing` and `uuid-collab-edit-with-proofs` for a key-based (vs. position-based) approach.

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
    fn type_text(&self, text: &str, position: u64) -> Vec<Operation> {
        println!(
            "{}[{}]{} types '{}' at position {}",
            self.color, self.name, "\x1b[0m", text, position
        );

        text.chars()
            .enumerate()
            .map(|(i, ch)| Operation::Insert {
                position: position + i as u64,
                value: ch,
            })
            .collect()
    }

    /// Simulate deleting a range of characters
    fn delete_range(&self, start_pos: u64, count: usize) -> Vec<Operation> {
        println!(
            "{}[{}]{} deletes {} character(s) at position {}",
            self.color, self.name, "\x1b[0m", count, start_pos
        );

        (0..count)
            .map(|_| Operation::Delete {
                position: start_pos,
            })
            .collect()
    }
}

/// Operation types (maps to ListOp in real implementation)
enum Operation {
    Insert { position: u64, value: char },
    Delete { position: u64 },
}

/// Shared document state
struct Document {
    content: Vec<char>,
    operation_count: usize,
    batch_count: usize,
}

impl Document {
    fn new() -> Self {
        Document {
            content: Vec::new(),
            operation_count: 0,
            batch_count: 0,
        }
    }

    /// Apply a batch of operations atomically
    /// 
    /// In the real implementation, this would call:
    /// ```
    /// merk.apply_list_batch(&ops, &grove_version)
    /// ```
    fn apply_operations(&mut self, ops: Vec<Operation>) {
        if ops.is_empty() {
            return;
        }

        self.batch_count += 1;
        self.operation_count += ops.len();

        // Apply operations to our simple in-memory document
        for op in ops {
            match op {
                Operation::Insert { position, value } => {
                    self.content.insert(position as usize, value);
                }
                Operation::Delete { position } => {
                    if (position as usize) < self.content.len() {
                        self.content.remove(position as usize);
                    }
                }
            }
        }
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
        println!("Total operations applied: {}", self.operation_count);
        println!("Total batch commits: {}", self.batch_count);
        println!("{}", "=".repeat(60));
    }
}

fn main() {
    println!("\nCollaborative Editing Demo with Merk List Mode\n");
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
    doc.apply_operations(ops);
    doc.display("After Alice's initial text");

    // === Scenario 2: Bob edits in the middle ===
    println!("\nScenario 2: Bob inserts text in the middle\n");

    let ops = bob.type_text(" Beautiful", 5); // After "Hello"
    doc.apply_operations(ops);
    doc.display("After Bob's insertion");

    // === Scenario 3: Carol appends at the end ===
    println!("\nScenario 3: Carol adds to the end\n");

    let current_len = doc.len();
    let ops = carol.type_text("!", current_len as u64);
    doc.apply_operations(ops);
    doc.display("After Carol's addition");

    // === Scenario 4: Alice makes multiple edits ===
    println!("\nScenario 4: Alice makes corrections\n");

    // Delete "Beautiful " (10 chars at position 5)
    let mut ops = alice.delete_range(5, 10);
    doc.apply_operations(ops);
    doc.display("After Alice deletes 'Beautiful '");

    // Insert "Awesome " instead
    ops = alice.type_text("Awesome ", 5);
    doc.apply_operations(ops);
    doc.display("After Alice inserts 'Awesome '");

    // === Scenario 5: Concurrent edits (simulated) ===
    println!("\nScenario 5: Multiple users edit simultaneously (batch applied)\n");

    // Collect operations from multiple users
    let mut all_ops = Vec::new();

    // Bob adds at the end
    let current_len = doc.len();
    println!(
        "{}[Bob]{} prepares to add ' from Merk' at position {}",
        bob.color, "\x1b[0m", current_len
    );
    all_ops.extend(bob.type_text(" from Merk", current_len as u64));

    // Carol inserts at position 0
    println!(
        "{}[Carol]{} prepares to add '>>> ' at position 0",
        carol.color, "\x1b[0m"
    );
    all_ops.extend(carol.type_text(">>> ", 0));

    println!("\nApplying all operations as a single atomic batch...");
    doc.apply_operations(all_ops);
    doc.display("After concurrent batch operations");

    // === Scenario 6: Complex editing workflow ===
    println!("\nScenario 6: Complex editing workflow\n");

    // Alice fixes the prefix
    let ops = alice.delete_range(0, 4); // Delete ">>> "
    doc.apply_operations(ops);
    doc.display("After Alice removes prefix");

    // Bob adds emphasis
    let mut ops = bob.type_text("*** ", 0);
    doc.apply_operations(ops);
    
    let current_len = doc.len();
    ops = bob.type_text(" ***", current_len as u64);
    doc.apply_operations(ops);
    doc.display("After Bob adds emphasis");

    // Carol replaces a word (delete "Awesome", insert "Amazing")
    let mut ops = carol.delete_range(8, 8); // Delete "Awesome " (8 chars at position 4+4)
    doc.apply_operations(ops);
    
    ops = carol.type_text("Amazing ", 8);
    doc.apply_operations(ops);
    doc.display("After Carol's word replacement");

    // === Final summary ===
    println!("\nFinal Document Summary\n");
    println!("The document has been collaboratively edited by:");
    println!("  {}• Alice{} (Blue) - Created initial text, made corrections", alice.color, "\x1b[0m");
    println!("  {}• Bob{} (Green) - Added text, emphasis", bob.color, "\x1b[0m");
    println!("  {}• Carol{} (Magenta) - Made additions, replacements", carol.color, "\x1b[0m");
    println!();
    doc.display("FINAL DOCUMENT");

    println!();

}
