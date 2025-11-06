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
    position: usize,
    uuid: Option<String>,
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
    println!("{}", "║         Merk Collaborative Editor - Changelog Auditor        ║".bright_cyan());
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
            println!(
                "{} #{} - {} at position {} {}",
                "Operation".bright_blue(),
                entry.op_index,
                entry.operation.bright_white(),
                entry.position,
                if let Some(val) = entry.value {
                    format!("('{}')", val)
                } else {
                    String::new()
                }
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

    // Verify the positional proof
    // The proof_bytes are already in the correct encoded format
    let result = verify_positional_proof(
        &proof_bytes,
        entry.position as u64,
        *expected_root,
        grove_version,
    )
    .value
    .map_err(|e| anyhow!("Proof verification failed: {:?}", e))?;

    // For insert operations, verify the key and value match
    if entry.operation == "insert" {
        // Verify the key matches the UUID
        if let Some(uuid_str) = &entry.uuid {
            let uuid = uuid::Uuid::parse_str(uuid_str)
                .context("Failed to parse UUID")?;
            let expected_key = uuid.as_bytes().to_vec();
            
            if result.key != expected_key {
                return Err(anyhow!(
                    "Key mismatch: proof contains different key than expected"
                ));
            }
        }

        // Verify the value matches
        if let Some(val) = entry.value {
            let expected_value = vec![val as u8];
            if result.value != expected_value {
                return Err(anyhow!(
                    "Value mismatch: proof contains '{}' but expected '{}'",
                    result.value[0] as char,
                    val
                ));
            }
        }
    }
    
    // For delete operations, the proof shows the state AFTER deletion,
    // so the deleted key won't be in the proof - it will show neighboring keys

    Ok(())
}
