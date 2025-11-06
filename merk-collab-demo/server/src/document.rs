use anyhow::{anyhow, Result};
use grovedb_merk::{Merk, ListOp, MerkType, TreeType};
use grovedb_merk::merk::prove::ProofConstructionResult;
use grovedb_path::SubtreePath;
use grovedb_storage::{rocksdb_storage::test_utils::TempStorage, Storage, StorageBatch};
use grovedb_version::version::GroveVersion;
use std::path::Path;

/// Represents a single character with its UUID
#[derive(Clone, Debug)]
struct Character {
    uuid: Vec<u8>,
    value: char,
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
    pub fn get_content(&self) -> Vec<(String, char)> {
        self.characters
            .iter()
            .map(|c| {
                let uuid_str = uuid::Uuid::from_slice(&c.uuid)
                    .map(|u| u.to_string())
                    .unwrap_or_else(|_| hex::encode(&c.uuid));
                (uuid_str, c.value)
            })
            .collect()
    }

    /// Insert a character at a specific position
    /// Returns the UUID, new root hash, and a positional proof
    pub fn insert(
        &mut self,
        position: usize,
        uuid: Vec<u8>,
        value: char,
    ) -> Result<(Vec<u8>, [u8; 32], Vec<u8>)> {
        // Create the insert operation using the provided UUID
        let op = ListOp::InsertAtPositionWithKey {
            position: position as u64,
            key: uuid.clone(),
            value: vec![value as u8],
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

        // Generate proof for the position
        let proof_result = self.merk.prove_position(position as u64, &self.grove_version)
            .value
            .map_err(|e| anyhow!("Failed to generate proof: {:?}", e))?;

        let root_hash = self.merk.root_hash().value;

        // Update local cache
        self.characters.insert(
            position,
            Character {
                uuid: actual_uuid.clone(),
                value,
            },
        );

        Ok((actual_uuid, root_hash, proof_result.proof))
    }

    /// Delete a character at a specific position
    /// Returns the UUID of the deleted character, new root hash, and a positional proof
    pub fn delete(&mut self, position: usize) -> Result<(Vec<u8>, [u8; 32], Vec<u8>)> {
        // Get the UUID at this position before deletion
        if position >= self.characters.len() {
            return Err(anyhow!("Position {} out of bounds", position));
        }

        let uuid = self.characters[position].uuid.clone();

        // Create delete operation
        let op = ListOp::DeleteAtPosition {
            position: position as u64,
        };

        // Apply the operation
        self.merk.apply_list_batch(&[op], &self.grove_version)
            .value
            .map_err(|e| anyhow!("Failed to delete: {:?}", e))?;

        // Generate proof AFTER deletion (to match the new root hash)
        // Note: For delete, we prove the position that now contains the next character
        // If we deleted the last character, prove position will fail, so we need to handle that
        let proof_result = if position < self.characters.len() - 1 {
            // There are more characters after this position
            self.merk.prove_position(position as u64, &self.grove_version)
                .value
                .map_err(|e| anyhow!("Failed to generate proof: {:?}", e))?
        } else {
            // We deleted the last character or the tree is now empty
            // Generate proof for the new last position (position - 1)
            if position > 0 {
                self.merk.prove_position((position - 1) as u64, &self.grove_version)
                    .value
                    .map_err(|e| anyhow!("Failed to generate proof: {:?}", e))?
            } else {
                // Tree is now empty, return an empty proof
                ProofConstructionResult::new(vec![], None)
            }
        };

        let root_hash = self.merk.root_hash().value;

        // Update local cache
        self.characters.remove(position);

        Ok((uuid, root_hash, proof_result.proof))
    }

    /// Get the size of the document (number of characters)
    pub fn len(&self) -> usize {
        self.characters.len()
    }

    /// Check if the document is empty
    pub fn is_empty(&self) -> bool {
        self.characters.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

        assert_eq!(doc.len(), 4);
        let content = doc.get_content();
        let text: String = content.iter().map(|(_, c)| c).collect();
        assert_eq!(text, "hllo");
    }
}
