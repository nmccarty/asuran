use std::collections::HashMap;
use std::io::{self, Write};
use std::time::{Duration, Instant};

use asuran::prelude::*;

use anyhow::Result;
use prettytable::{cell, row, Table};

const ONE_MIB: usize = 1_048_576;
const REPETITIONS: usize = 100;

/// Runs each encryption/hmac pair over 1MiB of zeros, 100 times
///
/// Produces output in MiB/s
pub fn bench_with_settings(encryption: Encryption, hmac: HMAC) -> f64 {
    let key = Key::random(encryption.key_length());
    let compression = Compression::NoCompression;
    let bytes = vec![0_u8; ONE_MIB];
    let mut total_duration = Duration::new(0, 0);
    for _ in 0..REPETITIONS {
        // Clone the input
        let x = bytes.clone();
        // Start the timer
        let start = Instant::now();
        // Pack the chunk
        let _ = Chunk::pack(x, compression, encryption, hmac, &key);
        // Stop the timer
        let duration = start.elapsed();
        total_duration += duration;
    }
    let elapsed = total_duration.as_secs_f64();
    // Convert to MiB/s, which is easy, because we are using 1MiB blocks
    (REPETITIONS as f64) / elapsed
}

pub async fn bench_crypto() -> Result<()> {
    // Print the info
    println!(
        "                        === asuran-cli bench-crypto ===
                        
This command will provide benchmarks of the raw single threaded performance of
Encryption and HMAC operations with each of Asuran's supported crypto
primitives.

These benchmarks are *not* the final throughput of asuran. Compression and
chunker settings are likely to have a far greater impact on final throughput
than any of these.

                          === Beginning Benchmarks ===\n"
    );
    // Flush the output before doing anything
    io::stdout().flush()?;

    let mut map: HashMap<Encryption, Vec<(HMAC, f64)>> = HashMap::new();
    let encryptions = vec![Encryption::new_aes256ctr(), Encryption::new_chacha20()];
    let hmacs = vec![
        HMAC::SHA256,
        HMAC::Blake2b,
        HMAC::Blake2bp,
        HMAC::Blake3,
        HMAC::SHA3,
    ];
    for enc in encryptions.clone() {
        let mut results: Vec<(HMAC, f64)> = Vec::new();
        for hmac in &hmacs {
            results.push((*hmac, bench_with_settings(enc.clone(), *hmac)));
            // Print a dot and flush to indicate progress
            print!("*");
            io::stdout().flush()?;
        }
        map.insert(enc, results);
    }
    println!("\n                                === Results ===\n");
    // Make the output table
    let mut table = Table::new();
    table.set_titles(row![
        "       Encryption Type        ",
        "       HMAC Type      ",
        "       Speed      "
    ]);
    for enc in encryptions {
        let mut first = true;
        let results = map.remove(&enc).unwrap();
        for (hmac, result) in results {
            let enc = if first {
                format!("{}", encryption_to_str(&enc))
            } else {
                "".to_string()
            };
            first = false;
            table.add_row(row![
                enc,
                hmac_to_str(hmac).to_string(),
                format!("{:.2} MiB/s", result)
            ]);
        }
    }
    table.printstd();
    println!(
        "\n                              === Authors Note ===

All of the cryptographic primitives that asuran uses are of roughly comparable
cryptographic security, with the possible exception of SHA2. SHA2 is not yet
broken, and the fact that asuran uses an HMAC may or may not mitigate any future
attacks on it.

Simply put, chose the combination that is fastest on your machine, but if a
combination including SHA2 is fastest on your machine, and you do not understand
the above security disclaimer and its implications, you may be well advised to
choose the fastest combination that does not include SHA2."
    );
    Ok(())
}

fn encryption_to_str(encryption: &Encryption) -> &'static str {
    match encryption {
        Encryption::AES256CTR { .. } => "AES256-CTR",
        Encryption::ChaCha20 { .. } => "ChaCha20",
        _ => unimplemented!(),
    }
}

fn hmac_to_str(hmac: HMAC) -> &'static str {
    match hmac {
        HMAC::SHA256 => "SHA2",
        HMAC::Blake2b => "BLAKE2b",
        HMAC::Blake2bp => "BLAKE2bp",
        HMAC::Blake3 => "BLAKE3",
        HMAC::SHA3 => "SHA3",
    }
}
