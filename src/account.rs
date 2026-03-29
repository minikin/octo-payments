use rust_decimal::Decimal;
use serde::Serialize;

/// Account state for a client.
#[derive(Debug, Default)]
pub struct Account {
    pub available: Decimal,
    pub held: Decimal,
    pub locked: bool,
}

impl Account {
    /// Total is always computed to avoid maintaining an invariant across mutations.
    #[must_use]
    pub fn total(&self) -> Decimal {
        self.available + self.held
    }
}

/// Output shape for the CSV writer. Uses String fields for decimal values so we
/// can enforce exactly 4 decimal places in `format_decimal`.
#[derive(Debug, Serialize)]
pub struct AccountRecord {
    pub client: u16,
    pub available: String,
    pub held: String,
    pub total: String,
    pub locked: bool,
}

impl AccountRecord {
    #[must_use]
    pub fn from_account(
        client: u16,
        account: &Account,
    ) -> Self {
        Self {
            client,
            available: format_decimal(account.available),
            held: format_decimal(account.held),
            total: format_decimal(account.total()),
            locked: account.locked,
        }
    }
}

#[must_use]
pub fn format_decimal(d: Decimal) -> String {
    format!("{d:.4}")
}

#[cfg(test)]
mod tests {
    use rust_decimal_macros::dec;

    use super::*;

    #[test]
    fn format_integer_to_four_dp() {
        assert_eq!(format_decimal(dec!(1)), "1.0000");
        assert_eq!(format_decimal(dec!(0)), "0.0000");
    }

    #[test]
    fn format_partial_dp_pads_to_four() {
        assert_eq!(format_decimal(dec!(1.5)), "1.5000");
        assert_eq!(format_decimal(dec!(1.23)), "1.2300");
        assert_eq!(format_decimal(dec!(1.234)), "1.2340");
    }

    #[test]
    fn format_full_four_dp() {
        assert_eq!(format_decimal(dec!(9999.9999)), "9999.9999");
        assert_eq!(format_decimal(dec!(0.0001)), "0.0001");
    }

    #[test]
    fn total_is_sum_of_available_and_held() {
        let account = Account {
            available: dec!(1.5),
            held: dec!(0.5),
            locked: false,
        };
        assert_eq!(account.total(), dec!(2.0));
    }
}
