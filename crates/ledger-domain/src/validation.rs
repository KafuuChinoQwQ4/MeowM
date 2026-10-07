//! Validation of transaction and posting invariants.

use crate::{Account, AccountKind, DomainError, Transaction, TransactionKind};

/// The contribution a confirmed transaction makes to aggregate reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReportEffect {
    pub income_cny_minor: i64,
    pub expense_cny_minor: i64,
}

/// Validate account references and the structural invariants of a transaction.
///
/// This deliberately does not resolve a refund's parent transaction. The application
/// layer owns that lookup because the domain function receives only the current
/// transaction and the account catalogue.
pub fn validate_transaction_shape(
    transaction: &Transaction,
    accounts: &[Account],
) -> Result<(), DomainError> {
    if transaction.kind.requires_original_transaction() {
        match transaction.original_transaction_id {
            None => return Err(DomainError::RefundOriginalRequired),
            Some(original_id) if original_id == transaction.id => {
                return Err(DomainError::SelfReferentialRefund);
            }
            Some(_) => {}
        }
    }

    if transaction.postings.is_empty() {
        return if transaction.status.allows_empty_postings() {
            Ok(())
        } else {
            Err(DomainError::MissingPosting)
        };
    }

    let posting_accounts = transaction
        .postings
        .iter()
        .map(|posting| {
            let account = accounts
                .iter()
                .find(|account| account.id == posting.account_id)
                .ok_or(DomainError::PostingAccountNotFound)?;
            if !account.currency_matches(&posting.delta) {
                return Err(DomainError::CurrencyMismatch);
            }
            Ok(account)
        })
        .collect::<Result<Vec<_>, DomainError>>()?;

    for (posting, account) in transaction.postings.iter().zip(&posting_accounts) {
        if !is_direction_valid(transaction.kind, account.kind, posting.delta.minor_units()) {
            return Err(DomainError::InvalidPostingDirection);
        }
    }

    if transaction.kind.requires_two_distinct_postings()
        && (transaction.postings.len() != 2
            || transaction.postings[0].account_id == transaction.postings[1].account_id)
    {
        return Err(DomainError::InvalidTransferPostings);
    }

    if matches!(
        transaction.kind,
        TransactionKind::Income | TransactionKind::Expense | TransactionKind::Refund
    ) && transaction
        .report_amount_cny
        .as_ref()
        .is_some_and(|amount| amount.minor_units() < 0)
    {
        return Err(DomainError::NegativeReportAmount);
    }

    if transaction.kind == TransactionKind::Transfer
        && transaction.postings.len() == 2
        && posting_accounts.len() == 2
    {
        let first_posting = &transaction.postings[0];
        let second_posting = &transaction.postings[1];
        if first_posting.delta.currency() != second_posting.delta.currency() {
            return Err(DomainError::CurrencyMismatch);
        }

        let first = first_posting.delta.minor_units();
        let second = second_posting.delta.minor_units();
        let same_balance_kind = posting_accounts[0].kind == posting_accounts[1].kind;
        // Transfers between accounts with the same normal-balance kind move in
        // opposite directions. Asset/liability transfers move together: a card
        // repayment lowers both balances, while a cash advance raises both.
        if first == 0 || second == 0 || (same_balance_kind == (first.signum() == second.signum())) {
            return Err(DomainError::InvalidPostingDirection);
        }
        if first.unsigned_abs() != second.unsigned_abs() {
            return Err(DomainError::UnbalancedTransfer);
        }
    }

    Ok(())
}

/// Assets are positive when owned; liabilities are positive when owed.
fn is_direction_valid(kind: TransactionKind, account_kind: AccountKind, delta: i64) -> bool {
    if delta == 0 {
        return false;
    }
    match (kind, account_kind) {
        (TransactionKind::Income, AccountKind::Asset)
        | (TransactionKind::Refund, AccountKind::Asset)
        | (TransactionKind::Expense, AccountKind::Liability) => delta > 0,
        (TransactionKind::Income, AccountKind::Liability)
        | (TransactionKind::Refund, AccountKind::Liability)
        | (TransactionKind::Expense, AccountKind::Asset) => delta < 0,
        (TransactionKind::BalanceAdjustment, _) => true,
        (TransactionKind::Transfer, _) => true,
    }
}

/// Return the confirmed transaction's contribution to CNY income and expense totals.
pub fn report_effect(transaction: &Transaction) -> Option<ReportEffect> {
    if !transaction.status.is_effective() {
        return None;
    }
    let amount = transaction.report_amount_cny.as_ref()?;
    if amount.currency().code() != "CNY" || amount.minor_units() < 0 {
        return None;
    }

    let amount = amount.minor_units();
    match transaction.kind {
        TransactionKind::Income => Some(ReportEffect {
            income_cny_minor: amount,
            expense_cny_minor: 0,
        }),
        TransactionKind::Expense => Some(ReportEffect {
            income_cny_minor: 0,
            expense_cny_minor: amount,
        }),
        TransactionKind::Refund => Some(ReportEffect {
            income_cny_minor: 0,
            expense_cny_minor: amount.checked_neg()?,
        }),
        TransactionKind::Transfer | TransactionKind::BalanceAdjustment => None,
    }
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};

    use crate::{
        Account, AccountId, AccountKind, CurrencyCode, Money, Posting, Transaction, TransactionId,
        TransactionKind, TransactionStatus, report_effect, validate_transaction_shape,
    };

    fn currency(code: &str) -> CurrencyCode {
        CurrencyCode::new(code, if code == "JPY" { 0 } else { 2 }).unwrap()
    }

    fn timestamp() -> DateTime<Utc> {
        DateTime::<Utc>::from_timestamp(0, 0).unwrap()
    }

    fn asset_account(name: &str, currency_code: &str) -> Account {
        let currency = currency(currency_code);
        Account {
            id: AccountId::new(),
            display_name: name.to_owned(),
            kind: AccountKind::Asset,
            currency: currency.clone(),
            opening_balance: Money::new(0, currency),
            opening_balance_effective_at: timestamp(),
        }
    }

    fn liability_account(name: &str, currency_code: &str) -> Account {
        let currency = currency(currency_code);
        Account {
            id: AccountId::new(),
            display_name: name.to_owned(),
            kind: AccountKind::Liability,
            currency: currency.clone(),
            opening_balance: Money::new(0, currency),
            opening_balance_effective_at: timestamp(),
        }
    }

    fn report_amount(minor_units: i64) -> Money {
        Money::new(minor_units, currency("CNY"))
    }

    fn confirmed_expense(account: Account, minor_units: i64) -> Transaction {
        Transaction {
            id: TransactionId::new(),
            occurred_at: timestamp(),
            kind: TransactionKind::Expense,
            status: TransactionStatus::Confirmed,
            postings: vec![Posting {
                account_id: account.id,
                delta: Money::new(-minor_units, account.currency.clone()),
            }],
            category_id: None,
            original_transaction_id: None,
            report_amount_cny: Some(report_amount(minor_units)),
            merchant: None,
            description: None,
        }
    }

    fn confirmed_transfer(from: Account, to: Account, minor_units: i64) -> Transaction {
        Transaction {
            id: TransactionId::new(),
            occurred_at: timestamp(),
            kind: TransactionKind::Transfer,
            status: TransactionStatus::Confirmed,
            postings: vec![
                Posting {
                    account_id: from.id,
                    delta: Money::new(-minor_units, from.currency.clone()),
                },
                Posting {
                    account_id: to.id,
                    delta: Money::new(minor_units, to.currency.clone()),
                },
            ],
            category_id: None,
            original_transaction_id: None,
            report_amount_cny: None,
            merchant: None,
            description: None,
        }
    }

    #[test]
    fn asset_expense_has_negative_asset_posting_and_positive_expense_effect() {
        let wallet = asset_account("wallet", "CNY");
        let expense = confirmed_expense(wallet.clone(), 2500);
        validate_transaction_shape(&expense, &[wallet]).unwrap();
        assert_eq!(expense.postings[0].delta.minor_units(), -2500);
        assert_eq!(report_effect(&expense).unwrap().expense_cny_minor, 2500);
    }

    #[test]
    fn transfer_rejects_unequal_amounts_in_the_same_currency() {
        let cash = asset_account("cash", "CNY");
        let bank = asset_account("bank", "CNY");
        let mut transfer = confirmed_transfer(cash.clone(), bank.clone(), 5_000);
        transfer.postings[1].delta = Money::new(4_900, bank.currency.clone());

        assert_eq!(
            validate_transaction_shape(&transfer, &[cash, bank]),
            Err(crate::DomainError::UnbalancedTransfer),
        );
    }

    #[test]
    fn transfer_rejects_different_posting_currencies_without_fx_semantics() {
        let cash = asset_account("cash", "CNY");
        let dollar_account = asset_account("dollar account", "USD");
        let transfer = confirmed_transfer(cash.clone(), dollar_account.clone(), 5_000);

        assert_eq!(
            validate_transaction_shape(&transfer, &[cash, dollar_account]),
            Err(crate::DomainError::CurrencyMismatch),
        );
    }

    #[test]
    fn same_currency_liability_transfer_and_cash_advance_are_valid() {
        let first_card = liability_account("first card", "CNY");
        let second_card = liability_account("second card", "CNY");
        let liability_transfer = Transaction {
            id: TransactionId::new(),
            occurred_at: timestamp(),
            kind: TransactionKind::Transfer,
            status: TransactionStatus::Confirmed,
            postings: vec![
                Posting {
                    account_id: first_card.id,
                    delta: Money::new(-2_000, first_card.currency.clone()),
                },
                Posting {
                    account_id: second_card.id,
                    delta: Money::new(2_000, second_card.currency.clone()),
                },
            ],
            category_id: None,
            original_transaction_id: None,
            report_amount_cny: None,
            merchant: None,
            description: None,
        };
        validate_transaction_shape(&liability_transfer, &[first_card, second_card]).unwrap();

        let wallet = asset_account("wallet", "CNY");
        let card = liability_account("card", "CNY");
        let cash_advance = Transaction {
            id: TransactionId::new(),
            occurred_at: timestamp(),
            kind: TransactionKind::Transfer,
            status: TransactionStatus::Confirmed,
            postings: vec![
                Posting {
                    account_id: wallet.id,
                    delta: Money::new(2_000, wallet.currency.clone()),
                },
                Posting {
                    account_id: card.id,
                    delta: Money::new(2_000, card.currency.clone()),
                },
            ],
            category_id: None,
            original_transaction_id: None,
            report_amount_cny: None,
            merchant: None,
            description: None,
        };
        validate_transaction_shape(&cash_advance, &[wallet, card]).unwrap();
    }

    #[test]
    fn transfer_has_two_postings_and_no_report_effect() {
        let cash = asset_account("cash", "CNY");
        let bank = asset_account("bank", "CNY");
        let transfer = confirmed_transfer(cash.clone(), bank.clone(), 5000);
        validate_transaction_shape(&transfer, &[cash, bank]).unwrap();
        assert_eq!(transfer.postings.len(), 2);
        assert_eq!(report_effect(&transfer), None);
    }

    #[test]
    fn card_purchase_increases_liability_and_counts_as_expense() {
        let card = liability_account("credit card", "CNY");
        let purchase = Transaction {
            id: TransactionId::new(),
            occurred_at: timestamp(),
            kind: TransactionKind::Expense,
            status: TransactionStatus::Confirmed,
            postings: vec![Posting {
                account_id: card.id,
                delta: Money::new(8_000, card.currency.clone()),
            }],
            category_id: None,
            original_transaction_id: None,
            report_amount_cny: Some(report_amount(8_000)),
            merchant: None,
            description: None,
        };
        validate_transaction_shape(&purchase, &[card]).unwrap();
        assert_eq!(purchase.postings[0].delta.minor_units(), 8_000);
        assert_eq!(report_effect(&purchase).unwrap().expense_cny_minor, 8_000);
    }

    #[test]
    fn card_repayment_is_a_transfer_and_has_no_expense_effect() {
        let wallet = asset_account("wallet", "CNY");
        let card = liability_account("credit card", "CNY");
        let repayment = Transaction {
            id: TransactionId::new(),
            occurred_at: timestamp(),
            kind: TransactionKind::Transfer,
            status: TransactionStatus::Confirmed,
            postings: vec![
                Posting {
                    account_id: wallet.id,
                    delta: Money::new(-3_000, wallet.currency.clone()),
                },
                Posting {
                    account_id: card.id,
                    delta: Money::new(-3_000, card.currency.clone()),
                },
            ],
            category_id: None,
            original_transaction_id: None,
            report_amount_cny: None,
            merchant: None,
            description: None,
        };
        validate_transaction_shape(&repayment, &[wallet, card]).unwrap();
        assert_eq!(repayment.postings[1].delta.minor_units(), -3_000_i64);
        assert_eq!(report_effect(&repayment), None);
    }

    #[test]
    fn pending_transaction_can_have_no_postings_and_has_no_report_effect() {
        let pending = Transaction {
            id: TransactionId::new(),
            occurred_at: timestamp(),
            kind: TransactionKind::Expense,
            status: TransactionStatus::Pending,
            postings: Vec::new(),
            category_id: None,
            original_transaction_id: None,
            report_amount_cny: Some(report_amount(1_200)),
            merchant: None,
            description: None,
        };
        validate_transaction_shape(&pending, &[]).unwrap();
        assert_eq!(report_effect(&pending), None);
    }

    #[test]
    fn ignored_transaction_has_no_report_effect() {
        let ignored = Transaction {
            id: TransactionId::new(),
            occurred_at: timestamp(),
            kind: TransactionKind::Expense,
            status: TransactionStatus::Ignored,
            postings: Vec::new(),
            category_id: None,
            original_transaction_id: None,
            report_amount_cny: Some(report_amount(1_200)),
            merchant: None,
            description: None,
        };
        validate_transaction_shape(&ignored, &[]).unwrap();
        assert_eq!(report_effect(&ignored), None);
    }

    #[test]
    fn refund_requires_parent_and_offsets_expense() {
        let wallet = asset_account("wallet", "CNY");
        let mut refund = confirmed_expense(wallet.clone(), 500);
        refund.kind = TransactionKind::Refund;
        refund.postings[0].delta = Money::new(500, wallet.currency.clone());
        assert!(validate_transaction_shape(&refund, std::slice::from_ref(&wallet)).is_err());

        refund.original_transaction_id = Some(TransactionId::new());
        validate_transaction_shape(&refund, &[wallet]).unwrap();
        assert_eq!(report_effect(&refund).unwrap().expense_cny_minor, -500);
    }

    #[test]
    fn confirmed_financial_transaction_requires_a_posting() {
        let transaction = Transaction {
            id: TransactionId::new(),
            occurred_at: timestamp(),
            kind: TransactionKind::Income,
            status: TransactionStatus::Confirmed,
            postings: Vec::new(),
            category_id: None,
            original_transaction_id: None,
            report_amount_cny: Some(report_amount(1_000)),
            merchant: None,
            description: None,
        };
        assert!(validate_transaction_shape(&transaction, &[]).is_err());
    }

    #[test]
    fn posting_must_reference_an_existing_account_with_matching_currency() {
        let wallet = asset_account("wallet", "CNY");
        let other = asset_account("other", "USD");
        let mut expense = confirmed_expense(wallet.clone(), 100);
        expense.postings[0].account_id = other.id;
        assert!(validate_transaction_shape(&expense, &[wallet, other]).is_err());
    }

    #[test]
    fn transfer_requires_two_distinct_postings() {
        let wallet = asset_account("wallet", "CNY");
        let mut transfer = confirmed_transfer(wallet.clone(), wallet.clone(), 100);
        assert!(validate_transaction_shape(&transfer, std::slice::from_ref(&wallet)).is_err());
        transfer.postings.pop();
        assert!(validate_transaction_shape(&transfer, &[wallet]).is_err());
    }

    #[test]
    fn expense_and_income_postings_follow_account_balance_convention() {
        let wallet = asset_account("wallet", "CNY");
        let mut expense = confirmed_expense(wallet.clone(), 100);
        expense.postings[0].delta = report_amount(100);
        assert!(validate_transaction_shape(&expense, std::slice::from_ref(&wallet)).is_err());

        let mut income = confirmed_expense(wallet.clone(), 100);
        income.kind = TransactionKind::Income;
        income.postings[0].delta = Money::new(-100, wallet.currency.clone());
        assert!(validate_transaction_shape(&income, &[wallet]).is_err());
    }

    #[test]
    fn income_and_refund_reduce_liability_and_expense_increases_it() {
        let card = liability_account("card", "CNY");
        let mut income = confirmed_expense(card.clone(), 100);
        income.kind = TransactionKind::Income;
        income.postings[0].delta = Money::new(-100, card.currency.clone());
        validate_transaction_shape(&income, std::slice::from_ref(&card)).unwrap();

        let mut expense = confirmed_expense(card.clone(), 100);
        expense.postings[0].delta = Money::new(100, card.currency.clone());
        validate_transaction_shape(&expense, std::slice::from_ref(&card)).unwrap();

        let mut refund = income;
        refund.kind = TransactionKind::Refund;
        refund.original_transaction_id = Some(TransactionId::new());
        refund.postings[0].delta = Money::new(-100, card.currency.clone());
        validate_transaction_shape(&refund, &[card]).unwrap();
    }

    #[test]
    fn transfer_rejects_directions_incoherent_with_account_kinds() {
        let first = asset_account("first", "CNY");
        let second = asset_account("second", "CNY");
        let mut transfer = confirmed_transfer(first.clone(), second.clone(), 100);
        transfer.postings[1].delta = Money::new(-100, second.currency.clone());
        assert!(validate_transaction_shape(&transfer, &[first.clone(), second]).is_err());

        let liability = liability_account("card", "CNY");
        let mut transfer = confirmed_transfer(first.clone(), liability.clone(), 100);
        transfer.postings[0].delta = Money::new(100, first.currency.clone());
        transfer.postings[1].delta = Money::new(-100, liability.currency.clone());
        assert!(validate_transaction_shape(&transfer, &[first, liability]).is_err());
    }

    #[test]
    fn transfer_rejects_zero_postings() {
        let first = asset_account("first", "CNY");
        let second = asset_account("second", "CNY");
        let mut transfer = confirmed_transfer(first.clone(), second.clone(), 100);
        transfer.postings[0].delta = Money::new(0, first.currency.clone());
        assert!(validate_transaction_shape(&transfer, &[first, second]).is_err());
    }

    #[test]
    fn report_amount_must_be_non_negative_and_refund_min_does_not_overflow() {
        let wallet = asset_account("wallet", "CNY");
        let mut expense = confirmed_expense(wallet.clone(), 100);
        expense.report_amount_cny = Some(report_amount(-100));
        assert!(validate_transaction_shape(&expense, std::slice::from_ref(&wallet)).is_err());
        assert_eq!(report_effect(&expense), None);

        let mut refund = confirmed_expense(wallet.clone(), 100);
        refund.kind = TransactionKind::Refund;
        refund.original_transaction_id = Some(TransactionId::new());
        refund.postings[0].delta = Money::new(100, wallet.currency.clone());
        refund.report_amount_cny = Some(report_amount(i64::MIN));
        assert!(validate_transaction_shape(&refund, std::slice::from_ref(&wallet)).is_err());
        assert_eq!(report_effect(&refund), None);
    }
}
