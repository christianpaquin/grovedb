use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose, Engine as _};
use clap::Parser;
use colored::*;
use grovedb_merk::proofs::positional::verify_positional_proof;
use grovedb_version::version::GroveVersion;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

/// Changelog entry from the server
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChangelogEntry {
    op_index: u64,
    operation: String,
    target_uuid: Option<String>,  // For insert: UUID inserted after (None = beginning)
    uuid: String,  // UUID of the character being inserted/deleted
    value: Option<char>,
    proof: String,
    new_root_hash: String,
}

/// Audit statistics
#[derive(Debug, Default)]
struct AuditStats {
    total_operations: usize,
    inserts: usize,
    deletes: usize,
    verified_proofs: usize,
    failed_proofs: usize,
}

/// Command-line arguments
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the changelog.jsonl file
    changelog_path: PathBuf,

    /// Verbose output (show each operation)
    #[arg(short, long)]
    verbose: bool,

    /// Stop on first error
    #[arg(short, long)]
    stop_on_error: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    println!("{}", "╔═══════════════════════════════════════════════════════════════╗".bright_cyan());
    println!("{}", "║         Merk Collaborative Editor - Changelog Auditor         ║".bright_cyan());
    println!("{}", "╚═══════════════════════════════════════════════════════════════╝".bright_cyan());
    println!();
    println!("Changelog: {}", args.changelog_path.display().to_string().bright_white());
    println!();

    // Open and read the changelog file
    let file = File::open(&args.changelog_path)
        .context("Failed to open changelog file. Make sure the server has run and created operations.")?;
    let reader = BufReader::new(file);

    let grove_version = GroveVersion::latest();
    let mut stats = AuditStats::default();
    let mut current_root_hash: Option<[u8; 32]> = None;

    println!("{}", "Starting verification...".bright_yellow());
    println!();

    for (line_num, line) in reader.lines().enumerate() {
        let line = line.context("Failed to read line from changelog")?;
        
        if line.trim().is_empty() {
            continue;
        }

        let entry: ChangelogEntry = serde_json::from_str(&line)
            .with_context(|| format!("Failed to parse JSON at line {}", line_num + 1))?;

        stats.total_operations += 1;
        match entry.operation.as_str() {
            "insert" => stats.inserts += 1,
            "delete" => stats.deletes += 1,
            _ => {}
        }

        if args.verbose {
            let op_detail = match entry.operation.as_str() {
                "insert" => {
                    let target_info = if let Some(target) = &entry.target_uuid {
                        format!("after UUID {}", &target[..8])
                    } else {
                        "at beginning".to_string()
                    };
                    let val_info = if let Some(val) = entry.value {
                        format!("'{}'", val)
                    } else {
                        String::new()
                    };
                    format!("{} UUID {} {}", target_info, &entry.uuid[..8], val_info)
                },
                "delete" => {
                    format!("UUID {}", &entry.uuid[..8])
                },
                _ => String::new()
            };
            
            println!(
                "{} #{} - {} {}",
                "Operation".bright_blue(),
                entry.op_index,
                entry.operation.bright_white(),
                op_detail
            );
        }

        // Parse the new root hash from this operation
        let new_root_bytes = hex::decode(&entry.new_root_hash)
            .context("Failed to decode root hash")?;
        if new_root_bytes.len() != 32 {
            return Err(anyhow!("Invalid root hash length: {}", new_root_bytes.len()));
        }
        let mut new_root = [0u8; 32];
        new_root.copy_from_slice(&new_root_bytes);

        // Verify the proof against the NEW root hash (proof shows state after operation)
        match verify_operation(&entry, &new_root, &grove_version) {
            Ok(()) => {
                stats.verified_proofs += 1;
                if args.verbose {
                    println!("  {} Proof verified", "✓".bright_green());
                }
            }
            Err(e) => {
                stats.failed_proofs += 1;
                println!(
                    "  {} Proof verification failed at operation {}: {}",
                    "✗".bright_red(),
                    entry.op_index,
                    e.to_string().red()
                );
                if args.stop_on_error {
                    return Err(e);
                }
            }
        }

        // Update current root hash for next iteration
        current_root_hash = Some(new_root);

        if args.verbose {
            println!("  Root hash: {}", entry.new_root_hash[..16].dimmed());
            println!();
        }
    }

    // Print summary
    println!();
    println!("{}", "═══════════════════════════════════════════════════════════════".bright_cyan());
    println!("{}", "                         Audit Summary                         ".bright_cyan());
    println!("{}", "═══════════════════════════════════════════════════════════════".bright_cyan());
    println!();
    println!("  Total operations:     {}", stats.total_operations.to_string().bright_white());
    println!("    - Inserts:          {}", stats.inserts.to_string().bright_white());
    println!("    - Deletes:          {}", stats.deletes.to_string().bright_white());
    println!();
    println!(
        "  Verified proofs:      {}",
        format!("{} / {}", stats.verified_proofs, stats.total_operations)
            .bright_green()
    );
    
    if stats.failed_proofs > 0 {
        println!(
            "  Failed proofs:        {}",
            stats.failed_proofs.to_string().bright_red()
        );
    }

    if let Some(root) = current_root_hash {
        println!();
        println!("  Final root hash:      {}", hex::encode(root).bright_white());
    }

    println!();
    if stats.failed_proofs == 0 {
        println!(
            "{} {}",
            "✓".bright_green(),
            "All proofs verified successfully! Server integrity confirmed.".bright_green()
        );
    } else {
        println!(
            "{} {}",
            "✗".bright_red(),
            "Some proofs failed verification. Server may have been tampered with.".bright_red()
        );
        return Err(anyhow!("Audit failed"));
    }
    println!();

    Ok(())
}

fn verify_operation(
    entry: &ChangelogEntry,
    expected_root: &[u8; 32],
    grove_version: &GroveVersion,
) -> Result<()> {
    // Decode the proof from base64
    let proof_bytes = general_purpose::STANDARD
        .decode(&entry.proof)
        .context("Failed to decode proof from base64")?;

    // Parse the UUID
    let uuid = uuid::Uuid::parse_str(&entry.uuid)
        .context("Failed to parse UUID")?;
    let expected_key = uuid.as_bytes().to_vec();

    match entry.operation.as_str() {
        "insert" => {
            // For inserts, use positional proof verification
            verify_insert_operation(&proof_bytes, &expected_key, entry.value, expected_root, grove_version)
        },
        "delete" => {
            // For deletes, use key-based proof verification
            verify_delete_operation(&proof_bytes, &expected_key, expected_root, grove_version)
        },
        _ => Err(anyhow!("Unknown operation type: {}", entry.operation))
    }
}

fn verify_positional_membership(
    proof_bytes: &[u8],
    expected_key: &[u8],
    expected_root: &[u8; 32],
    grove_version: &GroveVersion,
    expected_deleted_flag: u8,
    expected_char: Option<char>,
) -> Result<()> {
    let mut verified = false;
    let mut last_error = None;

    for position in 0..1000 {
        let result = verify_positional_proof(proof_bytes, position, *expected_root, grove_version)
            .value;

        match result {
            Ok(proof_result) if proof_result.key == expected_key => {
                if proof_result.value.len() != 2 {
                    return Err(anyhow!(
                        "Value format mismatch: expected 2 bytes but got {}",
                        proof_result.value.len()
                    ));
                }
                let deleted_flag = proof_result.value[0];
                if deleted_flag != expected_deleted_flag {
                    return Err(anyhow!(
                        "Unexpected deleted flag: expected {} but proof shows {}",
                        expected_deleted_flag,
                        deleted_flag
                    ));
                }
                if let Some(ch) = expected_char {
                    if proof_result.value[1] != ch as u8 {
                        return Err(anyhow!(
                            "Value mismatch: proof contains char '{}' but expected '{}'",
                            proof_result.value[1] as char,
                            ch
                        ));
                    }
                }
                verified = true;
                break;
            }
            Ok(_) => continue,
            Err(e) => {
                last_error = Some(e);
                continue;
            }
        }
    }

    if !verified {
        if let Some(err) = last_error {
            Err(anyhow!("Proof verification failed: {:?}", err))
        } else {
            Err(anyhow!("Could not find UUID at any position in proof"))
        }
    } else {
        Ok(())
    }
}

fn verify_insert_operation(
    proof_bytes: &[u8],
    expected_key: &[u8],
    value: Option<char>,
    expected_root: &[u8; 32],
    grove_version: &GroveVersion,
) -> Result<()> {
    verify_positional_membership(
        proof_bytes,
        expected_key,
        expected_root,
        grove_version,
        0,
        value,
    )
}

fn verify_delete_operation(
    proof_bytes: &[u8],
    expected_key: &[u8],
    expected_root: &[u8; 32],
    grove_version: &GroveVersion,
) -> Result<()> {
    verify_positional_membership(
        proof_bytes,
        expected_key,
        expected_root,
        grove_version,
        1,
        None,
    )
}
