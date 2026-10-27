use super::*;
use ledger_app::{FeatureRepository, ImportRepository};
use std::collections::{HashMap, HashSet};

#[derive(Serialize)]
pub struct RefundMatch {
    pub id: String,
    pub confidence: &'static str,
    pub reason: &'static str,
    pub merchant: Option<String>,
    pub amount: Option<String>,
    pub occurred_at: String,
    pub account_id: String,
    pub status: &'static str,
}

/// Querying suggestions never changes a transaction. Only explicit source order
/// references are eligible for automatic linking when confirmation is requested.
pub(super) struct RefundMatcher<'a> {
    repo: &'a SqliteLedgerRepository,
    transactions: HashMap<TransactionId, Transaction>,
    blocked: HashSet<TransactionId>,
    merchants: HashMap<TransactionId, String>,
    refunded: HashMap<TransactionId, i128>,
}

impl<'a> RefundMatcher<'a> {
    pub fn load(repo: &'a SqliteLedgerRepository, overrides: &[Transaction]) -> Result<Self> {
        let mut transactions = repo
            .list_transactions()
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|tx| (tx.id, tx))
            .collect::<HashMap<_, _>>();
        for tx in overrides {
            transactions.insert(tx.id, tx.clone());
        }
        let mut blocked = HashSet::new();
        let mut merchants = HashMap::new();
        for tx in transactions.values() {
            if let Some(original) = repo.metadata(tx.id).map_err(|e| e.to_string())?.reversal_of {
                blocked.insert(tx.id);
                blocked.insert(original);
            }
            if tx.kind == TransactionKind::Expense
                && let Some(merchant) = &tx.merchant
            {
                merchants.insert(
                    tx.id,
                    repo.canonical_merchant(merchant)
                        .map_err(|e| e.to_string())?,
                );
            }
        }
        let mut refunded = HashMap::new();
        for tx in transactions.values().filter(|tx| {
            tx.kind == TransactionKind::Refund
                && tx.status == TransactionStatus::Confirmed
                && !blocked.contains(&tx.id)
        }) {
            if let Some(original) = tx.original_transaction_id
                && let Some(posting) = tx.postings.first()
            {
                *refunded.entry(original).or_insert(0) +=
                    i128::from(posting.delta.minor_units()).abs();
            }
        }
        Ok(Self {
            repo,
            transactions,
            blocked,
            merchants,
            refunded,
        })
    }

    fn eligible(&self, refund: &Transaction, expense: &Transaction) -> bool {
        if expense.kind != TransactionKind::Expense
            || !matches!(
                expense.status,
                TransactionStatus::Pending | TransactionStatus::Confirmed
            )
            || self.blocked.contains(&expense.id)
            || expense.occurred_at > refund.occurred_at
            || expense.postings.len() != 1
            || refund.postings.len() != 1
        {
            return false;
        }
        let original = &expense.postings[0];
        let returned = &refund.postings[0];
        if original.account_id != returned.account_id
            || original.delta.currency() != returned.delta.currency()
        {
            return false;
        }
        let remaining = i128::from(original.delta.minor_units()).abs()
            - self.refunded.get(&expense.id).copied().unwrap_or(0);
        let amount = i128::from(returned.delta.minor_units()).abs();
        amount > 0 && remaining >= amount
    }

    pub fn matches(&self, refund: &Transaction) -> Result<Vec<RefundMatch>> {
        if refund.kind != TransactionKind::Refund
            || refund.status != TransactionStatus::Pending
            || refund.original_transaction_id.is_some()
        {
            return Ok(Vec::new());
        }
        let mut exact = HashSet::new();
        let mut unresolved_reference = false;
        for evidence in self
            .repo
            .source_evidence(refund.id)
            .map_err(|e| e.to_string())?
        {
            if let Some(reference) = evidence.original_external_transaction_id {
                if let Some(id) = self
                    .repo
                    .find_source_transaction(evidence.source_kind, &reference)
                    .map_err(|e| e.to_string())?
                {
                    exact.insert(id);
                } else {
                    unresolved_reference = true;
                }
            }
        }
        if !unresolved_reference && exact.len() == 1 {
            let id = *exact.iter().next().expect("one reference");
            if let Some(expense) = self
                .transactions
                .get(&id)
                .filter(|tx| self.eligible(refund, tx))
            {
                return Ok(vec![Self::view(expense, "exact", "账单原订单号一致")]);
            }
        }
        let Some(merchant) = &refund.merchant else {
            return Ok(Vec::new());
        };
        let canonical = self
            .repo
            .canonical_merchant(merchant)
            .map_err(|e| e.to_string())?;
        if canonical.is_empty() {
            return Ok(Vec::new());
        }
        let mut candidates = self
            .transactions
            .values()
            .filter(|tx| {
                self.eligible(refund, tx)
                    && (refund.occurred_at - tx.occurred_at).num_days() <= 90
                    && self.merchants.get(&tx.id) == Some(&canonical)
            })
            .collect::<Vec<_>>();
        candidates.sort_by_key(|tx| {
            (
                tx.report_amount_cny != refund.report_amount_cny,
                tx.description != refund.description,
                std::cmp::Reverse(tx.occurred_at),
                tx.id.as_uuid().to_string(),
            )
        });
        Ok(candidates
            .into_iter()
            .take(3)
            .map(|tx| Self::view(tx, "suggested", "同平台、同商户，消费时间和可退金额符合"))
            .collect())
    }

    pub fn record_confirmed(&mut self, mut tx: Transaction) {
        if tx.kind == TransactionKind::Refund
            && !self.blocked.contains(&tx.id)
            && let Some(original) = tx.original_transaction_id
            && let Some(posting) = tx.postings.first()
        {
            *self.refunded.entry(original).or_insert(0) +=
                i128::from(posting.delta.minor_units()).abs();
        }
        tx.status = TransactionStatus::Confirmed;
        self.transactions.insert(tx.id, tx);
    }

    fn view(tx: &Transaction, confidence: &'static str, reason: &'static str) -> RefundMatch {
        RefundMatch {
            id: tx.id.as_uuid().to_string(),
            confidence,
            reason,
            merchant: tx.merchant.clone(),
            amount: tx.report_amount_cny.as_ref().map(Money::decimal_string),
            occurred_at: tx.occurred_at.to_rfc3339(),
            account_id: tx.postings[0].account_id.as_uuid().to_string(),
            status: if tx.status == TransactionStatus::Confirmed {
                "confirmed"
            } else {
                "pending"
            },
        }
    }
}
