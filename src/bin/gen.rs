/// Generates a synthetic payments CSV for benchmarking.
/// Usage: gen <num_records> [num_clients]
use std::{
    env,
    io::{self, BufWriter, Write},
};

fn main() {
    let n: u64 = env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(1_000_000);
    let num_clients: u64 = env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(10_000);

    let out = io::stdout();
    let mut w = BufWriter::with_capacity(1 << 20, out.lock()); // 1 MiB write buffer

    writeln!(w, "type,client,tx,amount").unwrap();

    // Wyatt-style xorshift64 — fast, zero dependencies.
    let mut rng: u64 = 0xdeadbeef_cafebabe;
    let mut next = move || -> u64 {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        rng
    };

    for tx in 1u64..=n {
        let client = (next() % num_clients) + 1;
        let kind = next() % 100;

        if kind < 60 {
            // deposit: amount in range [10.0000, 999.9999]
            let units = (next() % 990) + 10;
            let frac = next() % 10_000;
            writeln!(w, "deposit,{client},{tx},{units}.{frac:04}").unwrap();
        } else {
            // withdrawal: amount in range [1.0000, 99.9999]
            let units = (next() % 99) + 1;
            let frac = next() % 10_000;
            writeln!(w, "withdrawal,{client},{tx},{units}.{frac:04}").unwrap();
        }
    }
}
