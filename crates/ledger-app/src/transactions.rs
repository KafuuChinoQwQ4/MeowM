//! Transaction confirmation, ignoring, balance, and transfer use cases.

use ledger_domain::{
    AccountId, Transaction, TransactionId, TransactionKind, TransactionStatus,
    validate_transaction_shape,
};

use crate::{
    AppError, FeatureRepository, ImportRepository, LedgerRepository, RepositoryError, Timestamp,
};
use ledger_domain::SourceEvidence;
use uuid::Uuid;

pub struct TransactionService<R> {
    repository: R,
}

impl<R: LedgerRepository> TransactionService<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub fn repository(&self) -> &R {
        &self.repository
    }

    pub fn confirm(&self, mut transaction: Transaction) -> Result<TransactionId, AppError> {
        if let Some(existing) = self.repository.get_transaction(transaction.id)? {
            let mut proposed = transaction.clone();
            proposed.status = existing.status;
            match existing.status {
                TransactionStatus::Confirmed if existing == proposed => {
                    return Ok(existing.id);
                }
                TransactionStatus::Confirmed => return Err(AppError::TransactionConflict),
                TransactionStatus::Ignored => return Err(AppError::TransactionNotPending),
                TransactionStatus::Pending => {
                    if existing != proposed {
                        return Err(AppError::TransactionConflict);
                    }
                    transaction.status = TransactionStatus::Confirmed;
                    self.validate_confirmation(&transaction)?;
                    self.repository.update_transaction(transaction.clone())?;
                    return Ok(transaction.id);
                }
            }
        }

        transaction.status = TransactionStatus::Confirmed;
        self.validate_confirmation(&transaction)?;
        if let Err(error) = self.repository.insert_transaction(transaction.clone()) {
            if error == crate::RepositoryError::Conflict
                && let Some(existing) = self.repository.get_transaction(transaction.id)?
            {
                if existing == transaction {
                    return Ok(transaction.id);
                }
                return Err(AppError::TransactionConflict);
            }
            return Err(error.into());
        }
        Ok(transaction.id)
    }

    fn validate_confirmation(&self, transaction: &Transaction) -> Result<(), AppError> {
        if transaction.kind == TransactionKind::Refund {
            let parent_id = transaction
                .original_transaction_id
                .ok_or(AppError::RefundParentInvalid)?;
            match self.repository.get_transaction(parent_id)? {
                Some(parent)
                    if parent.kind == TransactionKind::Expense
                        && parent.status == TransactionStatus::Confirmed => {}
                _ => return Err(AppError::RefundParentInvalid),
            }
        }

        let accounts = self.repository.list_accounts()?;
        validate_transaction_shape(transaction, &accounts)?;
        Ok(())
    }

    pub fn ignore(&self, transaction_id: TransactionId) -> Result<(), AppError> {
        let mut transaction = self
            .repository
            .get_transaction(transaction_id)?
            .ok_or(AppError::TransactionNotFound)?;
        if transaction.status != TransactionStatus::Pending {
            return Err(AppError::TransactionNotPending);
        }
        transaction.status = TransactionStatus::Ignored;
        self.repository.update_transaction(transaction)?;
        Ok(())
    }

    pub fn account_balance(
        &self,
        account_id: AccountId,
        at_utc: Timestamp,
    ) -> Result<Option<ledger_domain::Money>, AppError> {
        match self.repository.balance_as_of(account_id, at_utc) {
            Ok(balance) => Ok(balance),
            Err(crate::RepositoryError::NotFound) => Err(AppError::AccountNotFound),
            Err(error) => Err(error.into()),
        }
    }
}

/// Application boundary for the pending inbox. The repository owns transaction
/// boundaries and optimistic versions; this service only translates user intent
/// into the shared contracts.
pub struct PendingService<R> {
    repository: R,
}

impl<R: FeatureRepository> PendingService<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }
    pub fn repository(&self) -> &R {
        &self.repository
    }

    pub fn list(
        &self,
        query: ledger_domain::PendingQuery,
    ) -> Result<ledger_domain::TransactionPage, AppError> {
        self.repository
            .query_transactions(query)
            .map_err(Into::into)
    }

    pub fn confirm(
        &self,
        ids: Vec<ledger_domain::TransactionId>,
        edits: Vec<ledger_domain::TransactionEdit>,
    ) -> Result<(), AppError> {
        self.repository
            .confirm_bulk_with_edits(
                ledger_domain::BulkConfirmation {
                    transaction_ids: ids,
                },
                edits,
            )
            .map_err(Into::into)
    }

    pub fn ignore(&self, ids: &[ledger_domain::TransactionId]) -> Result<(), AppError> {
        self.repository.ignore_bulk(ids).map_err(Into::into)
    }

    pub fn undo_ignore(&self, id: ledger_domain::TransactionId) -> Result<(), AppError> {
        self.repository
            .restore_pending_atomic(id)
            .map_err(Into::into)
    }
}

impl<R: ImportRepository> PendingService<R> {
    pub fn stage_batch(
        &self,
        batch_id: Uuid,
        items: Vec<(Transaction, SourceEvidence)>,
    ) -> Result<Vec<TransactionId>, AppError> {
        if items.iter().any(|(transaction, evidence)| {
            transaction.status != TransactionStatus::Pending || evidence.import_batch_id != batch_id
        }) {
            return Err(RepositoryError::InvalidData(
                "import batch must contain pending candidates with matching evidence".into(),
            )
            .into());
        }
        self.repository
            .insert_pending_batch(batch_id, items)
            .map_err(Into::into)
    }
}

impl<R: FeatureRepository> TransactionService<R> {
    pub fn edit(&self, edit: ledger_domain::TransactionEdit) -> Result<(), AppError> {
        self.repository.edit_atomic(edit).map_err(Into::into)
    }
    pub fn reverse(&self, id: TransactionId, at: Timestamp) -> Result<TransactionId, AppError> {
        self.repository
            .reverse_atomic(id, TransactionId::new(), at)
            .map_err(Into::into)
    }
}
