use wasm_bindgen::prelude::*;
use grovedb_merk::proofs::positional::verify_positional_proof;
use grovedb_version::version::GroveVersion;
use serde::{Deserialize, Serialize};

// Re-export for JavaScript compatibility
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

// Wrapper types for JavaScript interop
#[derive(Serialize, Deserialize)]
#[wasm_bindgen]
pub struct VerificationResult {
    valid: bool,
    error: Option<String>,
}

#[wasm_bindgen]
impl VerificationResult {
    #[wasm_bindgen(getter)]
    pub fn valid(&self) -> bool {
        self.valid
    }

    #[wasm_bindgen(getter)]
    pub fn error(&self) -> Option<String> {
        self.error.clone()
    }
}

/// Initialize panic hook for better error messages in the browser
#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
}

/// Verify a positional proof
/// 
/// # Arguments
/// * `proof_bytes` - Serialized positional proof from the server
/// * `expected_root_hash` - The trusted root hash (32 bytes)
/// * `operation_type` - "insert" or "delete"
/// * `position` - The position index
/// * `uuid_bytes` - The 16-byte UUID (for insert, ignored for delete)
/// * `value` - The character value (for insert, ignored for delete)
///
/// # Returns
/// A `VerificationResult` indicating whether the proof is valid
#[wasm_bindgen]
pub fn verify_proof(
    proof_bytes: &[u8],
    expected_root_hash: &[u8],
    operation_type: &str,
    position: usize,
    uuid_bytes: Option<Vec<u8>>,
    value: Option<u8>,
) -> VerificationResult {
    // Validate root hash length
    if expected_root_hash.len() != 32 {
        return VerificationResult {
            valid: false,
            error: Some(format!(
                "Invalid root hash length: expected 32, got {}",
                expected_root_hash.len()
            )),
        };
    }

    let mut root_hash = [0u8; 32];
    root_hash.copy_from_slice(expected_root_hash);

    // Deserialize the proof
    let proof: PositionalProof = match bincode::deserialize(proof_bytes) {
        Ok(p) => p,
        Err(e) => {
            return VerificationResult {
                valid: false,
                error: Some(format!("Failed to deserialize proof: {}", e)),
            }
        }
    };

    // Prepare the operation data based on type
    let result = match operation_type {
        "insert" => {
            let uuid_bytes = match uuid_bytes {
                Some(bytes) if bytes.len() == 16 => bytes,
                Some(bytes) => {
                    return VerificationResult {
                        valid: false,
                        error: Some(format!(
                            "Invalid UUID length: expected 16, got {}",
                            bytes.len()
                        )),
                    }
                }
                None => {
                    return VerificationResult {
                        valid: false,
                        error: Some("UUID required for insert operation".to_string()),
                    }
                }
            };

            let value = match value {
                Some(v) => v,
                None => {
                    return VerificationResult {
                        valid: false,
                        error: Some("Value required for insert operation".to_string()),
                    }
                }
            };

            // Verify insertion proof
            verify_positional_proof(
                &proof,
                &root_hash,
                position,
                &uuid_bytes,
                &[value],
            )
        }
        "delete" => {
            // For deletion, we just verify the position exists
            // The UUID is derived from the proof itself
            verify_positional_proof(
                &proof,
                &root_hash,
                position,
                &[],  // UUID not needed for delete verification
                &[],  // Value not needed for delete verification
            )
        }
        _ => {
            return VerificationResult {
                valid: false,
                error: Some(format!("Unknown operation type: {}", operation_type)),
            }
        }
    };

    match result {
        Ok(_) => VerificationResult {
            valid: true,
            error: None,
        },
        Err(e) => VerificationResult {
            valid: false,
            error: Some(format!("Proof verification failed: {:?}", e)),
        },
    }
}

/// Get the size of the tree from a proof
/// This is useful for understanding proof complexity
#[wasm_bindgen]
pub fn get_proof_stats(proof_bytes: &[u8]) -> Result<JsValue, JsValue> {
    let proof: PositionalProof = bincode::deserialize(proof_bytes)
        .map_err(|e| JsValue::from_str(&format!("Failed to deserialize proof: {}", e)))?;

    #[derive(Serialize)]
    struct ProofStats {
        node_count: usize,
        proof_bytes: usize,
    }

    let stats = ProofStats {
        node_count: proof.path.len() + proof.left_nodes.len() + proof.right_nodes.len(),
        proof_bytes: proof_bytes.len(),
    };

    serde_wasm_bindgen::to_value(&stats)
        .map_err(|e| JsValue::from_str(&format!("Failed to serialize stats: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;

    #[wasm_bindgen_test]
    fn test_verification_result() {
        let result = VerificationResult {
            valid: true,
            error: None,
        };
        assert!(result.valid());
        assert!(result.error().is_none());
    }

    #[wasm_bindgen_test]
    fn test_invalid_root_hash_length() {
        let result = verify_proof(
            &[],
            &[0u8; 16], // Wrong length
            "insert",
            0,
            Some(vec![0u8; 16]),
            Some(b'a'),
        );
        assert!(!result.valid());
        assert!(result.error().unwrap().contains("Invalid root hash length"));
    }

    #[wasm_bindgen_test]
    fn test_missing_uuid_for_insert() {
        let result = verify_proof(
            &[],
            &[0u8; 32],
            "insert",
            0,
            None, // Missing UUID
            Some(b'a'),
        );
        assert!(!result.valid());
        assert!(result.error().unwrap().contains("UUID required"));
    }
}
