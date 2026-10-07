//! User-owned import selection and explicit refund mapping before atomic commit.
use ledger_app::{ImportRecord, ImportRepository};
use ledger_domain::*;
use ledger_import::{DuplicateMatch, ImportCandidate, ImportPreview, ParsedImport};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub struct ImportReview {
    pub preview: ImportPreview,
    pub included: Vec<bool>,
    pub parents: Vec<Transaction>,
    pub merge_targets: Vec<Transaction>,
    resolved: Vec<bool>,
    originals: Vec<ImportCandidate>,
    amounts: Vec<Money>,
    account: Account,
    confirmed_parent_ids: std::collections::HashSet<TransactionId>,
    existing_parents: Vec<Transaction>,
}
impl ImportReview {
    pub fn new(
        parsed: ParsedImport,
        account: Account,
        existing: Vec<ImportRecord>,
    ) -> Result<Self> {
        let batch = ImportCandidate::batch_id(parsed.source, &parsed.raw_rows);
        let amounts = parsed.rows.iter().map(|r| r.amount.clone()).collect();
        let mut candidates = Vec::new();
        for row in parsed.rows {
            let mut c = ImportCandidate::from_normalized(parsed.source, row, account.id, batch);
            if account.kind == AccountKind::Liability {
                for p in &mut c.transaction.postings {
                    p.delta = Money::new(
                        p.delta.minor_units().checked_neg().ok_or("金额溢出")?,
                        p.delta.currency().clone(),
                    );
                }
            }
            candidates.push(c);
        }
        let existing_for_merge = existing.clone();
        let preview =
            ImportPreview::build_with_warnings(candidates, existing.clone(), parsed.warnings)?;
        let confirmed_parent_ids = existing
            .iter()
            .filter(|e| {
                e.transaction.kind == TransactionKind::Expense
                    && e.transaction.status == TransactionStatus::Confirmed
            })
            .map(|e| e.transaction.id)
            .collect();
        let existing_parents = existing
            .into_iter()
            .map(|e| e.transaction)
            .filter(|t| {
                t.kind == TransactionKind::Expense && t.status == TransactionStatus::Confirmed
            })
            .collect();
        let mut merge_targets = Vec::new();
        for e in &existing_for_merge {
            if !merge_targets
                .iter()
                .any(|t: &Transaction| t.id == e.transaction.id)
                && e.transaction.status != TransactionStatus::Ignored
            {
                merge_targets.push(e.transaction.clone());
            }
        }
        let resolved = preview
            .candidates
            .iter()
            .map(|c| c.match_status != DuplicateMatch::StrongCandidate)
            .collect();
        let included = preview
            .candidates
            .iter()
            .map(|c| c.match_status == DuplicateMatch::NoMatch)
            .collect();
        let originals = preview.candidates.clone();
        let mut review = Self {
            originals,
            preview,
            included,
            merge_targets,
            resolved,
            parents: Vec::new(),
            amounts,
            account,
            confirmed_parent_ids,
            existing_parents,
        };
        review.rebuild_parents(None);
        Ok(review)
    }
    fn rebuild_parents(&mut self, replacement: Option<(TransactionId, TransactionId)>) {
        let mut parents = Vec::new();
        for transaction in self.existing_parents.iter().chain(
            self.preview
                .candidates
                .iter()
                .zip(&self.included)
                .zip(&self.resolved)
                .filter(|((c, included), resolved)| {
                    **included && **resolved && c.match_status == DuplicateMatch::NoMatch
                })
                .map(|((c, _), _)| &c.transaction),
        ) {
            if transaction.kind == TransactionKind::Expense
                && !parents.iter().any(|p: &Transaction| p.id == transaction.id)
            {
                parents.push(transaction.clone());
            }
        }
        for candidate in &mut self.preview.candidates {
            if candidate.transaction.kind != TransactionKind::Refund {
                continue;
            }
            let parent = &mut candidate.transaction.original_transaction_id;
            if let Some((old, new)) = replacement {
                // Only migrate a same-batch identity that disappeared. Existing
                // confirmed expenses retain the user's explicit association.
                if *parent == Some(old) && !parents.iter().any(|p| p.id == old) {
                    *parent = Some(new);
                }
            }
            if parent.is_some_and(|id| !parents.iter().any(|p| p.id == id)) {
                *parent = None;
            }
        }
        self.parents = parents;
    }
    pub fn add_existing_parents(&mut self, transactions: impl IntoIterator<Item = Transaction>) {
        for t in transactions {
            if t.kind == TransactionKind::Expense && t.status == TransactionStatus::Confirmed {
                self.confirmed_parent_ids.insert(t.id);
                if !self.existing_parents.iter().any(|p| p.id == t.id) {
                    self.existing_parents.push(t);
                }
            }
        }
        self.rebuild_parents(None);
    }
    pub fn resolve_distinct(&mut self, index: usize) -> Result<()> {
        let c = self
            .preview
            .candidates
            .get_mut(index)
            .ok_or("请选择账单行")?;
        if c.match_status == DuplicateMatch::ExactExternalId {
            return Err("确切重复不可重新入账".into());
        }
        let previous_id = c.transaction.id;
        *c = self.originals[index].clone();
        c.match_status = DuplicateMatch::NoMatch;
        self.resolved[index] = true;
        self.included[index] = true;
        let replacement_id = c.transaction.id;
        self.rebuild_parents(Some((previous_id, replacement_id)));
        Ok(())
    }
    pub fn resolve_merge(&mut self, index: usize, target: usize) -> Result<()> {
        let t = self
            .merge_targets
            .get(target)
            .ok_or("请选择已有交易")?
            .clone();
        let c = self
            .preview
            .candidates
            .get_mut(index)
            .ok_or("请选择账单行")?;
        if c.match_status == DuplicateMatch::ExactExternalId {
            return Err("该行已有关联证据".into());
        }
        let previous_id = c.transaction.id;
        let mut resolved = self.originals[index].clone();
        if t.kind != resolved.transaction.kind
            || t.report_amount_cny != resolved.transaction.report_amount_cny
        {
            return Err("交易类型或人民币金额冲突，不能合并".into());
        }
        if t.status == TransactionStatus::Confirmed {
            if t.postings != resolved.transaction.postings {
                return Err("账户或原币金额冲突，不能合并".into());
            }
            resolved.transaction = t;
        } else {
            resolved.transaction.id = t.id;
        }
        resolved.match_status = DuplicateMatch::NoMatch;
        *c = resolved;
        self.resolved[index] = true;
        self.included[index] = true;
        let replacement_id = c.transaction.id;
        self.rebuild_parents(Some((previous_id, replacement_id)));
        Ok(())
    }
    pub fn set_included(&mut self, index: usize, included: bool) -> Result<()> {
        let c = self.preview.candidates.get(index).ok_or("请先选择账单行")?;
        if included && c.match_status != DuplicateMatch::NoMatch {
            return Err("重复或疑似重复行不可直接重新入账".into());
        }
        *self.included.get_mut(index).ok_or("请先选择账单行")? = included;
        if !included {
            self.resolved[index] = true;
        }
        self.rebuild_parents(None);
        Ok(())
    }
    pub fn map_refund(&mut self, index: usize, parent_index: usize) -> Result<()> {
        let parent = self.parents.get(parent_index).ok_or("请选择原支出")?.id;
        let c = self
            .preview
            .candidates
            .get_mut(index)
            .ok_or("请选择退款行")?;
        if c.transaction.kind != TransactionKind::Refund {
            return Err("仅退款行需要关联原支出".into());
        }
        c.transaction.original_transaction_id = Some(parent);
        let amount = &self.amounts[index];
        let sign = if self.account.kind == AccountKind::Asset {
            1
        } else {
            -1
        };
        c.transaction.postings = vec![Posting {
            account_id: self.account.id,
            delta: Money::new(sign * amount.minor_units(), amount.currency().clone()),
        }];
        Ok(())
    }
    pub fn confirm(
        &self,
        repository: &impl ImportRepository,
    ) -> Result<ledger_import::ImportReceipt> {
        if self.resolved.iter().any(|v| !v) {
            return Err("请逐行处理疑似重复：合并、明确独立交易或取消纳入".into());
        }
        let mut preview = self.preview.clone();
        preview.candidates = preview
            .candidates
            .into_iter()
            .zip(&self.included)
            .filter(|(_, keep)| **keep)
            .map(|(c, _)| c)
            .collect();
        if preview.candidates.is_empty() {
            return Err("请至少选择一笔可导入记录".into());
        }
        for c in &preview.candidates {
            if c.transaction.kind == TransactionKind::Refund {
                let parent = c
                    .transaction
                    .original_transaction_id
                    .ok_or("请为退款明确选择原支出，或取消该行的导入勾选")?;
                if !self.confirmed_parent_ids.contains(&parent)
                    && !preview.candidates.iter().any(|p| {
                        p.transaction.id == parent && p.transaction.kind == TransactionKind::Expense
                    })
                {
                    return Err("关联的原支出已变更或取消导入，请重新选择或同时取消退款".into());
                }
            }
        }
        // Keep transaction IDs tied to their source rows, while each explicitly
        // selected subset gets an independent, deterministic commit identity.
        let mut row_ids = preview
            .candidates
            .iter()
            .map(|c| uuid::Uuid::new_v5(&self.preview.batch_id, &c.evidence.source_fields_digest))
            .collect::<Vec<_>>();
        row_ids.sort_unstable();
        let selected_rows = row_ids
            .iter()
            .flat_map(|id| id.as_bytes().iter().copied())
            .collect::<Vec<_>>();
        preview.batch_id = uuid::Uuid::new_v5(&self.preview.batch_id, &selected_rows);
        for candidate in &mut preview.candidates {
            candidate.evidence.import_batch_id = preview.batch_id;
        }
        Ok(preview.confirm(repository)?)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use ledger_app::{MobileLedger, TransactionFilter};
    use ledger_storage::sqlite::SqliteLedgerRepository;
    #[test]
    fn user_maps_refund_to_same_batch_expense_and_explicitly_excludes_unresolved_row() {
        let app = MobileLedger::new(SqliteLedgerRepository::in_memory().unwrap());
        let cny = CurrencyCode::new("CNY", 2).unwrap();
        let now = "2026-10-07T08:00:00Z".parse().unwrap();
        let id = app
            .create_account(
                "Cash",
                AccountKind::Asset,
                Money::new(10000, cny),
                "2026-01-01T00:00:00Z".parse().unwrap(),
            )
            .unwrap();
        let account = ledger_app::LedgerRepository::get_account(app.repository(), id)
            .unwrap()
            .unwrap();
        let csv = "交易时间,交易类型,交易对方,商品,收/支,金额(元),支付方式,当前状态,交易单号,商户单号,备注,原交易单号\n2026-10-07 10:31:00,退款,Cafe,Tea,收入,5.00,零钱,退款成功,refund-1,,,expense-1\n2026-10-07 10:30:00,商户消费,Cafe,Tea,支出,20.00,零钱,支付成功,expense-1,,,\n2026-10-07 10:32:00,退款,Other,Tea,收入,1.00,零钱,退款成功,refund-2,,,unknown\n";
        let parsed = ledger_import::parse_wechat_csv(csv.as_bytes()).unwrap();
        let mut review = ImportReview::new(parsed, account, vec![]).unwrap();
        assert!(review.confirm(&app).is_err());
        assert!(
            app.snapshot(now, TransactionFilter::default())
                .unwrap()
                .transactions
                .is_empty()
        );
        review.map_refund(0, 0).unwrap();
        review.set_included(1, false).unwrap();
        assert!(review.confirm(&app).is_err());
        assert!(
            app.snapshot(now, TransactionFilter::default())
                .unwrap()
                .transactions
                .is_empty()
        );
        review.set_included(1, true).unwrap();
        // Removing an eligible parent invalidates its old refund mapping.
        assert_eq!(
            review.preview.candidates[0]
                .transaction
                .original_transaction_id,
            None
        );
        review.map_refund(0, 0).unwrap();
        review.set_included(2, false).unwrap();
        review.confirm(&app).unwrap();
        let state = app.snapshot(now, TransactionFilter::default()).unwrap();
        assert_eq!(state.transactions.len(), 2);
        assert_eq!(state.month.expense_cny_minor, 1500);
        assert_eq!(state.accounts[0].balance.minor_units(), 8500);
        review.confirm(&app).unwrap();
        assert_eq!(
            app.snapshot(now, TransactionFilter::default())
                .unwrap()
                .transactions
                .len(),
            2
        );
    }
    #[test]
    fn explicit_merge_preserves_notification_identity_and_import_replay() {
        let csv = format!(
            "{HEADER}\n2026-10-07 10:30:00,商户消费,Cafe,Tea,支出,20.00,零钱,支付成功,expense-merge,,,\n"
        );
        let (app, account, parsed) = fixture(&csv);
        let original = ImportReview::new(parsed.clone(), account.clone(), vec![])
            .unwrap()
            .preview
            .candidates[0]
            .transaction
            .clone();
        let mut pending = original.clone();
        pending.id = TransactionId::new();
        pending.postings.clear();
        let batch = uuid::Uuid::new_v4();
        ledger_app::MobileRepository::capture_pending(
            app.repository(),
            ImportRecord {
                transaction: pending.clone(),
                evidence: SourceEvidence {
                    source_kind: SourceKind::Notification,
                    external_transaction_id: Some("notification-synthetic".into()),
                    import_batch_id: batch,
                    source_fields_digest: [7; 32],
                },
            },
            ledger_app::PendingDetails {
                amount: Some(Money::new(2000, CurrencyCode::new("CNY", 2).unwrap())),
                source: "wechat".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let mut review = ImportReview::new(
            parsed.clone(),
            account.clone(),
            app.list_import_records().unwrap(),
        )
        .unwrap();
        assert_eq!(
            review.preview.candidates[0].match_status,
            DuplicateMatch::StrongCandidate
        );
        assert!(review.confirm(&app).is_err());
        review.resolve_merge(0, 0).unwrap();
        review.confirm(&app).unwrap();
        let records = app.list_import_records().unwrap();
        assert_eq!(records.len(), 2);
        assert!(records.iter().all(|r| r.transaction.id == pending.id));
        assert_eq!(
            ledger_app::LedgerRepository::list_transactions(app.repository())
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            ledger_app::LedgerRepository::get_transaction(app.repository(), pending.id)
                .unwrap()
                .unwrap()
                .status,
            TransactionStatus::Confirmed
        );
        assert_eq!(
            ImportReview::new(parsed, account, records)
                .unwrap()
                .preview
                .candidates[0]
                .match_status,
            DuplicateMatch::ExactExternalId
        );
        app.delete_data(
            ledger_app::DeletionScope::ImportBatch(review.confirm(&app).unwrap().batch_id),
            true,
        )
        .unwrap();
        assert_eq!(app.list_import_records().unwrap().len(), 1);
        assert_eq!(
            ledger_app::LedgerRepository::list_transactions(app.repository())
                .unwrap()
                .len(),
            1
        );
    }
    #[test]
    fn explicit_distinct_resolution_persists_and_is_replay_safe() {
        let csv = format!(
            "{HEADER}\n2026-10-07 10:30:00,商户消费,Cafe,Tea,支出,20.00,零钱,支付成功,expense-a,,,\n"
        );
        let (app, account, parsed) = fixture(&csv);
        ImportReview::new(parsed, account.clone(), vec![])
            .unwrap()
            .confirm(&app)
            .unwrap();
        let parsed =
            ledger_import::parse_wechat_csv(csv.replace("expense-a", "expense-b").as_bytes())
                .unwrap();
        let mut review = ImportReview::new(
            parsed.clone(),
            account.clone(),
            app.list_import_records().unwrap(),
        )
        .unwrap();
        assert!(review.confirm(&app).is_err());
        review.resolve_distinct(0).unwrap();
        review.confirm(&app).unwrap();
        assert_eq!(
            ledger_app::LedgerRepository::list_transactions(app.repository())
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            ImportReview::new(parsed, account, app.list_import_records().unwrap())
                .unwrap()
                .preview
                .candidates[0]
                .match_status,
            DuplicateMatch::ExactExternalId
        );
    }
    fn pending_notification(
        app: &MobileLedger<SqliteLedgerRepository>,
        mut transaction: Transaction,
    ) -> TransactionId {
        transaction.id = TransactionId::new();
        transaction.status = TransactionStatus::Pending;
        transaction.postings.clear();
        let id = transaction.id;
        ledger_app::MobileRepository::capture_pending(
            app.repository(),
            ImportRecord {
                transaction,
                evidence: SourceEvidence {
                    source_kind: SourceKind::Notification,
                    external_transaction_id: Some("notification-refund-parent".into()),
                    import_batch_id: uuid::Uuid::new_v4(),
                    source_fields_digest: [9; 32],
                },
            },
            ledger_app::PendingDetails {
                amount: Some(Money::new(2000, CurrencyCode::new("CNY", 2).unwrap())),
                source: "wechat".into(),
                ..Default::default()
            },
        )
        .unwrap();
        id
    }
    #[test]
    fn resolving_notification_match_exposes_refund_parent_for_merge_and_distinct() {
        for merge in [true, false] {
            let csv = format!(
                "{HEADER}\n2026-10-07 10:30:00,商户消费,Cafe,Tea,支出,20.00,零钱,支付成功,expense-review,,,\n2026-10-07 11:30:00,退款,Cafe,Tea,收入,5.00,零钱,退款成功,refund-review,,,expense-review\n"
            );
            let (app, account, parsed) = fixture(&csv);
            let original = ImportReview::new(parsed.clone(), account.clone(), vec![])
                .unwrap()
                .preview
                .candidates[0]
                .transaction
                .clone();
            let pending_id = pending_notification(&app, original.clone());
            let mut review = ImportReview::new(
                parsed.clone(),
                account.clone(),
                app.list_import_records().unwrap(),
            )
            .unwrap();
            assert_eq!(
                review.preview.candidates[0].match_status,
                DuplicateMatch::StrongCandidate
            );
            assert!(review.parents.is_empty());
            let expected_parent = if merge {
                review.resolve_merge(0, 0).unwrap();
                pending_id
            } else {
                review.resolve_distinct(0).unwrap();
                original.id
            };
            let parent = review
                .parents
                .iter()
                .position(|p| p.id == expected_parent)
                .expect("resolved expense must be available as refund parent");
            review.map_refund(1, parent).unwrap();
            review.confirm(&app).unwrap();
            assert_eq!(review.confirm(&app).unwrap().inserted, 0);
            let transactions =
                ledger_app::LedgerRepository::list_transactions(app.repository()).unwrap();
            assert_eq!(
                transactions
                    .iter()
                    .find(|t| t.kind == TransactionKind::Refund)
                    .unwrap()
                    .original_transaction_id,
                Some(expected_parent)
            );
            let state = app
                .snapshot(
                    "2026-10-07T08:00:00Z".parse().unwrap(),
                    TransactionFilter::default(),
                )
                .unwrap();
            assert_eq!(state.accounts[0].balance.minor_units(), 8500);
            let replay =
                ImportReview::new(parsed, account, app.list_import_records().unwrap()).unwrap();
            assert!(replay.included.iter().all(|keep| !keep));
        }
    }
    #[test]
    fn merging_prelinked_expense_retargets_refund_by_identity_and_remains_replay_safe() {
        let csv = format!(
            "{HEADER}\n2026-10-07 10:30:00,商户消费,Cafe,Tea,支出,20.00,零钱,支付成功,expense-review,,,\n2026-10-07 11:30:00,退款,Cafe,Tea,收入,5.00,零钱,退款成功,refund-review,,,expense-review\n"
        );
        let (app, account, parsed) = fixture(&csv);
        let mut original = ImportReview::new(parsed.clone(), account.clone(), vec![])
            .unwrap()
            .preview
            .candidates[0]
            .transaction
            .clone();
        original.occurred_at -= chrono::Duration::days(1);
        let pending_id = pending_notification(&app, original);
        let mut review =
            ImportReview::new(parsed, account, app.list_import_records().unwrap()).unwrap();
        assert_eq!(
            review.preview.candidates[0].match_status,
            DuplicateMatch::NoMatch
        );
        review.map_refund(1, 0).unwrap();
        review.resolve_merge(0, 0).unwrap();
        assert_eq!(
            review.preview.candidates[1]
                .transaction
                .original_transaction_id,
            Some(pending_id)
        );
        assert_eq!(review.parents.len(), 1);
        assert_eq!(review.parents[0].id, pending_id);
        review.resolve_distinct(0).unwrap();
        let distinct_id = review.preview.candidates[0].transaction.id;
        assert_ne!(distinct_id, pending_id);
        assert_eq!(
            review.preview.candidates[1]
                .transaction
                .original_transaction_id,
            Some(distinct_id)
        );
        review.resolve_merge(0, 0).unwrap();
        assert_eq!(
            review.preview.candidates[1]
                .transaction
                .original_transaction_id,
            Some(pending_id)
        );
        review.set_included(0, false).unwrap();
        assert!(review.parents.is_empty());
        assert_eq!(
            review.preview.candidates[1]
                .transaction
                .original_transaction_id,
            None
        );
        assert!(review.confirm(&app).is_err());
        review.set_included(0, true).unwrap();
        assert_eq!(review.parents[0].id, pending_id);
        assert!(review.confirm(&app).is_err());
        review.map_refund(1, 0).unwrap();
        review.confirm(&app).unwrap();
        assert_eq!(review.confirm(&app).unwrap().inserted, 0);
        let transactions =
            ledger_app::LedgerRepository::list_transactions(app.repository()).unwrap();
        assert_eq!(transactions.len(), 2);
        assert_eq!(
            transactions
                .iter()
                .find(|t| t.kind == TransactionKind::Refund)
                .unwrap()
                .original_transaction_id,
            Some(pending_id)
        );
    }
    const HEADER: &str = "交易时间,交易类型,交易对方,商品,收/支,金额(元),支付方式,当前状态,交易单号,商户单号,备注,原交易单号";
    fn fixture(csv: &str) -> (MobileLedger<SqliteLedgerRepository>, Account, ParsedImport) {
        let app = MobileLedger::new(SqliteLedgerRepository::in_memory().unwrap());
        let id = app
            .create_account(
                "Cash",
                AccountKind::Asset,
                Money::new(10000, CurrencyCode::new("CNY", 2).unwrap()),
                "2026-01-01T00:00:00Z".parse().unwrap(),
            )
            .unwrap();
        let account = ledger_app::LedgerRepository::get_account(app.repository(), id)
            .unwrap()
            .unwrap();
        (
            app,
            account,
            ledger_import::parse_wechat_csv(csv.as_bytes()).unwrap(),
        )
    }
    #[test]
    fn selecting_different_subsets_imports_remaining_rows_and_replays_each_subset_once() {
        let csv = format!(
            "{HEADER}\n2026-10-07 10:30:00,商户消费,A,Tea,支出,20.00,零钱,支付成功,expense-a,,,\n2026-10-07 11:30:00,商户消费,B,Tea,支出,5.00,零钱,支付成功,expense-b,,,\n"
        );
        let (app, account, parsed) = fixture(&csv);
        let mut first = ImportReview::new(parsed.clone(), account.clone(), vec![]).unwrap();
        first.set_included(1, false).unwrap();
        assert_eq!(first.confirm(&app).unwrap().inserted, 1);
        assert_eq!(first.confirm(&app).unwrap().inserted, 0);
        let second =
            ImportReview::new(parsed, account, app.list_import_records().unwrap()).unwrap();
        assert_eq!(second.included, vec![false, true]);
        assert_eq!(second.confirm(&app).unwrap().inserted, 1);
        assert_eq!(second.confirm(&app).unwrap().inserted, 0);
        let state = app
            .snapshot(
                "2026-10-07T08:00:00Z".parse().unwrap(),
                TransactionFilter::default(),
            )
            .unwrap();
        assert_eq!(state.transactions.len(), 2);
        assert_eq!(state.accounts[0].balance.minor_units(), 7500);
        assert_eq!(app.repository().import_batch_count().unwrap(), 2);
    }
    #[test]
    fn refund_can_reference_confirmed_expense_excluded_as_duplicate_in_current_preview() {
        let csv = format!(
            "{HEADER}\n2026-10-07 10:30:00,商户消费,Cafe,Tea,支出,20.00,零钱,支付成功,expense-a,,,\n2026-10-07 11:30:00,退款,Cafe,Tea,收入,5.00,零钱,退款成功,refund-a,,,expense-a\n"
        );
        let (app, account, parsed) = fixture(&csv);
        let mut first = ImportReview::new(parsed.clone(), account.clone(), vec![]).unwrap();
        first.set_included(1, false).unwrap();
        first.confirm(&app).unwrap();
        let mut refund =
            ImportReview::new(parsed, account, app.list_import_records().unwrap()).unwrap();
        assert_eq!(refund.included, vec![false, true]);
        refund.map_refund(1, 0).unwrap();
        assert_eq!(refund.confirm(&app).unwrap().inserted, 1);
        let state = app
            .snapshot(
                "2026-10-07T08:00:00Z".parse().unwrap(),
                TransactionFilter::default(),
            )
            .unwrap();
        assert_eq!(state.transactions.len(), 2);
        assert_eq!(state.month.expense_cny_minor, 1500);
        assert_eq!(state.accounts[0].balance.minor_units(), 8500);
    }
}
