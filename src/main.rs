use std::{
    env::args,
    fs::File,
    io::{BufRead, BufReader, Read, Seek, SeekFrom},
    path::Path,
    sync::mpsc,
    thread,
};

use csv::{ReaderBuilder, Trim, Writer};
use octo_payments::{
    account::AccountRecord, engine::PaymentEngine, error::PaymentEngineError,
    transaction::TransactionRecord,
};

/// Splits the file into `num_chunks` byte ranges, each aligned to a line boundary.
/// Returns the raw header bytes (to be prepended to every chunk) and a list of
/// `(start, end)` byte offsets covering the data after the header.
fn chunk_boundaries(
    path: &Path,
    num_chunks: usize,
) -> std::io::Result<(Vec<u8>, Vec<(u64, u64)>)> {
    let file_len = std::fs::metadata(path)?.len();
    let mut reader = BufReader::new(File::open(path)?);

    // Consume the header line so we know where data begins.
    let mut header = Vec::new();
    reader.read_until(b'\n', &mut header)?;
    let header_end = header.len() as u64;

    if file_len <= header_end {
        return Ok((header, vec![]));
    }

    let data_len = file_len - header_end;
    let chunk_size = (data_len / num_chunks as u64).max(1);
    let mut boundaries = Vec::with_capacity(num_chunks);
    let mut start = header_end;

    for i in 0..num_chunks {
        if start >= file_len {
            break;
        }
        let end = if i == num_chunks - 1 {
            file_len
        } else {
            let nominal = start + chunk_size;
            if nominal >= file_len {
                file_len
            } else {
                // Advance to the end of the line that straddles the nominal boundary
                // so the split always falls on a complete row.
                reader.seek(SeekFrom::Start(nominal))?;
                let mut tail = Vec::new();
                reader.read_until(b'\n', &mut tail)?;
                nominal + tail.len() as u64
            }
        };
        boundaries.push((start, end));
        start = end;
    }

    Ok((header, boundaries))
}

fn main() -> Result<(), PaymentEngineError> {
    let Some(path) = args().nth(1) else {
        eprintln!("Usage: octo-payments <transactions.csv> [workers]");
        std::process::exit(1);
    };

    let num_workers = args()
        .nth(2)
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|&n| n > 0)
        .unwrap_or_else(|| thread::available_parallelism().map(|n| n.get()).unwrap_or(1));

    let (header, boundaries) = chunk_boundaries(Path::new(&path), num_workers)?;

    // Phase 1 — parse chunks in parallel.
    // Each thread opens the file independently, seeks to its byte range, prepends
    // the saved header so the CSV reader can match column names, then parses its slice.
    let parse_handles: Vec<_> = boundaries
        .into_iter()
        .map(|(start, end)| {
            let path = path.clone();
            let header = header.clone();
            thread::spawn(move || -> Vec<TransactionRecord> {
                let mut file = File::open(&path).expect("failed to open file in parser thread");
                file.seek(SeekFrom::Start(start)).expect("seek failed");
                let data = header.as_slice().chain(file.take(end - start));
                ReaderBuilder::new()
                    .trim(Trim::All)
                    .flexible(true)
                    .from_reader(data)
                    .deserialize::<TransactionRecord>()
                    .filter_map(|r| {
                        r.map_err(|e| eprintln!("warn: skipping malformed row: {e}")).ok()
                    })
                    .collect()
            })
        })
        .collect();

    // Collect in chunk order — chunks cover sequential file ranges, so concatenating
    // them in order preserves the original transaction ordering within each client.
    let chunks: Vec<Vec<TransactionRecord>> = parse_handles
        .into_iter()
        .map(|h| h.join().unwrap_or_default())
        .collect();

    // Phase 2 — route to per-client-shard workers and process in parallel.
    // Workers are identical to the previous design; the only change is that records
    // now arrive via in-order Vecs rather than directly from the CSV reader.
    let (senders, receivers): (Vec<_>, Vec<_>) = (0..num_workers)
        .map(|_| mpsc::channel::<TransactionRecord>())
        .unzip();

    let process_handles: Vec<_> = receivers
        .into_iter()
        .map(|rx| {
            thread::spawn(move || {
                let mut engine = PaymentEngine::new();
                for record in rx {
                    if let Err(e) = engine.process(record) {
                        eprintln!("warn: {e}");
                    }
                }
                engine.into_accounts()
            })
        })
        .collect();

    // Route chunk[0] first, then chunk[1], … to preserve file ordering.
    for chunk in chunks {
        for record in chunk {
            let idx = (record.client as usize) % num_workers;
            let _ = senders[idx].send(record);
        }
    }
    drop(senders);

    let mut accounts: Vec<_> = process_handles
        .into_iter()
        .flat_map(|h| h.join().unwrap_or_default())
        .collect();

    accounts.sort_by_key(|(client, _)| *client);

    let mut writer = Writer::from_writer(std::io::stdout());
    for (client, account) in &accounts {
        writer.serialize(AccountRecord::from_account(*client, account))?;
    }
    let _ = writer.flush();

    Ok(())
}
