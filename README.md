# Payments Engine

A streaming CSV payments engine that processes deposits, withdrawals, disputes, resolutions,
and chargebacks, then outputs the final state of all client accounts.

- [Payments Engine](#payments-engine)
  - [How to run](#how-to-run)
  - [How to test](#how-to-test)
  - [Design decisions](#design-decisions)

## How to run

```
cargo run -- transactions.csv > accounts.csv

Or

cargo run --release -- transactions.csv > accounts.csv
```

## How to test

```
cargo test
```

## Design decisions

**`rust_decimal` instead of `f64`**
Financial amounts require exact decimal arithmetic. `f64` cannot represent `0.1` exactly,
so `0.1 + 0.2 != 0.3` in floating point. `rust_decimal` stores values as an integer
significand with a base-10 exponent, giving exact results up to 28 significant digits —
no drift at four decimal places.

**Streaming, not load-all**
The CSV reader processes rows one at a time via an iterator.
Only two maps live in memory:`accounts` and `transactions`.

**`total()` is computed, not stored**
`total = available + held` always. Storing all three would require maintaining that
invariant across every mutation. Computing it eliminates that class of bug entirely.

**Separation of deserialization shape and engine state**
`TransactionRecord` mirrors the CSV row and is consumed immediately.
`StoredTransaction` is the engine's internal record. Keeping them separate means serde machinery never leaks
into engine logic.