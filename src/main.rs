use std::env::args;

use csv::{ReaderBuilder, Trim::All, Writer};
use octo_payments::{
    account::AccountRecord, engine::PaymentEngine, error::PaymentEngineError, transaction::TransactionRecord,
};

fn main() -> Result<(), PaymentEngineError> {
    let path = args().nth(1).expect("Usage: <transactions.csv>");

    let mut reader = ReaderBuilder::new().trim(All).flexible(true).from_path(&path)?;

    let mut payment_engine = PaymentEngine::new();

    for result in reader.deserialize::<TransactionRecord>() {
        match result {
            Ok(record) => {
                if let Err(e) = payment_engine.process(record) {
                    eprintln!("warn: {e}");
                }
            },
            Err(e) => eprintln!("warn: skipping malformed row: {e}"),
        }
    }

    let mut writer = Writer::from_writer(std::io::stdout());

    let mut accounts: Vec<_> = payment_engine.into_accounts().into_iter().collect();
    accounts.sort_by_key(|(client, _)| *client);

    for (client, account) in &accounts {
        writer.serialize(AccountRecord::from_account(*client, account))?;
    }

    let _ = writer.flush();

    Ok(())
}
