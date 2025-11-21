use anyhow::{anyhow, Result};
use grovedb_merk::{ListOp, Merk, MerkType, TreeType};
use grovedb_path::SubtreePath;
use grovedb_storage::{rocksdb_storage::test_utils::TempStorage, Storage, StorageBatch};
use grovedb_version::version::GroveVersion;
use std::path::Path;

/// Represents a single character with its UUID and deletion status
#[derive(Clone, Debug)]
struct Character {
    uuid: Vec<u8>,
    value: char,
    deleted: bool,
}

/// Encode a character value with deletion flag
/// Format: [deleted_flag, char_byte]
/// - deleted_flag: 0 = active, 1 = deleted (tombstone)
/// - char_byte: the character value
fn encode_value(ch: char, deleted: bool) -> Vec<u8> {
    vec![if deleted { 1 } else { 0 }, ch as u8]
}

/// Decode a character value with deletion flag
/// Returns (char, is_deleted)
#[allow(dead_code)]
fn decode_value(bytes: &[u8]) -> Result<(char, bool)> {
    if bytes.len() != 2 {
        return Err(anyhow!("Invalid value format: expected 2 bytes, got {}", bytes.len()));
    }
    let deleted = bytes[0] != 0;
    let ch = bytes[1] as char;
    Ok((ch, deleted))
}

pub struct Document {
    merk: Merk<grovedb_storage::rocksdb_storage::PrefixedRocksDbTransactionContext<'static>>,
    grove_version: GroveVersion,
    // Cache of characters in order
    characters: Vec<Character>,
}

// Safety: Document is protected by a Mutex in the server, so only one thread
// accesses it at a time. The Send+Sync traits are required by Axum's routing.
unsafe impl Send for Document {}
unsafe impl Sync for Document {}

impl Document {
    /// Create a new document with an empty Merk tree
    pub fn new(_path: &Path) -> Result<Self> {
        let storage = Box::leak(Box::new(TempStorage::new()));
        let batch = Box::leak(Box::new(StorageBatch::new()));
        let tx = Box::leak(Box::new(storage.start_transaction()));
        
        let grove_version = GroveVersion::latest();
        
        let context = storage
            .get_transactional_storage_context(SubtreePath::empty(), Some(batch), tx)
            .value;
        
        let merk = Merk::open_empty(context, MerkType::StandaloneMerk, TreeType::ListTree);
        
        Ok(Self {
            merk,
            grove_version: grove_version.clone(),
            characters: Vec::new(),
        })
    }

    /// Get the current root hash
    pub fn root_hash(&self) -> Result<[u8; 32]> {
        Ok(self.merk.root_hash().value)
    }

    /// Get the root hash as a hex string
    pub fn root_hash_hex(&self) -> String {
        self.root_hash()
            .map(|h| hex::encode(h))
            .unwrap_or_else(|_| "error".to_string())
    }

    /// Get the entire document content as a list of (uuid, char) pairs
    /// Only returns non-deleted (active) characters
    pub fn get_content(&self) -> Vec<(String, char)> {
        self.characters
            .iter()
            .filter(|c| !c.deleted)  // Filter out tombstones
            .map(|c| {
                let uuid_str = uuid::Uuid::from_slice(&c.uuid)
                    .map(|u| u.to_string())
                    .unwrap_or_else(|_| hex::encode(&c.uuid));
                (uuid_str, c.value)
            })
            .collect()
    }

    /// Convert a visible position (counting only active characters) to tree position (including tombstones)
    #[allow(dead_code)]
    fn visible_to_tree_position(&self, visible_pos: usize) -> usize {
        let mut active_count = 0;
        for (tree_pos, ch) in self.characters.iter().enumerate() {
            if !ch.deleted {
                if active_count == visible_pos {
                    return tree_pos;
                }
                active_count += 1;
            }
        }
        // If visible_pos is beyond all active characters, return position after last character
        self.characters.len()
    }

    /// Convert a tree position (including tombstones) to visible position (counting only active characters)
    #[allow(dead_code)]
    fn tree_to_visible_position(&self, tree_pos: usize) -> usize {
        self.characters.iter()
            .take(tree_pos)
            .filter(|c| !c.deleted)
            .count()
    }

    /// Insert a character after a specific UUID (reference-based operation)
    /// This implements Matt Weidner's "Text Without CRDTs" design
    /// Returns the UUID, new root hash, and a positional proof
    pub fn insert_after(
        &mut self,
        target_uuid: Option<Vec<u8>>,  // None means insert at beginning
        uuid: Vec<u8>,
        value: char,
    ) -> Result<(Vec<u8>, [u8; 32], Vec<u8>)> {
        // Calculate tree position BEFORE inserting (for cache update and proof generation)
        let tree_position = if let Some(ref target_key) = target_uuid {
            // Find target position in cache, insert after it
            self.find_uuid_position(target_key)? + 1
        } else {
            0  // Insert at beginning
        };

        let op = if let Some(target_key) = target_uuid {
            // Use reference-based InsertAfterKeyWithKey (pure reference-based!)
            // With UpdateValueByKey for deletions, this now works reliably
            ListOp::InsertAfterKeyWithKey {
                target_key,
                key: uuid.clone(),
                value: encode_value(value, false),
            }
        } else {
            // Insert at beginning (no target)
            ListOp::InsertAtPositionWithKey {
                position: 0,
                key: uuid.clone(),
                value: encode_value(value, false),
            }
        };

        // Apply the operation
        let result = self.merk.apply_list_batch(&[op], &self.grove_version)
            .value
            .map_err(|e| anyhow!("Failed to apply operation: {:?}", e))?;
        
        // The UUID should match what we provided
        let actual_uuid = result.keys.get(0)
            .ok_or_else(|| anyhow!("No key returned from operation"))?
            .clone();

        // Verify it matches (should always match for client-provided UUIDs)
        if actual_uuid != uuid {
            return Err(anyhow!("UUID mismatch: expected {:?}, got {:?}", uuid, actual_uuid));
        }

        // For reference-based operations, the actual tree position might differ from
        // our calculated position due to tree rebalancing. We need to find where
        // the UUID actually ended up in the tree.
        let actual_tree_position = self.find_actual_tree_position(&uuid)?;

        // Generate proof for the ACTUAL tree position
        let proof_result = self.merk.prove_position(actual_tree_position as u64, &self.grove_version)
            .value
            .map_err(|e| anyhow!("Failed to generate proof: {:?}", e))?;

        let root_hash = self.merk.root_hash().value;

        // Update local cache at the calculated position (for future lookups)
        self.characters.insert(
            tree_position,
            Character {
                uuid: actual_uuid.clone(),
                value,
                deleted: false,
            },
        );

        Ok((actual_uuid, root_hash, proof_result.proof))
    }

    /// Find the actual position of a UUID in the Merk tree by trying positions
    fn find_actual_tree_position(&self, uuid: &[u8]) -> Result<usize> {
        self
            .merk
            .get_key_position(uuid)
            .map(|pos| pos as usize)
            .ok_or_else(|| anyhow!("Could not find UUID in tree at any position"))
    }

    /// Find the position of a UUID in the character cache
    fn find_uuid_position(&self, uuid: &[u8]) -> Result<usize> {
        self.characters
            .iter()
            .position(|c| c.uuid == uuid)
            .ok_or_else(|| anyhow!("UUID not found in cache"))
    }

    /// Delete a character by UUID (reference-based operation)
    /// Returns the UUID, new root hash, and a KEY-BASED proof (not positional)
    pub fn delete_by_uuid(&mut self, uuid: Vec<u8>) -> Result<(Vec<u8>, [u8; 32], Vec<u8>)> {
        // Find the character with this UUID
        let tree_position = self.find_uuid_position(&uuid)?;
        
        let character = &self.characters[tree_position];
        if character.deleted {
            return Err(anyhow!("Character with UUID {:?} already deleted", uuid));
        }

        let value = character.value;

        let op = ListOp::UpdateValueByKey {
            key: uuid.clone(),
            value: encode_value(value, true),
        };

        self.merk
            .apply_list_batch(&[op], &self.grove_version)
            .value
            .map_err(|e| anyhow!("Failed to mark as deleted: {:?}", e))?;

        // Generate a positional proof for the UUID's actual position (tombstone)
        let actual_tree_position = self.find_actual_tree_position(&uuid)?;
        let proof_result = self
            .merk
            .prove_position(actual_tree_position as u64, &self.grove_version)
            .value
            .map_err(|e| anyhow!("Failed to generate proof: {:?}", e))?;

        // Get root hash AFTER prove() to see if order matters
        let root_hash = self.merk.root_hash().value;

        // Update local cache to mark as deleted (at the cached tree position)
        self.characters[tree_position].deleted = true;

        Ok((uuid, root_hash, proof_result.proof))
    }

    /// Insert a character at a specific VISIBLE position (client perspective)
    /// DEPRECATED: Use insert_after() for reference-based operations
    /// The position counts only active characters, not tombstones
    /// Returns the UUID, new root hash, and a positional proof
    #[allow(dead_code)]
    pub fn insert(
        &mut self,
        visible_position: usize,
        uuid: Vec<u8>,
        value: char,
    ) -> Result<(Vec<u8>, [u8; 32], Vec<u8>)> {
        // Convert visible position to tree position
        let tree_position = self.visible_to_tree_position(visible_position);
        
        // Create the insert operation using the provided UUID
        // Value format: [deleted_flag=0, char_byte]
        let op = ListOp::InsertAtPositionWithKey {
            position: tree_position as u64,
            key: uuid.clone(),
            value: encode_value(value, false),  // Insert as active (not deleted)
        };

        // Apply the operation
        let result = self.merk.apply_list_batch(&[op], &self.grove_version)
            .value
            .map_err(|e| anyhow!("Failed to apply operation: {:?}", e))?;
        
        // The UUID should match what we provided
        let actual_uuid = result.keys.get(0)
            .ok_or_else(|| anyhow!("No key returned from operation"))?
            .clone();

        // Verify it matches (should always match for client-provided UUIDs)
        if actual_uuid != uuid {
            return Err(anyhow!("UUID mismatch: expected {:?}, got {:?}", uuid, actual_uuid));
        }

        // Generate proof for the tree position
        let proof_result = self.merk.prove_position(tree_position as u64, &self.grove_version)
            .value
            .map_err(|e| anyhow!("Failed to generate proof: {:?}", e))?;

        let root_hash = self.merk.root_hash().value;

        // Update local cache at tree position
        self.characters.insert(
            tree_position,
            Character {
                uuid: actual_uuid.clone(),
                value,
                deleted: false,
            },
        );

        Ok((actual_uuid, root_hash, proof_result.proof))
    }

    /// Delete a character at a specific VISIBLE position (client perspective)
    /// The position counts only active characters, not tombstones
    /// Returns the UUID of the deleted character, new root hash, and a positional proof
    #[allow(dead_code)]
    pub fn delete(&mut self, visible_position: usize) -> Result<(Vec<u8>, [u8; 32], Vec<u8>)> {
        // Convert visible position to tree position
        let tree_position = self.visible_to_tree_position(visible_position);
        
        // Get the character at this tree position
        if tree_position >= self.characters.len() {
            return Err(anyhow!("Position {} out of bounds", visible_position));
        }

        let character = &self.characters[tree_position];
        if character.deleted {
            return Err(anyhow!("Character at visible position {} already deleted", visible_position));
        }

        let uuid = character.uuid.clone();
        let value = character.value;

        let op = ListOp::UpdateValueByKey {
            key: uuid.clone(),
            value: encode_value(value, true),
        };

        self.merk
            .apply_list_batch(&[op], &self.grove_version)
            .value
            .map_err(|e| anyhow!("Failed to mark as deleted: {:?}", e))?;

        // Generate proof AFTER marking as deleted using actual tree position
        let actual_tree_position = self.find_actual_tree_position(&uuid)?;
        let proof_result = self
            .merk
            .prove_position(actual_tree_position as u64, &self.grove_version)
            .value
            .map_err(|e| anyhow!("Failed to generate proof: {:?}", e))?;

        let root_hash = self.merk.root_hash().value;

        // Update local cache to mark as deleted (at tree position)
        self.characters[tree_position].deleted = true;

        Ok((uuid, root_hash, proof_result.proof))
    }

    /// Get the size of the document (number of active/non-deleted characters)
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.characters.iter().filter(|c| !c.deleted).count()
    }

    /// Check if the document is empty
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.characters.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use grovedb_merk::proofs::positional::verify_positional_proof;

    fn assert_positional_proof_contains(
        proof: &[u8],
        uuid: &[u8],
        expected_flag: u8,
        expected_char: Option<char>,
        root: [u8; 32],
        grove_version: &GroveVersion,
    ) {
        for position in 0..1000u64 {
            let result = verify_positional_proof(proof, position, root, grove_version).value;
            match result {
                Ok(proof_result) if proof_result.key == uuid => {
                    assert!(proof_result.value.len() >= 2);
                    assert_eq!(proof_result.value[0], expected_flag);
                    if let Some(ch) = expected_char {
                        assert_eq!(proof_result.value[1], ch as u8);
                    }
                    return;
                }
                Ok(_) => continue,
                Err(_) => continue,
            }
        }
        panic!("positional proof did not resolve to target uuid");
    }

    #[test]
    fn test_empty_document() {
        let temp = tempfile::tempdir().unwrap();
        let doc = Document::new(temp.path()).unwrap();
        assert!(doc.is_empty());
        assert_eq!(doc.len(), 0);
        assert_eq!(doc.get_content().len(), 0);
    }

    #[test]
    fn test_insert_and_retrieve() {
        let temp = tempfile::tempdir().unwrap();
        let mut doc = Document::new(temp.path()).unwrap();

        // Insert "hello"
        let chars = vec!['h', 'e', 'l', 'l', 'o'];
        for (i, &c) in chars.iter().enumerate() {
            let uuid = uuid::Uuid::new_v4().as_bytes().to_vec();
            doc.insert(i, uuid, c).unwrap();
        }

        assert_eq!(doc.len(), 5);
        let content = doc.get_content();
        assert_eq!(content.len(), 5);

        let text: String = content.iter().map(|(_, c)| c).collect();
        assert_eq!(text, "hello");
    }

    #[test]
    fn test_delete() {
        let temp = tempfile::tempdir().unwrap();
        let mut doc = Document::new(temp.path()).unwrap();

        // Insert "hello"
        let chars = vec!['h', 'e', 'l', 'l', 'o'];
        for (i, &c) in chars.iter().enumerate() {
            let uuid = uuid::Uuid::new_v4().as_bytes().to_vec();
            doc.insert(i, uuid, c).unwrap();
        }

        // Delete the 'e' at position 1
        doc.delete(1).unwrap();

        // Active characters should be 4 (tombstone still exists but is hidden)
        assert_eq!(doc.len(), 4);
        let content = doc.get_content();
        assert_eq!(content.len(), 4);
        let text: String = content.iter().map(|(_, c)| c).collect();
        assert_eq!(text, "hllo");
        
        // Total characters (including tombstone) should still be 5
        assert_eq!(doc.characters.len(), 5);
        assert!(doc.characters[1].deleted);  // The 'e' is marked as deleted
    }

    #[test]
    fn test_tombstones_keep_position() {
        let temp = tempfile::tempdir().unwrap();
        let mut doc = Document::new(temp.path()).unwrap();

        // Insert "abc"
        let chars = vec!['a', 'b', 'c'];
        for (i, &c) in chars.iter().enumerate() {
            let uuid = uuid::Uuid::new_v4().as_bytes().to_vec();
            doc.insert(i, uuid, c).unwrap();
        }

        // Delete 'b' at position 1
        let (deleted_uuid, _, _) = doc.delete(1).unwrap();

        // Verify tombstone exists at position 1
        assert_eq!(doc.characters.len(), 3);
        assert_eq!(doc.characters[1].uuid, deleted_uuid);
        assert_eq!(doc.characters[1].value, 'b');
        assert!(doc.characters[1].deleted);

        // Verify 'c' is still at position 2 (tombstone preserves positions)
        assert_eq!(doc.characters[2].value, 'c');
        assert!(!doc.characters[2].deleted);

        // Active content is "ac"
        let content = doc.get_content();
        let text: String = content.iter().map(|(_, c)| c).collect();
        assert_eq!(text, "ac");
    }

    #[test]
    fn test_reference_flow_matches_auditor() {
        use grovedb_merk::proofs::positional::verify_positional_proof;

        fn verify_insert(
            proof: &[u8],
            uuid: &[u8],
            value: char,
            root: [u8; 32],
            grove_version: &GroveVersion,
        ) {
            for position in 0..1000u64 {
                match verify_positional_proof(proof, position, root, grove_version).value {
                    Ok(result) if result.key == uuid => {
                        assert_eq!(result.value, vec![0, value as u8]);
                        return;
                    }
                    Ok(_) => continue,
                    Err(_) => continue,
                }
            }
            panic!("positional proof did not resolve to target uuid");
        }

        let temp = tempfile::tempdir().unwrap();
        let mut doc = Document::new(temp.path()).unwrap();

        let uuid_a = uuid::Uuid::new_v4().as_bytes().to_vec();
        let (key_a, root_a, proof_a) = doc
            .insert_after(None, uuid_a.clone(), 'A')
            .expect("insert A");
        verify_insert(&proof_a, &key_a, 'A', root_a, &doc.grove_version);

        let uuid_b = uuid::Uuid::new_v4().as_bytes().to_vec();
        let (key_b, root_b, proof_b) = doc
            .insert_after(Some(uuid_a.clone()), uuid_b.clone(), 'B')
            .expect("insert B");
        verify_insert(&proof_b, &key_b, 'B', root_b, &doc.grove_version);

        let uuid_c = uuid::Uuid::new_v4().as_bytes().to_vec();
        let (key_c, root_c, proof_c) = doc
            .insert_after(Some(uuid_b.clone()), uuid_c.clone(), 'C')
            .expect("insert C");
        verify_insert(&proof_c, &key_c, 'C', root_c, &doc.grove_version);

        let (deleted_c, _root_delete, _delete_proof) = doc
            .delete_by_uuid(uuid_c.clone())
            .expect("delete C");
        assert_eq!(deleted_c, uuid_c);

        let uuid_d = uuid::Uuid::new_v4().as_bytes().to_vec();
        let (key_d, root_d, proof_d) = doc
            .insert_after(Some(uuid_b.clone()), uuid_d.clone(), 'D')
            .expect("insert D");
        assert!(doc.merk.get_key_position(&key_d).is_some());
        verify_insert(&proof_d, &key_d, 'D', root_d, &doc.grove_version);

        let uuid_e = uuid::Uuid::new_v4().as_bytes().to_vec();
        let (key_e, root_e, proof_e) = doc
            .insert_after(Some(uuid_d.clone()), uuid_e.clone(), 'E')
            .expect("insert E");
        verify_insert(&proof_e, &key_e, 'E', root_e, &doc.grove_version);
    }

    #[test]
    fn test_delete_key_proof_contains_tombstone() {
        let temp = tempfile::tempdir().unwrap();
        let mut doc = Document::new(temp.path()).unwrap();

        let uuid = uuid::Uuid::new_v4().as_bytes().to_vec();
        let (inserted_uuid, _, _) =
            doc.insert_after(None, uuid.clone(), 'x').expect("insert x");

        let (deleted_uuid, delete_root, delete_proof) =
            doc.delete_by_uuid(inserted_uuid.clone()).expect("delete x");
        assert_eq!(deleted_uuid, uuid);

        assert_positional_proof_contains(
            &delete_proof,
            &uuid,
            1,
            Some('x'),
            delete_root,
            &doc.grove_version,
        );
    }

    #[test]
    fn test_delete_proof_after_subsequent_ops_verifies() {
        let temp = tempfile::tempdir().unwrap();
        let mut doc = Document::new(temp.path()).unwrap();

        let uuid_a = uuid::Uuid::parse_str("4077b53e-1409-45d9-8b86-f1a9f8277ef6")
            .unwrap()
            .as_bytes()
            .to_vec();
        let (uuid_a, _, _) = doc
            .insert_after(None, uuid_a, 'A')
            .expect("insert A");

        let uuid_b = uuid::Uuid::parse_str("eb9d9c3a-721a-45be-a1ad-1d0cdc9be8be")
            .unwrap()
            .as_bytes()
            .to_vec();
        doc.insert_after(Some(uuid_a.clone()), uuid_b.clone(), 'B')
            .expect("insert B");

        doc.delete_by_uuid(uuid_b.clone()).expect("delete B");

        let uuid_c = uuid::Uuid::parse_str("13212858-f697-47a1-a407-932ca55b8bec")
            .unwrap()
            .as_bytes()
            .to_vec();
        doc.insert_after(Some(uuid_a.clone()), uuid_c.clone(), '!')
            .expect("insert C");

        let (deleted_uuid, delete_root, delete_proof) =
            doc.delete_by_uuid(uuid_a.clone()).expect("delete A");
        assert_eq!(deleted_uuid, uuid_a);

        assert_positional_proof_contains(
            &delete_proof,
            &uuid_a,
            1,
            Some('A'),
            delete_root,
            &doc.grove_version,
        );
    }
}
