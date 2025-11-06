/**
 * Simplified Merkle Proof Verification for Positional Proofs
 * 
 * This is a TypeScript implementation of Merk's positional proof verification.
 * It parses the binary proof format and reconstructs the tree to verify the root hash.
 * 
 * Based on merk/src/proofs/encoding.rs and merk/src/proofs/positional.rs
 * 
 * STATUS: Partially implemented
 * ✅ Proof decoding (parsing binary format)
 * ✅ Target node extraction
 * ❌ Tree reconstruction from proof operations
 * ❌ Blake3 hashing integration
 * ❌ Root hash computation and verification
 * 
 * TODO to complete:
 * 1. Install blake3-wasm: npm install blake3-wasm
 * 2. Implement executeProofOps() to reconstruct tree from operations
 * 3. Implement computeTreeHash() to recursively hash nodes
 * 4. Compare computed root hash with expected hash
 * 
 * For now, this serves as a foundation and does basic proof parsing.
 */

// Blake3 hashing - we'll use a dynamic import
/**
 * Blake3 hash function (dynamically imported)
 */
let blake3Fn: any = null;
let blake3Initialized = false;

async function initBlake3(): Promise<boolean> {
  if (blake3Initialized) return true;
  
  try {
    // Dynamically import blake3
    const module = await import('@noble/hashes/blake3.js');
    blake3Fn = module.blake3;
    
    // Test that blake3 is available
    const test = blake3Fn('test');
    if (test && test.length === 32) {
      blake3Initialized = true;
      console.log('[ProofVerifier] Blake3 initialized successfully');
      return true;
    }
    return false;
  } catch (err) {
    console.error('[ProofVerifier] Failed to initialize Blake3:', err);
    return false;
  }
}

// Proof operation opcodes (from merk/src/proofs/encoding.rs)
enum OpCode {
  PushHash = 0x01,
  PushKVHash = 0x02,
  PushKV = 0x03,
  PushKVValueHash = 0x04,
  PushKVDigest = 0x05,
  PushKVRefValueHash = 0x06,
  PushKVValueHashFeatureType = 0x07,
  
  PushInvertedHash = 0x08,
  PushInvertedKVHash = 0x09,
  PushInvertedKV = 0x0a,
  PushInvertedKVValueHash = 0x0b,
  PushInvertedKVDigest = 0x0c,
  PushInvertedKVRefValueHash = 0x0d,
  PushInvertedKVValueHashFeatureType = 0x0e,
  
  // List-mode variants with subtree_size
  PushHashWithSubtreeSize = 0x14,
  PushKVWithSubtreeSize = 0x15,
  PushKVValueHashWithSubtreeSize = 0x16,
  
  Parent = 0x10,
  Child = 0x11,
  ParentInverted = 0x12,
  ChildInverted = 0x13,
}

// TreeNode structure for future tree reconstruction
/**
 * Tree node structure for reconstruction (will be used in tree reconstruction phase)
 * 
 * interface TreeNode {
 *   hash: Uint8Array;
 *   left?: TreeNode;
 *   right?: TreeNode;
 *   key?: Uint8Array;
 *   value?: Uint8Array;
 * }
 */

interface ProofOp {
  opcode: OpCode;
  data: {
    key?: Uint8Array;
    value?: Uint8Array;
    hash?: Uint8Array;
    subtreeSize?: number;
  };
}

/**
 * Proof decoder - parses binary proof format
 */
class ProofDecoder {
  private data: Uint8Array;
  private pos: number = 0;

  constructor(proofBytes: Uint8Array) {
    this.data = proofBytes;
  }

  hasMore(): boolean {
    return this.pos < this.data.length;
  }

  private readByte(): number {
    if (this.pos >= this.data.length) {
      throw new Error('Unexpected end of proof data');
    }
    return this.data[this.pos++];
  }

  private readBytes(n: number): Uint8Array {
    if (this.pos + n > this.data.length) {
      throw new Error(`Unexpected end of proof data: need ${n} bytes`);
    }
    const bytes = this.data.slice(this.pos, this.pos + n);
    this.pos += n;
    return bytes;
  }

  private readU16(): number {
    const b1 = this.readByte();
    const b2 = this.readByte();
    return (b2 << 8) | b1; // Little endian
  }

  private readU64(): bigint {
    let value = 0n;
    for (let i = 0; i < 8; i++) {
      value |= BigInt(this.readByte()) << BigInt(i * 8);
    }
    return value;
  }

  decodeOp(): ProofOp | null {
    if (!this.hasMore()) return null;

    const opcode = this.readByte() as OpCode;
    const data: ProofOp['data'] = {};

    switch (opcode) {
      case OpCode.PushHash:
      case OpCode.PushInvertedHash:
        data.hash = this.readBytes(32); // HASH_LENGTH = 32
        break;

      case OpCode.PushKVHash:
      case OpCode.PushInvertedKVHash:
        data.hash = this.readBytes(32);
        break;

      case OpCode.PushKV:
      case OpCode.PushInvertedKV: {
        const keyLen = this.readByte();
        data.key = this.readBytes(keyLen);
        const valueLen = this.readU16();
        data.value = this.readBytes(valueLen);
        break;
      }

      case OpCode.PushKVValueHash:
      case OpCode.PushInvertedKVValueHash: {
        const keyLen = this.readByte();
        data.key = this.readBytes(keyLen);
        const valueLen = this.readU16();
        data.value = this.readBytes(valueLen);
        data.hash = this.readBytes(32); // value_hash
        break;
      }

      case OpCode.PushHashWithSubtreeSize:
        data.hash = this.readBytes(32);
        data.subtreeSize = Number(this.readU64());
        break;

      case OpCode.PushKVWithSubtreeSize: {
        const keyLen = this.readByte();
        data.key = this.readBytes(keyLen);
        const valueLen = this.readU16();
        data.value = this.readBytes(valueLen);
        data.subtreeSize = Number(this.readU64());
        break;
      }

      case OpCode.PushKVValueHashWithSubtreeSize: {
        const keyLen = this.readByte();
        data.key = this.readBytes(keyLen);
        const valueLen = this.readU16();
        data.value = this.readBytes(valueLen);
        data.hash = this.readBytes(32); // value_hash
        data.subtreeSize = Number(this.readU64());
        break;
      }

      case OpCode.Parent:
      case OpCode.Child:
      case OpCode.ParentInverted:
      case OpCode.ChildInverted:
        // These have no additional data
        break;

      default:
        throw new Error(`Unknown opcode: 0x${opcode.toString(16)}`);
    }

    return { opcode, data };
  }

  decodeAll(): ProofOp[] {
    const ops: ProofOp[] = [];
    while (this.hasMore()) {
      const op = this.decodeOp();
      if (op) ops.push(op);
    }
    return ops;
  }
}

/**
 * Verify a positional Merkle proof
 * 
 * @param proofBytes - Base64-encoded proof from server
 * @param expectedRootHash - Hex-encoded trusted root hash
 * @param position - Position in the tree
 * @param expectedKey - Expected UUID at position (optional check)
 * @param expectedValue - Expected value at position (optional check)
 * @returns true if proof is valid
 */
export async function verifyPositionalProof(
  proofBase64: string,
  _expectedRootHashHex: string, // Will be used for root hash comparison when implemented
  _position: number, // Will be used for position verification when implemented
  expectedUuid: string,
  expectedValue: string
): Promise<boolean> {
  try {
    // Initialize Blake3 if needed
    const hasBlake3 = await initBlake3();
    if (!hasBlake3) {
      console.error('[ProofVerifier] Blake3 not available - cannot verify proofs');
      return false;
    }

    // Decode proof from base64
    const proofBytes = Uint8Array.from(atob(proofBase64), c => c.charCodeAt(0));
    
    // Decode expected root hash from hex (will be used for verification)
    // Commented out until root hash verification is implemented
    /*
    const expectedRootHash = new Uint8Array(
      expectedRootHashHex.match(/.{1,2}/g)!.map(byte => parseInt(byte, 16))
    );
    */

    // Parse proof operations
    const decoder = new ProofDecoder(proofBytes);
    const ops = decoder.decodeAll();

    console.log(`[ProofVerifier] Decoded ${ops.length} proof operations`);

    // Find the target node (the KV node with subtree_size=1)
    let targetKey: Uint8Array | undefined;
    let targetValue: Uint8Array | undefined;

    for (const op of ops) {
      if (op.data.key && op.data.value && op.data.subtreeSize === 1) {
        targetKey = op.data.key;
        targetValue = op.data.value;
        break;
      }
    }

    if (!targetKey || !targetValue) {
      console.error('[ProofVerifier] Could not find target node in proof');
      return false;
    }

    // Verify the key matches expected UUID
    const keyUuid = arrayToUuid(targetKey);
    if (keyUuid !== expectedUuid) {
      console.error(`[ProofVerifier] Key mismatch: expected ${expectedUuid}, got ${keyUuid}`);
      return false;
    }

    // Verify the value matches expected value
    const valueStr = new TextDecoder().decode(targetValue);
    if (valueStr !== expectedValue) {
      console.error(`[ProofVerifier] Value mismatch: expected ${expectedValue}, got ${valueStr}`);
      return false;
    }

    // Reconstruct tree and compute root hash
    // This is the complex part - for now, we'll return a placeholder
    // TODO: Implement tree reconstruction and hashing
    
    console.warn('[ProofVerifier] Tree reconstruction not yet implemented - using mock verification');
    
    return true; // Temporary - still using mock verification

  } catch (error) {
    console.error('[ProofVerifier] Error:', error instanceof Error ? error.message : String(error));
    return false;
  }
}

/**
 * Convert byte array to UUID string
 */
function arrayToUuid(bytes: Uint8Array): string {
  if (bytes.length !== 16) {
    throw new Error(`Invalid UUID length: ${bytes.length}`);
  }
  
  const hex = Array.from(bytes)
    .map(b => b.toString(16).padStart(2, '0'))
    .join('');
  
  // Format as UUID: xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

/**
 * Helper to print bytes as hex for debugging
 * 
 * function bytesToHex(bytes: Uint8Array): string {
 *   return Array.from(bytes)
 *     .map(b => b.toString(16).padStart(2, '0'))
 *     .join('');
 * }
 */
