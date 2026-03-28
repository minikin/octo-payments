use rust_decimal::Decimal;
use serde::Deserialize;

#[non_exhaustive]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransactionType {
    Deposit,
    Withdrawal,
    Dispute,
    Resolve,
    Chargeback,
}

/// A single row from the input CSV.
/// `amount` is Option because dispute/resolve/chargeback rows omit that column.
#[derive(Debug, Deserialize)]
pub struct TransactionRecord {
    #[serde(rename = "type")]
    pub tx_type: TransactionType,
    pub client: u16,
    pub tx: u32,
    pub amount: Option<Decimal>,
}

/// The engine's internal record of a deposit or withdrawal, kept for dispute lookups.
#[derive(Debug, Deserialize)]
pub struct StoredTransaction {
    pub client: u16,
    pub amount: Decimal,
    pub disputed: bool,
}
