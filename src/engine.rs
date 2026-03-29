use std::collections::HashMap;

use rust_decimal::Decimal;

use crate::{
    account::Account,
    error::PaymentEngineError,
    transaction::{StoredTransaction, TransactionRecord, TransactionType},
};

#[derive(Debug)]
pub struct PaymentEngine {
    accounts: HashMap<u16, Account>,
    /// Stores deposits for dispute/resolve/chargeback lookups.
    transactions: HashMap<u32, StoredTransaction>,
}

impl PaymentEngine {
    #[must_use]
    pub fn new() -> Self {
        Self {
            accounts: HashMap::new(),
            transactions: HashMap::new(),
        }
    }

    /// Process a single CSV row. Returns `Ok(())` for both successful processing
    /// and for silently-ignored invalid inputs (unknown tx, insufficient funds, etc.).
    /// Returns `Err` only when a deposit or withdrawal is missing its required amount field.
    pub fn process(
        &mut self,
        record: TransactionRecord,
    ) -> Result<(), PaymentEngineError> {
        match record.tx_type {
            TransactionType::Deposit => {
                let amount = record.amount.ok_or(PaymentEngineError::MissingAmount(record.tx))?;
                self.apply_deposit(record.client, record.tx, amount);
            },
            TransactionType::Withdrawal => {
                let amount = record.amount.ok_or(PaymentEngineError::MissingAmount(record.tx))?;
                self.apply_withdrawal(record.client, amount);
            },
            TransactionType::Dispute => self.apply_dispute(record.client, record.tx),
            TransactionType::Resolve => self.apply_resolve(record.client, record.tx),
            TransactionType::Chargeback => self.apply_chargeback(record.client, record.tx),
        }

        Ok(())
    }

    #[must_use]
    pub fn into_accounts(self) -> HashMap<u16, Account> {
        self.accounts
    }

    /// Implements the rules for a deposit transaction, including ignoring invalid inputs
    /// and maintaining the invariant that accounts with locked status cannot be mutated.
    fn apply_deposit(
        &mut self,
        client: u16,
        tx: u32,
        amount: Decimal,
    ) {
        if amount <= Decimal::ZERO {
            return;
        }

        let account = self.accounts.entry(client).or_default();
        if account.locked {
            return;
        }

        account.available += amount;
        self.transactions.insert(tx, StoredTransaction::new(client, amount));
    }

    /// Implements the rules for a withdrawal transaction, including ignoring invalid inputs
    /// and maintaining the invariant that accounts with locked status cannot be mutated.
    fn apply_withdrawal(
        &mut self,
        client: u16,
        amount: Decimal,
    ) {
        if amount <= Decimal::ZERO {
            return;
        }

        let Some(account) = self.accounts.get_mut(&client) else {
            return;
        };

        if account.locked || account.available < amount {
            return;
        }

        account.available -= amount;
    }

    /// Implements the rules for a dispute transaction, including ignoring invalid inputs
    /// and maintaining the invariant that accounts with locked status cannot be mutated.
    fn apply_dispute(
        &mut self,
        client: u16,
        tx: u32,
    ) {
        let transactions = &mut self.transactions;
        let accounts = &mut self.accounts;

        let Some(stored) = transactions.get_mut(&tx) else {
            return;
        };
        if stored.client != client || stored.disputed {
            return;
        }

        let account = accounts.entry(client).or_default();
        if account.locked {
            return;
        }

        // available may go negative if funds were subsequently withdrawn
        account.available -= stored.amount;
        account.held += stored.amount;
        stored.disputed = true;
    }

    /// Implements the rules for a resolve transaction, including ignoring invalid inputs
    /// and maintaining the invariant that accounts with locked status cannot be mutated.
    fn apply_resolve(
        &mut self,
        client: u16,
        tx: u32,
    ) {
        let transactions = &mut self.transactions;
        let accounts = &mut self.accounts;

        let Some(stored) = transactions.get_mut(&tx) else {
            return;
        };

        if stored.client != client || !stored.disputed {
            return;
        }

        let account = accounts.entry(client).or_default();
        if account.locked {
            return;
        }

        account.held -= stored.amount;
        account.available += stored.amount;
        stored.disputed = false;
    }

    /// Implements the rules for a chargeback transaction, including ignoring invalid inputs
    /// and maintaining the invariant that accounts with locked status cannot be mutated.
    fn apply_chargeback(
        &mut self,
        client: u16,
        tx: u32,
    ) {
        let transactions = &mut self.transactions;
        let accounts = &mut self.accounts;

        let Some(stored) = transactions.get_mut(&tx) else {
            return;
        };

        if stored.client != client || !stored.disputed {
            return;
        }

        // chargebacks are not gated on locked status
        let account = accounts.entry(client).or_default();
        account.held -= stored.amount;
        account.locked = true;
        stored.disputed = false
    }
}

impl Default for PaymentEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use rust_decimal_macros::dec;

    use super::*;

    fn engine_with(csv: &str) -> HashMap<u16, Account> {
        let mut reader = csv::ReaderBuilder::new()
            .trim(csv::Trim::All)
            .flexible(true)
            .from_reader(csv.as_bytes());
        let mut engine = PaymentEngine::new();
        for result in reader.deserialize::<TransactionRecord>() {
            let record = result.unwrap();
            let _ = engine.process(record);
        }
        engine.into_accounts()
    }

    #[test]
    fn deposit_credits_available() {
        let accounts = engine_with("type,client,tx,amount\ndeposit,1,1,1.0");
        assert_eq!(accounts[&1].available, dec!(1.0));
        assert_eq!(accounts[&1].held, dec!(0));
    }

    #[test]
    fn deposit_creates_account_if_not_exists() {
        let accounts = engine_with("type,client,tx,amount\ndeposit,99,1,5.0");
        assert!(accounts.contains_key(&99));
    }

    #[test]
    fn deposit_negative_amount_is_ignored() {
        let accounts = engine_with("type,client,tx,amount\ndeposit,1,1,-5.0");
        assert_eq!(accounts.get(&1).map(|a| a.available), None);
    }

    #[test]
    fn deposit_zero_amount_is_ignored() {
        let accounts = engine_with("type,client,tx,amount\ndeposit,1,1,0.0");
        assert_eq!(accounts.get(&1).map(|a| a.available), None);
    }

    #[test]
    fn withdrawal_debits_available() {
        let accounts = engine_with("type,client,tx,amount\ndeposit,1,1,2.0\nwithdrawal,1,2,1.5");
        assert_eq!(accounts[&1].available, dec!(0.5));
    }

    #[test]
    fn withdrawal_insufficient_funds_is_ignored() {
        let accounts = engine_with("type,client,tx,amount\ndeposit,1,1,1.0\nwithdrawal,1,2,2.0");
        assert_eq!(accounts[&1].available, dec!(1.0));
    }

    #[test]
    fn withdrawal_negative_amount_is_ignored() {
        let accounts = engine_with("type,client,tx,amount\ndeposit,1,1,5.0\nwithdrawal,1,2,-1.0");
        assert_eq!(accounts[&1].available, dec!(5.0));
    }

    #[test]
    fn withdrawal_exact_funds_succeeds() {
        let accounts = engine_with("type,client,tx,amount\ndeposit,1,1,1.0\nwithdrawal,1,2,1.0");
        assert_eq!(accounts[&1].available, dec!(0));
    }

    #[test]
    fn dispute_moves_funds_available_to_held() {
        let accounts = engine_with("type,client,tx,amount\ndeposit,1,1,2.0\ndispute,1,1,");
        assert_eq!(accounts[&1].available, dec!(0));
        assert_eq!(accounts[&1].held, dec!(2.0));
        assert_eq!(accounts[&1].total(), dec!(2.0));
    }

    #[test]
    fn dispute_unknown_tx_is_ignored() {
        let accounts = engine_with("type,client,tx,amount\ndeposit,1,1,2.0\ndispute,1,99,");
        assert_eq!(accounts[&1].available, dec!(2.0));
        assert_eq!(accounts[&1].held, dec!(0));
    }

    #[test]
    fn dispute_already_disputed_is_ignored() {
        let accounts = engine_with("type,client,tx,amount\ndeposit,1,1,2.0\ndispute,1,1,\ndispute,1,1,");
        assert_eq!(accounts[&1].available, dec!(0));
        assert_eq!(accounts[&1].held, dec!(2.0));
    }

    #[test]
    fn resolve_releases_held_to_available() {
        let accounts = engine_with("type,client,tx,amount\ndeposit,1,1,2.0\ndispute,1,1,\nresolve,1,1,");
        assert_eq!(accounts[&1].available, dec!(2.0));
        assert_eq!(accounts[&1].held, dec!(0));
    }

    #[test]
    fn resolve_non_disputed_tx_is_ignored() {
        let accounts = engine_with("type,client,tx,amount\ndeposit,1,1,2.0\nresolve,1,1,");
        assert_eq!(accounts[&1].available, dec!(2.0));
        assert_eq!(accounts[&1].held, dec!(0));
    }

    #[test]
    fn resolve_unknown_tx_is_ignored() {
        let accounts = engine_with("type,client,tx,amount\ndeposit,1,1,2.0\nresolve,1,99,");
        assert_eq!(accounts[&1].available, dec!(2.0));
    }

    #[test]
    fn chargeback_decrements_held_and_total() {
        let accounts = engine_with("type,client,tx,amount\ndeposit,1,1,2.0\ndispute,1,1,\nchargeback,1,1,");
        assert_eq!(accounts[&1].held, dec!(0));
        assert_eq!(accounts[&1].total(), dec!(0));
    }

    #[test]
    fn chargeback_locks_account() {
        let accounts = engine_with("type,client,tx,amount\ndeposit,1,1,2.0\ndispute,1,1,\nchargeback,1,1,");
        assert!(accounts[&1].locked);
    }

    #[test]
    fn chargeback_non_disputed_tx_is_ignored() {
        let accounts = engine_with("type,client,tx,amount\ndeposit,1,1,2.0\nchargeback,1,1,");
        assert!(!accounts[&1].locked);
        assert_eq!(accounts[&1].available, dec!(2.0));
    }

    #[test]
    fn double_chargeback_is_prevented() {
        let accounts =
            engine_with("type,client,tx,amount\ndeposit,1,1,2.0\ndispute,1,1,\nchargeback,1,1,\nchargeback,1,1,");
        assert_eq!(accounts[&1].held, dec!(0));
        assert_eq!(accounts[&1].total(), dec!(0));
    }

    #[test]
    fn locked_account_ignores_deposit() {
        let accounts =
            engine_with("type,client,tx,amount\ndeposit,1,1,2.0\ndispute,1,1,\nchargeback,1,1,\ndeposit,1,2,5.0");
        assert_eq!(accounts[&1].available, dec!(0));
    }

    #[test]
    fn locked_account_ignores_withdrawal() {
        let accounts =
            engine_with("type,client,tx,amount\ndeposit,1,1,5.0\ndispute,1,1,\nchargeback,1,1,\nwithdrawal,1,2,1.0");
        assert_eq!(accounts[&1].available, dec!(0));
    }

    #[test]
    fn no_floating_point_drift() {
        let accounts = engine_with("type,client,tx,amount\ndeposit,1,1,0.1\ndeposit,1,2,0.2");
        assert_eq!(accounts[&1].available, dec!(0.3));
    }

    #[test]
    fn four_decimal_precision_preserved() {
        let accounts = engine_with("type,client,tx,amount\ndeposit,1,1,9999.9999");
        assert_eq!(accounts[&1].available, dec!(9999.9999));
    }
}
