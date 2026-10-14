//! Account creation and opening-balance use cases.

use ledger_domain::{Account, AccountId, Money};

use crate::{AppError, LedgerRepository, Timestamp};

pub struct AccountService<R> {
    repository: R,
}

impl<R: LedgerRepository> AccountService<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub fn repository(&self) -> &R {
        &self.repository
    }

    pub fn create_account(&self, account: Account) -> Result<(), AppError> {
        if account
            .opening_balance
            .as_ref()
            .is_some_and(|amount| !account.currency_matches(amount))
        {
            return Err(AppError::AccountCurrencyMismatch);
        }
        if account.opening_balance.is_some() != account.opening_balance_effective_at.is_some() {
            return Err(crate::RepositoryError::InvalidData(
                "reference amount and timestamp must both be set or absent".into(),
            )
            .into());
        }
        self.repository.insert_account(account)?;
        Ok(())
    }

    pub fn set_opening_balance(
        &self,
        account_id: AccountId,
        opening_balance: Money,
        effective_at_utc: Timestamp,
    ) -> Result<(), AppError> {
        let mut account = self
            .repository
            .get_account(account_id)?
            .ok_or(AppError::AccountNotFound)?;
        if !account.currency_matches(&opening_balance) {
            return Err(AppError::AccountCurrencyMismatch);
        }
        account.opening_balance = Some(opening_balance);
        account.opening_balance_effective_at = Some(effective_at_utc);
        self.repository.update_account(account)?;
        Ok(())
    }
    /// Sets the balance after all postings on the selected local calendar day.
    /// A later call replaces the account's reference, without adding a transaction.
    pub fn set_day_closing_balance(
        &self,
        account_id: AccountId,
        amount: Money,
        day: chrono::NaiveDate,
        offset: chrono::FixedOffset,
    ) -> Result<(), AppError> {
        use chrono::TimeZone;
        let closing = day
            .and_hms_micro_opt(23, 59, 59, 999_999)
            .and_then(|local| offset.from_local_datetime(&local).single())
            .ok_or_else(|| crate::RepositoryError::InvalidData("invalid local day".into()))?;
        self.set_opening_balance(account_id, amount, closing.with_timezone(&chrono::Utc))
    }

    pub fn clear_opening_balance(&self, account_id: AccountId) -> Result<(), AppError> {
        let mut account = self
            .repository
            .get_account(account_id)?
            .ok_or(AppError::AccountNotFound)?;
        account.opening_balance = None;
        account.opening_balance_effective_at = None;
        self.repository.update_account(account)?;
        Ok(())
    }
}
