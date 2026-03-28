use std::collections::HashMap;

use rust_decimal::Decimal;

use crate::{
    account::Account,
    error::PaymentEngineError,
    transaction::{StoredTransaction, TransactionRecord, TransactionType},
};

#[derive(Debug)]
pub struct PaymentEngine {
    pub accounts: HashMap<u16, Account>,

    pub transactions: HashMap<u32, StoredTransaction>,
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
                self.apply_withdrawal(record.client, record.tx, amount);
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

    fn apply_withdrawal(
        &mut self,
        client: u16,
        tx: u32,
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
        self.transactions.insert(tx, StoredTransaction::new(client, amount));
    }

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

        let account = accounts.entry(client).or_default();
        if account.locked {
            return;
        }

        account.available -= stored.amount;
        account.held += stored.amount;
        stored.disputed = true;
    }

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

        let account = accounts.entry(client).or_default();
        if account.locked {
            return;
        }

        account.held -= stored.amount;
        account.available += stored.amount;
        stored.disputed = false;
    }

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
