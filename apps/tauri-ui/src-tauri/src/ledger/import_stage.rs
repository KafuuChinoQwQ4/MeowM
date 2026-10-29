use super::*;

impl Ledger {
    pub fn stage_preview(
        &self,
        preview_id: &str,
        account: &str,
        ids: Vec<String>,
        edits: Vec<PendingEdit>,
    ) -> Result<Vec<String>> {
        let session = self.preview.borrow();
        let session = session.as_ref().ok_or("预览已失效，请重新选择文件")?;
        if session.id.to_string() != preview_id {
            return Err("预览已失效，请重新选择文件".into());
        }
        if ids.is_empty()
            || ids.len() > ledger_import::MAX_IMPORT_ROWS
            || ids.iter().collect::<std::collections::HashSet<_>>().len() != ids.len()
        {
            return Err("请选择有效候选行".into());
        }
        if edits.iter().any(|edit| !ids.contains(&edit.id))
            || edits
                .iter()
                .map(|edit| &edit.id)
                .collect::<std::collections::HashSet<_>>()
                .len()
                != edits.len()
        {
            return Err("修订字段与候选行不匹配".into());
        }
        let account = self.account(account)?;
        let mut items = Vec::new();
        for id in ids {
            let (_, row, decision) = session
                .rows
                .iter()
                .find(|item| item.0 == id)
                .ok_or("候选行不存在")?;
            if matches!(
                decision,
                ledger_import::PreviewDecision::ExactDuplicate { .. }
            ) {
                return Err("精确重复行不能再次导入".into());
            }
            let amount = row.amount_minor;
            if row.kind == TransactionKind::Transfer
                && !edits.iter().any(|edit| {
                    edit.id == id
                        && edit.kind == "transfer"
                        && !edit.account_id.is_empty()
                        && edit
                            .destination_id
                            .as_ref()
                            .is_some_and(|id| !id.is_empty())
                })
            {
                return Err(format!("第 {} 行转账需选择转出和转入平台", row.row_number));
            }
            let positive = matches!(row.kind, TransactionKind::Income | TransactionKind::Refund);
            let sign = if positive == (account.kind == AccountKind::Asset) {
                1
            } else {
                -1
            };
            let mut tx = Transaction {
                id: transaction_id(&id)?,
                occurred_at: row.occurred_at,
                kind: row.kind,
                status: TransactionStatus::Pending,
                postings: vec![Posting {
                    account_id: account.id,
                    delta: money(amount * sign),
                }],
                category_id: None,
                original_transaction_id: None,
                report_amount_cny: Some(money(amount)),
                merchant: row.merchant.clone(),
                description: row.description.clone(),
            };
            if let Some(edit) = edits.iter().find(|edit| edit.id == id) {
                tx = self.apply_pending_edit(tx, edit)?;
            }
            if tx.kind == TransactionKind::Transfer {
                tx.report_amount_cny = None;
            }
            items.push((
                tx,
                ledger_domain::SourceEvidence {
                    source_kind: row.source_kind,
                    external_transaction_id: row.external_transaction_id.clone(),
                    original_external_transaction_id: row.original_external_transaction_id.clone(),
                    merchant_order_id: row.merchant_order_id.clone(),
                    import_batch_id: session.id,
                    source_fields_digest: row.source_fields_digest,
                },
            ));
        }
        ledger_app::PendingService::new(self.repo()?)
            .stage_batch(session.id, items)
            .map(|ids| ids.iter().map(|id| id.as_uuid().to_string()).collect())
            .map_err(|error| error.to_string())
    }
}
