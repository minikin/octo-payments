use std::collections::HashMap;

use octo_payments::{account::Account, engine::PaymentEngine, transaction::TransactionRecord};
use rust_decimal_macros::dec;

fn run_engine(csv: &str) -> HashMap<u16, Account> {
    let mut reader = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .flexible(true)
        .from_reader(csv.as_bytes());
    let mut engine = PaymentEngine::new();
    for result in reader.deserialize::<TransactionRecord>() {
        let record = result.expect("malformed test CSV");
        let _ = engine.process(record);
    }
    engine.into_accounts()
}

/// The exact example from the challenge spec.
#[test]
fn spec_example() {
    let accounts = run_engine(
        "type, client, tx, amount
            deposit,    1, 1, 1.0
            deposit,    2, 2, 2.0
            deposit,    1, 3, 2.0
            withdrawal, 1, 4, 1.5
            withdrawal, 2, 5, 3.0",
    );

    let c1 = &accounts[&1];
    assert_eq!(c1.available, dec!(1.5));
    assert_eq!(c1.held, dec!(0));
    assert_eq!(c1.total(), dec!(1.5));
    assert!(!c1.locked);

    let c2 = &accounts[&2];
    assert_eq!(c2.available, dec!(2.0));
    assert_eq!(c2.held, dec!(0));
    assert_eq!(c2.total(), dec!(2.0));
    assert!(!c2.locked);
}

/// Full dispute → resolve lifecycle: account returns to original state.
#[test]
fn dispute_resolve_lifecycle() {
    let accounts = run_engine(
        "type,client,tx,amount
            deposit,1,1,5.0
            dispute,1,1,
            resolve,1,1,",
    );
    let c1 = &accounts[&1];
    assert_eq!(c1.available, dec!(5.0));
    assert_eq!(c1.held, dec!(0));
    assert!(!c1.locked);
}

/// Full dispute → chargeback lifecycle: account locked, total reduced.
#[test]
fn dispute_chargeback_lifecycle() {
    let accounts = run_engine(
        "type,client,tx,amount
            deposit,1,1,5.0
            dispute,1,1,
            chargeback,1,1,",
    );
    let c1 = &accounts[&1];
    assert_eq!(c1.available, dec!(0));
    assert_eq!(c1.held, dec!(0));
    assert_eq!(c1.total(), dec!(0));
    assert!(c1.locked);
}

/// Disputing the same transaction twice: second dispute is a no-op.
#[test]
fn double_dispute_prevention() {
    let accounts = run_engine(
        "type,client,tx,amount
            deposit,1,1,3.0
            dispute,1,1,
            dispute,1,1,",
    );
    let c1 = &accounts[&1];
    assert_eq!(c1.available, dec!(0));
    assert_eq!(c1.held, dec!(3.0));
    assert_eq!(c1.total(), dec!(3.0));
}

/// Chargeback without a prior dispute is silently ignored.
#[test]
fn chargeback_without_prior_dispute() {
    let accounts = run_engine(
        "type,client,tx,amount
            deposit,1,1,5.0
            chargeback,1,1,",
    );
    let c1 = &accounts[&1];
    assert_eq!(c1.available, dec!(5.0));
    assert!(!c1.locked);
}

/// After a chargeback, further deposits on the locked account are ignored.
#[test]
fn locked_account_rejects_deposits() {
    let accounts = run_engine(
        "type,client,tx,amount
            deposit,1,1,5.0
            dispute,1,1,
            chargeback,1,1,
            deposit,1,2,100.0",
    );
    let c1 = &accounts[&1];
    assert_eq!(c1.total(), dec!(0));
    assert!(c1.locked);
}

/// CSV with spaces after every comma (as shown in the spec example).
#[test]
fn whitespace_heavy_csv() {
    let accounts = run_engine(
        "type, client, tx, amount
            deposit, 1, 1, 2.5000
            withdrawal, 1, 2, 0.5000",
    );
    assert_eq!(accounts[&1].available, dec!(2.0));
}

/// Four decimal place values round-trip without loss.
#[test]
fn four_decimal_precision_roundtrip() {
    let accounts = run_engine(
        "type,client,tx,amount
            deposit,1,1,0.0001
            deposit,1,2,9999.9998",
    );
    assert_eq!(accounts[&1].available, dec!(9999.9999));
}

/// Multiple clients with interleaved transactions remain independent.
#[test]
fn multiple_clients_are_independent() {
    let accounts = run_engine(
        "type,client,tx,amount
            deposit,1,1,10.0
            deposit,2,2,20.0
            withdrawal,1,3,3.0
            dispute,2,2,",
    );

    let c1 = &accounts[&1];
    assert_eq!(c1.available, dec!(7.0));
    assert_eq!(c1.held, dec!(0));

    let c2 = &accounts[&2];
    assert_eq!(c2.available, dec!(0));
    assert_eq!(c2.held, dec!(20.0));
    assert_eq!(c2.total(), dec!(20.0));
}

/// A client not in a transaction still does not appear in output.
#[test]
fn unknown_client_not_created_by_dispute() {
    let accounts = run_engine(
        "type,client,tx,amount
            deposit,1,1,5.0
            dispute,2,1,", // client 2 disputes client 1's tx — cross-client guard rejects this
    );
    // Client 1's funds must be unchanged.
    assert_eq!(accounts[&1].available, dec!(5.0));
    assert_eq!(accounts[&1].held, dec!(0));
}
