# Payments Engine

A streaming CSV payments engine that processes deposits, withdrawals, disputes, resolutions,
and chargebacks, then outputs the final state of all client accounts.

## How to run

```
cargo run -- transactions.csv > accounts.csv

Or

cargo run --release -- transactions.csv > accounts.csv
```

## Design decisions
TODO: Add design decisions here.