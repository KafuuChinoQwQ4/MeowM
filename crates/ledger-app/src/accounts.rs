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
        if !account.currency_matches(&account.opening_balance) {
            return Err(AppError::AccountCurrencyMismatch);
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
        account.opening_balance = opening_balance;
        account.opening_balance_effective_at = effective_at_utc;
        self.repository.update_account(account)?;
        Ok(())
    }
}
