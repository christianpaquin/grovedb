#![cfg(feature = "list_mode")]

use grovedb_merk::{Merk, MerkType, TreeType, ListOp, proofs::positional::verify_positional_proof};
use grovedb_storage::{rocksdb_storage::test_utils::TempStorage, Storage, StorageBatch};
use grovedb_path::SubtreePath;
use grovedb_version::version::GroveVersion;

#[test]
fn test_standalone_merk_proof_chain() {
    let storage = Box::leak(Box::new(TempStorage::new()));
    let batch = Box::leak(Box::new(StorageBatch::new()));
    let tx = Box::leak(Box::new(storage.start_transaction()));
    
    let grove_version = GroveVersion::latest();
    
    let context = storage
        .get_transactional_storage_context(SubtreePath::empty(), Some(batch), tx)
        .unwrap();
    
    let mut merk = Merk::open_empty(context, MerkType::StandaloneMerk, TreeType::ListTree);
    
    // Insert 'a', 'b', 'c'
    for (i, ch) in [b'a', b'b', b'c'].iter().enumerate() {
        let op = ListOp::InsertAtPosition { position: i as u64, value: vec![*ch] };
        merk.apply_list_batch(&[op], &grove_version).unwrap().unwrap();
        
        let root_hash = merk.root_hash().unwrap();
        let proof = merk.prove_position(i as u64, &grove_version).unwrap().unwrap();
        
        // Verify the proof
        let result = verify_positional_proof(&proof.proof, i as u64, root_hash, &grove_version);
        assert!(result.value.is_ok(), "Proof verification failed for position {}: {:?}", i, result.value.as_ref().err());
        
        let verified = result.unwrap().unwrap();
        assert_eq!(verified.value, vec![*ch]);
    }
    
    // Delete 'c' (position 2)
    let op_del = ListOp::DeleteAtPosition { position: 2 };
    merk.apply_list_batch(&[op_del], &grove_version).unwrap().unwrap();
    
    let root_hash_after_del = merk.root_hash().unwrap();
    
    // Verify proofs for remaining elements
    for i in 0..2 {
        let proof = merk.prove_position(i, &grove_version).unwrap().unwrap();
        let result = verify_positional_proof(&proof.proof, i, root_hash_after_del, &grove_version);
        assert!(result.value.is_ok(), "Proof verification failed after delete for position {}: {:?}", i, result.value.as_ref().err());
    }
    
    // Insert 'd' at position 2
    let op_insert_d = ListOp::InsertAtPosition { position: 2, value: vec![b'd'] };
    merk.apply_list_batch(&[op_insert_d], &grove_version).unwrap().unwrap();
    
    let root_hash_final = merk.root_hash().unwrap();
    
    // Verify all positions work
    for i in 0..3 {
        let proof = merk.prove_position(i, &grove_version).unwrap().unwrap();
        let result = verify_positional_proof(&proof.proof, i, root_hash_final, &grove_version);
        assert!(result.value.is_ok(), "Final proof verification failed for position {}: {:?}", i, result.value.as_ref().err());
    }
}
