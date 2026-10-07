use crate::{LedgerState, LedgerWindow, Row};
use chrono::{Local, NaiveDate, TimeZone, Utc};
use ledger_app::*;
use ledger_domain::*;
use ledger_storage::sqlite::SqliteLedgerRepository;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::{cell::RefCell, rc::Rc};
type App = MobileLedger<SqliteLedgerRepository>;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
struct Controller {
    app: App,
    snapshot: MobileSnapshot,
    filter: TransactionFilter,
    editing: Option<TransactionRow>,
    import: Option<crate::import_review::ImportReview>,
    delete_confirmation: Option<DeletionScope>,
    #[cfg(target_os = "android")]
    document_timer: slint::Timer,
}
fn strings(values: impl IntoIterator<Item = String>) -> ModelRc<SharedString> {
    Rc::new(VecModel::from(
        values
            .into_iter()
            .map(SharedString::from)
            .collect::<Vec<_>>(),
    ))
    .into()
}
fn money(n: i64) -> String {
    Money::new(n, CurrencyCode::new("CNY", 2).unwrap()).decimal_string()
}
fn account_id(state: &Controller, index: i32) -> Option<AccountId> {
    usize::try_from(index - 1)
        .ok()
        .and_then(|i| state.snapshot.accounts.get(i))
        .map(|a| a.account.id)
}
fn category_id(state: &Controller, index: i32) -> Option<CategoryId> {
    usize::try_from(index - 1)
        .ok()
        .and_then(|i| state.snapshot.categories.get(i))
        .map(|a| a.id)
}
fn account_index(state: &Controller, id: Option<AccountId>) -> i32 {
    state
        .snapshot
        .accounts
        .iter()
        .position(|a| Some(a.account.id) == id)
        .map(|i| i as i32 + 1)
        .unwrap_or(0)
}
fn category_index(state: &Controller, id: Option<CategoryId>) -> i32 {
    state
        .snapshot
        .categories
        .iter()
        .position(|a| Some(a.id) == id)
        .map(|i| i as i32 + 1)
        .unwrap_or(0)
}
fn date(value: &str) -> Result<Timestamp> {
    let day = NaiveDate::parse_from_str(value, "%Y-%m-%d")?;
    Local
        .from_local_datetime(&day.and_hms_opt(0, 0, 0).unwrap())
        .single()
        .map(|v| v.with_timezone(&Utc))
        .ok_or_else(|| "This local date is ambiguous".into())
}
/// Exact decimal parser: no floats, no silently discarded fraction digits.
fn parse_money(value: &str, currency: &CurrencyCode) -> Result<Money> {
    let value = value.trim();
    let (negative, digits) = value
        .strip_prefix('-')
        .map(|v| (true, v))
        .unwrap_or((false, value));
    let mut parts = digits.split('.');
    let whole = parts.next().unwrap_or("");
    let fraction = parts.next().unwrap_or("");
    let exp = usize::from(currency.minor_unit_exponent());
    if whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || !fraction.bytes().all(|b| b.is_ascii_digit())
        || fraction.len() > exp
        || parts.next().is_some()
    {
        return Err("Enter a valid amount with the currency's decimal precision".into());
    }
    let scale = 10i64.pow(exp as u32);
    let whole: i64 = whole.parse()?;
    let fract: i64 = if fraction.is_empty() {
        0
    } else {
        fraction
            .parse::<i64>()?
            .checked_mul(10i64.pow((exp - fraction.len()) as u32))
            .ok_or("Amount overflow")?
    };
    let minor = whole
        .checked_mul(scale)
        .and_then(|v| v.checked_add(fract))
        .ok_or("Amount overflow")?;
    Ok(Money::new(
        if negative { -minor } else { minor },
        currency.clone(),
    ))
}
fn view_row(row: &TransactionRow) -> Row {
    let t = &row.transaction;
    let amount = t
        .postings
        .first()
        .map(|p| &p.delta)
        .or_else(|| row.details.as_ref().and_then(|d| d.amount.as_ref()))
        .or(t.report_amount_cny.as_ref());
    Row {
        title: t.merchant.as_deref().unwrap_or("Transaction").into(),
        subtitle: format!(
            "{} · {} · {}",
            t.occurred_at.with_timezone(&Local).format("%Y-%m-%d"),
            row.source,
            t.description.as_deref().unwrap_or("")
        )
        .into(),
        amount: amount
            .map(|m| {
                format!(
                    "{} {}",
                    m.currency().code(),
                    m.decimal_string().trim_start_matches('-')
                )
            })
            .unwrap_or_else(|| "Amount needed".into())
            .into(),
        status: format!(
            "{} · {}",
            match t.status {
                TransactionStatus::Pending => "待确认",
                TransactionStatus::Confirmed => "已确认",
                TransactionStatus::Ignored => "已忽略",
            },
            match t.kind {
                TransactionKind::Income => "收入",
                TransactionKind::Expense => "支出",
                TransactionKind::Transfer => "转账",
                TransactionKind::Refund => "退款",
                TransactionKind::BalanceAdjustment => "余额调整",
            }
        )
        .into(),
    }
}
fn clear_delete_confirmation(w: &LedgerWindow, c: &mut Controller) {
    c.delete_confirmation = None;
    let g = w.global::<LedgerState>();
    g.set_delete_transaction_confirmed(false);
    g.set_delete_batch_confirmed(false);
    g.set_delete_all_confirmed(false);
}
fn deletion_scope(w: &LedgerWindow, c: &Controller, kind: i32) -> Result<DeletionScope> {
    match kind {
        0 => Ok(DeletionScope::Transaction(
            c.editing.as_ref().ok_or("请选择交易")?.transaction.id,
        )),
        1 => {
            let mut batches = c
                .app
                .list_import_records()?
                .iter()
                .map(|r| r.evidence.import_batch_id)
                .collect::<Vec<_>>();
            batches.sort();
            batches.dedup();
            Ok(DeletionScope::ImportBatch(
                *batches
                    .get(w.global::<LedgerState>().get_batch_selection() as usize)
                    .ok_or("请选择批次")?,
            ))
        }
        2 => Ok(DeletionScope::AllLocal),
        _ => Err("无效删除范围".into()),
    }
}
fn refresh(w: &LedgerWindow, c: &mut Controller) -> Result<()> {
    clear_delete_confirmation(w, c);
    c.snapshot =
        c.app
            .snapshot_in_timezone(Utc::now(), *Local::now().offset(), c.filter.clone())?;
    let g = w.global::<LedgerState>();
    let mut batches = c
        .app
        .list_import_records()?
        .iter()
        .map(|r| r.evidence.import_batch_id)
        .collect::<Vec<_>>();
    batches.sort();
    batches.dedup();
    g.set_batch_names(strings(batches.iter().map(|b| b.to_string())));
    let rules = c.app.repository().list_rules()?;
    g.set_rule_names(strings(rules.iter().map(|r| {
        format!(
            "{} · {} · {}{}",
            r.priority,
            r.merchant_contains.as_deref().unwrap_or(""),
            r.description_contains.as_deref().unwrap_or(""),
            if r.enabled { "" } else { "（停用）" }
        )
    })));
    let s = &c.snapshot;
    g.set_month_title(Local::now().format("%B %Y").to_string().into());
    g.set_income(money(s.month.income_cny_minor).into());
    g.set_expense(
        money(
            s.month
                .expense_cny_minor
                .checked_add(s.refund_cny_minor)
                .ok_or("Amount overflow")?,
        )
        .into(),
    );
    g.set_net_expense(money(s.month.expense_cny_minor).into());
    g.set_accounts(strings(s.accounts.iter().map(|a| {
        format!(
            "{}   {} {}",
            a.account.display_name,
            a.balance.currency().code(),
            a.balance.decimal_string()
        )
    })));
    g.set_account_names(strings(
        std::iter::once("选择 / 全部账户".into()).chain(
            s.accounts
                .iter()
                .map(|a| format!("{} ({})", a.account.display_name, a.account.currency.code())),
        ),
    ));
    g.set_category_names(strings(
        std::iter::once("未分类 / 全部分类".into()).chain(
            s.categories
                .iter()
                .map(|c| format!("{}{}", c.name, if c.enabled { "" } else { "（已停用）" })),
        ),
    ));
    g.set_transactions(
        Rc::new(VecModel::from(
            s.transactions.iter().map(view_row).collect::<Vec<_>>(),
        ))
        .into(),
    );
    g.set_pending(
        Rc::new(VecModel::from(
            s.pending.iter().map(view_row).collect::<Vec<_>>(),
        ))
        .into(),
    );
    let all = c.app.repository().list_transactions()?;
    g.set_refund_names(strings(
        std::iter::once("选择原已确认支出".into()).chain(
            all.iter()
                .filter(|t| {
                    t.status == TransactionStatus::Confirmed && t.kind == TransactionKind::Expense
                })
                .map(|t| {
                    format!(
                        "{} · {} · {}",
                        t.occurred_at.with_timezone(&Local).format("%m-%d"),
                        t.merchant.as_deref().unwrap_or("Expense"),
                        t.report_amount_cny
                            .as_ref()
                            .map(Money::decimal_string)
                            .unwrap_or_default()
                    )
                }),
        ),
    ));
    let mut lines = vec![
        format!("本月退款抵扣：¥ {}", money(s.refund_cny_minor)),
        "月度趋势 · 收入 / 净支出".into(),
    ];
    lines.extend(
        s.trend
            .iter()
            .map(|(m, i, e)| format!("{m}     {} / {}", money(*i), money(*e))),
    );
    lines.push("分类占比 · 净支出".into());
    for category in &s.categories {
        if let Some(n) = s.month.expense_by_category.get(&category.id) {
            let percent = if s.month.expense_cny_minor > 0 {
                i128::from(*n) * 100 / i128::from(s.month.expense_cny_minor)
            } else {
                0
            };
            lines.push(format!(
                "{}   CNY {}   ({}%)",
                category.name,
                money(*n),
                percent
            ));
        }
    }
    let categorized = s
        .month
        .expense_by_category
        .values()
        .try_fold(0i64, |a, b| a.checked_add(*b))
        .ok_or("Amount overflow")?;
    let uncategorized = s
        .month
        .expense_cny_minor
        .checked_sub(categorized)
        .ok_or("Amount overflow")?;
    if uncategorized != 0 {
        lines.push(format!("未分类   ¥ {}", money(uncategorized)))
    }
    lines.push("商户排行 · 净支出".into());
    lines.extend(
        s.merchants
            .iter()
            .map(|(m, n)| format!("{m}   CNY {}", money(*n))),
    );
    g.set_report_lines(strings(lines));
    Ok(())
}
fn editor(w: &LedgerWindow, c: &mut Controller, row: Option<TransactionRow>) -> Result<()> {
    let g = w.global::<LedgerState>();
    g.set_editing_new(row.is_none());
    g.set_editing_pending(
        row.as_ref()
            .is_some_and(|r| r.transaction.status == TransactionStatus::Pending),
    );
    clear_delete_confirmation(w, c);
    g.set_editor_title(
        if row.is_none() {
            "新增交易"
        } else if g.get_editing_pending() {
            "确认待处理交易"
        } else {
            "编辑分类与备注"
        }
        .into(),
    );
    let settings = c.app.settings()?;
    let t = row.as_ref().map(|r| &r.transaction);
    let amount = row
        .as_ref()
        .and_then(|r| r.details.as_ref().and_then(|d| d.amount.as_ref()))
        .or_else(|| t.and_then(|t| t.postings.first().map(|p| &p.delta)))
        .or_else(|| t.and_then(|t| t.report_amount_cny.as_ref()));
    g.set_editor_amount(
        amount
            .map(|m| m.decimal_string().trim_start_matches('-').to_string())
            .unwrap_or_default()
            .into(),
    );
    g.set_editor_report(
        t.and_then(|t| t.report_amount_cny.as_ref())
            .map(Money::decimal_string)
            .unwrap_or_default()
            .into(),
    );
    g.set_editor_date(
        t.map(|t| {
            t.occurred_at
                .with_timezone(&Local)
                .format("%Y-%m-%d")
                .to_string()
        })
        .unwrap_or_else(|| Local::now().format("%Y-%m-%d").to_string())
        .into(),
    );
    g.set_editor_account(account_index(
        c,
        t.and_then(|t| t.postings.first().map(|p| p.account_id))
            .or(settings.default_account),
    ));
    g.set_editor_destination(0);
    let suggested = t.and_then(|t| t.category_id).or_else(|| {
        t.and_then(|t| {
            ledger_app::classification::suggest_category(
                t,
                &c.app.repository().list_rules().unwrap_or_default(),
                &c.snapshot.categories,
            )
        })
    });
    g.set_editor_category(category_index(c, suggested));
    g.set_editor_kind(match t.map(|t| t.kind) {
        Some(TransactionKind::Income) => 1,
        Some(TransactionKind::Transfer) => 2,
        Some(TransactionKind::Refund) => 3,
        _ => 0,
    });
    g.set_editor_merchant(t.and_then(|t| t.merchant.as_deref()).unwrap_or("").into());
    g.set_editor_memo(
        t.and_then(|t| t.description.as_deref())
            .unwrap_or("")
            .into(),
    );
    g.set_editor_refund(0);
    g.set_editor_hint(
        row.as_ref()
            .and_then(|r| r.details.as_ref())
            .map(|d| {
                format!(
                    "{} · account hint: {} · original reference: {}",
                    d.source,
                    d.account_hint.as_deref().unwrap_or("none"),
                    d.original_external_id.as_deref().unwrap_or("none")
                )
            })
            .unwrap_or_else(|| "确认后影响所选账户。外币交易需要明确填写人民币报表金额。".into())
            .into(),
    );
    c.editing = row;
    g.set_page(5);
    Ok(())
}
fn render_import(w: &LedgerWindow, review: &crate::import_review::ImportReview) {
    let g = w.global::<LedgerState>();
    let p = &review.preview;
    let mut lines = vec![format!(
        "新增 {} · 重复 {} · 疑似重复 {} · 警告 {} · 已选择 {}",
        p.counts.new,
        p.counts.duplicate,
        p.counts.suspected,
        p.counts.warnings,
        review.included.iter().filter(|v| **v).count()
    )];
    let rows = p
        .candidates
        .iter()
        .zip(&review.included)
        .enumerate()
        .map(|(i, (c, keep))| {
            format!(
                "{} {} · {} · {}{}",
                if *keep { "✓" } else { "－" },
                i + 1,
                c.transaction.merchant.as_deref().unwrap_or("交易"),
                c.transaction
                    .report_amount_cny
                    .as_ref()
                    .map(Money::decimal_string)
                    .unwrap_or_else(|| "缺少人民币估值".into()),
                if c.transaction.kind == TransactionKind::Refund {
                    if c.transaction.original_transaction_id.is_some() {
                        " · 退款已关联"
                    } else {
                        " · 退款待关联"
                    }
                } else {
                    ""
                }
            )
        })
        .collect::<Vec<_>>();
    lines.extend(rows.clone());
    lines.extend(
        p.warnings
            .iter()
            .filter(|v| v.code != ledger_import::WarningCode::MissingRefundOriginal)
            .map(|v| format!("第{}行：{}", v.row_number, v.message)),
    );
    g.set_import_lines(strings(lines));
    g.set_import_row_names(strings(rows));
    // Parent eligibility and identity may change after a resolution. Require a
    // fresh selection instead of carrying an index to a different transaction.
    g.set_import_parent_selection(0);
    g.set_import_parent_names(strings(std::iter::once("请选择原支出".to_owned()).chain(
        review.parents.iter().map(|t| {
            format!(
                "{} · {} · {}",
                t.occurred_at.with_timezone(&Local).format("%Y-%m-%d"),
                t.merchant.as_deref().unwrap_or("支出"),
                t.report_amount_cny
                    .as_ref()
                    .map(Money::decimal_string)
                    .unwrap_or_default()
            )
        }),
    )));
    g.set_import_merge_names(strings(review.merge_targets.iter().map(|t| {
        format!(
            "{} · {} · {} · {:?}",
            t.occurred_at.with_timezone(&Local).format("%m-%d %H:%M"),
            t.merchant.as_deref().unwrap_or("交易"),
            t.report_amount_cny
                .as_ref()
                .map(Money::decimal_string)
                .unwrap_or_default(),
            t.status
        )
    })));
    g.set_import_ready(review.included.iter().any(|v| *v));
}
/// Restoring swaps the identity of the ledger, so no index or editable field
/// from its predecessor may be reused against the replacement catalogue.
fn reset_ledger_state(w: &LedgerWindow) {
    let g = w.global::<LedgerState>();
    g.set_delete_transaction_confirmed(false);
    g.set_delete_batch_confirmed(false);
    g.set_delete_all_confirmed(false);
    g.set_rule_selection(0);
    g.set_rule_merchant("".into());
    g.set_rule_keyword("".into());
    g.set_rule_category(0);
    g.set_rule_priority("10".into());
    g.set_rule_enabled(true);
    g.set_from_date("".into());
    g.set_until_date("".into());
    g.set_filter_source(0);
    g.set_filter_account(0);
    g.set_filter_category(0);
    g.set_filter_status(0);
    g.set_editing_pending(false);
    g.set_editing_new(false);
    g.set_editor_title("".into());
    g.set_editor_hint("".into());
    g.set_editor_amount("".into());
    g.set_editor_report("".into());
    g.set_editor_date("".into());
    g.set_editor_merchant("".into());
    g.set_editor_memo("".into());
    g.set_editor_account(0);
    g.set_editor_destination(0);
    g.set_editor_kind(0);
    g.set_editor_category(0);
    g.set_editor_refund(0);
    g.set_account_name("".into());
    g.set_account_opening("0.00".into());
    g.set_account_date(Local::now().format("%Y-%m-%d").to_string().into());
    g.set_account_kind(0);
    g.set_account_currency(0);
    g.set_category_selection(0);
    g.set_category_name("".into());
    g.set_category_enabled(true);
    g.set_import_path("".into());
    g.set_import_source(0);
    g.set_import_account(0);
    g.set_import_ready(false);
    g.set_import_lines(strings(Vec::new()));
    g.set_import_row_names(strings(Vec::new()));
    g.set_import_parent_names(strings(Vec::new()));
    g.set_import_row_selection(0);
    g.set_import_parent_selection(0);
    g.set_import_row_included(true);
    g.set_import_merge_names(strings(Vec::new()));
    g.set_import_merge_selection(0);
}
fn read_bounded(path: &str, limit: u64) -> Result<Vec<u8>> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err("文件超过大小限制".into());
    }
    Ok(bytes)
}
fn feedback(w: &LedgerWindow, result: Result<()>, success: &str) {
    w.global::<LedgerState>().set_message(
        match result {
            Ok(()) => success.to_owned(),
            Err(e) => e.to_string(),
        }
        .into(),
    );
}
macro_rules! bind {($w:expr,$c:expr,$method:ident,|$win:ident,$state:ident $(,$arg:ident)*|$body:block)=>{{let weak=$w.as_weak();let controller=$c.clone();$w.global::<LedgerState>().$method(move |$($arg),*|{if let Some($win)=weak.upgrade(){let mut state_ref=controller.borrow_mut();let $state=&mut *state_ref;$win.global::<LedgerState>().set_message("".into());#[allow(clippy::redundant_closure_call)] let outcome=(||->Result<()> { $body })();if outcome.is_err() { feedback(&$win,outcome,""); }}});}};}
pub fn connect(w: &LedgerWindow, app: App) -> Result<()> {
    let snapshot = app.snapshot_in_timezone(
        Utc::now(),
        *Local::now().offset(),
        TransactionFilter::default(),
    )?;
    let settings = app.settings()?;
    let controller = Rc::new(RefCell::new(Controller {
        app,
        snapshot,
        filter: TransactionFilter::default(),
        editing: None,
        import: None,
        delete_confirmation: None,
        #[cfg(target_os = "android")]
        document_timer: slint::Timer::default(),
    }));
    #[cfg(target_os = "android")]
    {
        let weak = w.as_weak();
        controller.borrow().document_timer.start(
            slint::TimerMode::Repeated,
            std::time::Duration::from_millis(300),
            move || {
                let Some(w) = weak.upgrade() else {
                    return;
                };
                if let Ok(result) = crate::document_result()
                    && !result.is_empty()
                {
                    let g = w.global::<LedgerState>();
                    if let Some(path) = result.strip_prefix("backup:") {
                        g.set_backup_path(path.into());
                        g.set_backup_confirmed(false);
                        g.set_message("文件已选择。请输入密码并确认覆盖恢复".into());
                    } else if let Some(path) = result.strip_prefix("csv:") {
                        g.set_import_path(path.into());
                        g.set_import_ready(false);
                        g.set_message("文件已选择，请预览后确认导入".into());
                    } else {
                        g.set_message(
                            result
                                .split_once(':')
                                .map(|(_, m)| m)
                                .unwrap_or(&result)
                                .into(),
                        );
                    }
                }
            },
        );
    }
    let g = w.global::<LedgerState>();
    g.set_android_platform(cfg!(target_os = "android"));
    g.set_account_date(Local::now().format("%Y-%m-%d").to_string().into());
    g.set_import_wechat(settings.import_wechat);
    g.set_import_alipay(settings.import_alipay);
    g.set_import_bank(settings.import_bank);
    g.set_backup_enabled(settings.backup_enabled);
    g.set_api_enabled(settings.api_enabled);
    g.set_api_wechat(settings.api_wechat);
    g.set_api_alipay(settings.api_alipay);
    g.set_api_bank(settings.api_bank);
    g.set_default_account(account_index(
        &controller.borrow(),
        settings.default_account,
    ));
    refresh(w, &mut controller.borrow_mut())?;
    bind!(w, controller, on_refresh, |w, c| { refresh(&w, c) });
    bind!(w, controller, on_navigate, |w, c, page| {
        refresh(&w, c)?;
        w.global::<LedgerState>().set_page(page);
        if page != 4 {
            w.global::<LedgerState>().set_backup_password("".into());
            w.global::<LedgerState>().set_backup_confirmed(false);
        }
        Ok(())
    });
    bind!(w, controller, on_new_transaction, |w, c| {
        editor(&w, c, None)
    });
    bind!(w, controller, on_edit, |w, c, index, pending| {
        let row = if pending {
            c.snapshot.pending.get(index as usize)
        } else {
            c.snapshot.transactions.get(index as usize)
        }
        .cloned()
        .ok_or("Transaction no longer available")?;
        editor(&w, c, Some(row))
    });
    bind!(w, controller, on_ignore, |w, c, index| {
        let id = c
            .snapshot
            .pending
            .get(index as usize)
            .ok_or("Pending item no longer available")?
            .transaction
            .id;
        c.app.ignore(id)?;
        refresh(&w, c)
    });
    bind!(w, controller, on_apply_filters, |w, c| {
        let g = w.global::<LedgerState>();
        c.filter = TransactionFilter {
            from: if g.get_from_date().is_empty() {
                None
            } else {
                Some(date(&g.get_from_date())?)
            },
            until: if g.get_until_date().is_empty() {
                None
            } else {
                Some(date(&g.get_until_date())? + chrono::Duration::days(1))
            },
            source: match g.get_filter_source() {
                1 => Some("manual"),
                2 => Some("wechat"),
                3 => Some("alipay"),
                4 => Some("bank"),
                5 => Some("notification"),
                _ => None,
            }
            .map(str::to_owned),
            account_id: account_id(c, g.get_filter_account()),
            category_id: category_id(c, g.get_filter_category()),
            status: match g.get_filter_status() {
                1 => Some(TransactionStatus::Pending),
                2 => Some(TransactionStatus::Confirmed),
                3 => Some(TransactionStatus::Ignored),
                _ => None,
            },
        };
        refresh(&w, c)
    });
    bind!(w, controller, on_create_account, |w, c| {
        let g = w.global::<LedgerState>();
        let (code, exp) = match g.get_account_currency() {
            1 => ("USD", 2),
            2 => ("EUR", 2),
            3 => ("JPY", 0),
            4 => ("HKD", 2),
            _ => ("CNY", 2),
        };
        let currency = CurrencyCode::new(code, exp)?;
        c.app.create_account(
            &g.get_account_name(),
            if g.get_account_kind() == 1 {
                AccountKind::Liability
            } else {
                AccountKind::Asset
            },
            parse_money(&g.get_account_opening(), &currency)?,
            date(&g.get_account_date())?,
        )?;
        g.set_account_name("".into());
        refresh(&w, c)
    });
    bind!(w, controller, on_select_category, |w, c, index| {
        if let Some(category) = usize::try_from(index - 1)
            .ok()
            .and_then(|i| c.snapshot.categories.get(i))
        {
            let g = w.global::<LedgerState>();
            g.set_category_name(category.name.clone().into());
            g.set_category_enabled(category.enabled);
        }
        Ok(())
    });
    bind!(w, controller, on_save_category, |w, c, new| {
        let g = w.global::<LedgerState>();
        if new {
            c.app.create_category(&g.get_category_name())?;
        } else {
            c.app.change_category(
                category_id(c, g.get_category_selection()).ok_or("Select a category")?,
                &g.get_category_name(),
                g.get_category_enabled(),
            )?;
        }
        refresh(&w, c)
    });
    bind!(w, controller, on_confirm_deletion, |w, c, kind, checked| {
        clear_delete_confirmation(&w, c);
        if checked {
            c.delete_confirmation = Some(deletion_scope(&w, c, kind)?);
            let g = w.global::<LedgerState>();
            match kind {
                0 => g.set_delete_transaction_confirmed(true),
                1 => g.set_delete_batch_confirmed(true),
                2 => g.set_delete_all_confirmed(true),
                _ => unreachable!(),
            }
        }
        Ok(())
    });
    bind!(w, controller, on_select_delete_batch, |w, c, index| {
        clear_delete_confirmation(&w, c);
        w.global::<LedgerState>().set_batch_selection(index);
        Ok(())
    });
    bind!(w, controller, on_delete_data, |w, c, kind| {
        let g = w.global::<LedgerState>();
        let confirmed = c.delete_confirmation;
        clear_delete_confirmation(&w, c);
        let scope = deletion_scope(&w, c, kind)?;
        if confirmed != Some(scope) {
            return Err("删除前请为当前范围重新勾选明确确认".into());
        }
        if kind == 2 {
            crate::local_deletion(false)?;
        }
        c.app.delete_data(scope, true)?;
        c.editing = None;
        c.import = None;
        c.filter = TransactionFilter::default();
        reset_ledger_state(&w);
        refresh(&w, c)?;
        if kind == 2 {
            g.set_import_wechat(false);
            g.set_import_alipay(false);
            g.set_import_bank(false);
            g.set_api_enabled(false);
            g.set_api_wechat(false);
            g.set_api_alipay(false);
            g.set_api_bank(false);
            g.set_backup_enabled(false);
            g.set_default_account(0);
            g.set_backup_password("".into());
            g.set_backup_path("".into());
            g.set_backup_confirmed(false);
            crate::local_deletion(true)?;
        }
        g.set_page(if kind == 0 { 1 } else { 4 });
        g.set_message("删除完成".into());
        Ok(())
    });
    bind!(w, controller, on_select_rule, |w, c, index| {
        let rule = c
            .app
            .repository()
            .list_rules()?
            .get(index as usize)
            .cloned()
            .ok_or("请选择规则")?;
        let g = w.global::<LedgerState>();
        g.set_rule_merchant(rule.merchant_contains.unwrap_or_default().into());
        g.set_rule_keyword(rule.description_contains.unwrap_or_default().into());
        g.set_rule_priority(rule.priority.to_string().into());
        g.set_rule_category(category_index(c, Some(rule.category_id)));
        g.set_rule_enabled(rule.enabled);
        Ok(())
    });
    bind!(w, controller, on_save_rule, |w, c, new| {
        let g = w.global::<LedgerState>();
        let id = if new {
            RuleId::new()
        } else {
            c.app
                .repository()
                .list_rules()?
                .get(g.get_rule_selection() as usize)
                .ok_or("请选择规则")?
                .id
        };
        let text = |v: SharedString| {
            if v.trim().is_empty() {
                None
            } else {
                Some(v.trim().to_string())
            }
        };
        c.app.save_rule(
            ClassificationRule {
                id,
                priority: g.get_rule_priority().parse()?,
                merchant_contains: text(g.get_rule_merchant()),
                description_contains: text(g.get_rule_keyword()),
                category_id: category_id(c, g.get_rule_category()).ok_or("请选择分类")?,
                enabled: g.get_rule_enabled(),
            },
            new,
        )?;
        refresh(&w, c)
    });
    bind!(w, controller, on_save_settings, |w, c| {
        let g = w.global::<LedgerState>();
        let mut s = c.app.settings()?;
        s.import_wechat = g.get_import_wechat();
        s.import_alipay = g.get_import_alipay();
        s.import_bank = g.get_import_bank();
        s.backup_enabled = g.get_backup_enabled();
        s.api_enabled = g.get_api_enabled();
        s.api_wechat = g.get_api_wechat();
        s.api_alipay = g.get_api_alipay();
        s.api_bank = g.get_api_bank();
        s.default_account = account_id(c, g.get_default_account());
        c.app.save_settings(s)?;
        Ok(())
    });
    bind!(w, controller, on_choose_backup, |w, _c| {
        #[cfg(target_os = "android")]
        {
            crate::choose_document(2, &[])?;
        }
        #[cfg(not(target_os = "android"))]
        {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("加密账本", &["meowm"])
                .pick_file()
            {
                w.global::<LedgerState>()
                    .set_backup_path(path.to_string_lossy().as_ref().into());
                w.global::<LedgerState>().set_backup_confirmed(false);
            }
        }
        Ok(())
    });
    bind!(w, controller, on_choose_csv, |w, c| {
        c.import = None;
        w.global::<LedgerState>().set_import_ready(false);
        #[cfg(target_os = "android")]
        {
            crate::choose_document(3, &[])?;
        }
        #[cfg(not(target_os = "android"))]
        {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("CSV账单", &["csv"])
                .pick_file()
            {
                w.global::<LedgerState>()
                    .set_import_path(path.to_string_lossy().as_ref().into());
            }
        }
        Ok(())
    });
    bind!(w, controller, on_export_backup, |w, c| {
        let g = w.global::<LedgerState>();
        let password = zeroize::Zeroizing::new(g.get_backup_password().to_string());
        g.set_backup_password("".into());
        let confirmed = g.get_backup_confirmed();
        g.set_backup_confirmed(false);
        let bytes = c
            .app
            .export_backup(&ledger_backup::EncryptedBackup, &password, confirmed)?;
        #[cfg(target_os = "android")]
        {
            crate::choose_document(1, &bytes)?;
            g.set_message("请选择备份保存位置".into());
        }
        #[cfg(not(target_os = "android"))]
        {
            let path = if g.get_backup_path().is_empty() {
                let Some(path) = rfd::FileDialog::new()
                    .set_file_name("local-ledger.meowm")
                    .save_file()
                else {
                    return Ok(());
                };
                path
            } else {
                std::path::PathBuf::from(g.get_backup_path().as_str())
            };
            if let Some(database) = c.app.repository().database_path()
                && path.canonicalize().ok() == std::path::Path::new(database).canonicalize().ok()
                && path.exists()
            {
                return Err("备份不能覆盖正在使用的账本数据库".into());
            }
            use std::io::Write;
            let parent = path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(std::path::Path::new("."));
            let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
            temporary.write_all(&bytes)?;
            temporary.as_file().sync_all()?;
            temporary.persist(path)?;
            g.set_message("已导出加密备份".into());
        }
        Ok(())
    });
    bind!(w, controller, on_restore_backup, |w, c| {
        let g = w.global::<LedgerState>();
        let password = zeroize::Zeroizing::new(g.get_backup_password().to_string());
        g.set_backup_password("".into());
        let confirmed = g.get_backup_confirmed();
        g.set_backup_confirmed(false);
        if !confirmed {
            return Err("恢复前请勾选覆盖确认".into());
        }
        if g.get_backup_path().is_empty() {
            return Err("请选择备份文件".into());
        }
        let bytes = read_bounded(g.get_backup_path().as_str(), 64 * 1024 * 1024 + 128)?;
        c.app
            .restore_backup(&ledger_backup::EncryptedBackup, &bytes, &password, true)?;
        c.editing = None;
        c.import = None;
        c.filter = TransactionFilter::default();
        reset_ledger_state(&w);
        let settings = c.app.settings()?;
        g.set_import_wechat(settings.import_wechat);
        g.set_import_alipay(settings.import_alipay);
        g.set_import_bank(settings.import_bank);
        g.set_backup_enabled(settings.backup_enabled);
        g.set_api_enabled(settings.api_enabled);
        g.set_api_wechat(settings.api_wechat);
        g.set_api_alipay(settings.api_alipay);
        g.set_api_bank(settings.api_bank);
        g.set_import_ready(false);
        g.set_import_lines(strings(Vec::new()));
        refresh(&w, c)?;
        g.set_default_account(account_index(c, settings.default_account));
        g.set_message("已验证并恢复备份".into());
        #[cfg(target_os = "android")]
        {
            let _ = std::fs::remove_file(g.get_backup_path().as_str());
            g.set_backup_path("".into());
        }
        Ok(())
    });
    bind!(w, controller, on_notification_settings, |_w, _c| {
        crate::open_notification_settings().map_err(Into::into)
    });
    bind!(w, controller, on_preview_import, |w, c| {
        let g = w.global::<LedgerState>();
        g.set_import_ready(false);
        c.import = None;
        let account_id =
            account_id(c, g.get_import_account()).ok_or("Select the target account")?;
        let account = c
            .app
            .repository()
            .get_account(account_id)?
            .ok_or("Account missing")?;
        let settings = c.app.settings()?;
        let is_wechat = g.get_import_source() == 0;
        if if is_wechat {
            !settings.import_wechat
        } else {
            !settings.import_alipay
        } {
            return Err("Enable this import source in Settings and save preferences first".into());
        }
        let bytes = read_bounded(g.get_import_path().as_str(), 32 * 1024 * 1024);
        #[cfg(target_os = "android")]
        {
            let _ = std::fs::remove_file(g.get_import_path().as_str());
            g.set_import_path("".into());
        }
        let bytes = bytes?;
        let parsed = if is_wechat {
            ledger_import::parse_wechat_csv(&bytes)?
        } else {
            ledger_import::parse_alipay_csv(&bytes)?
        };
        let mut review =
            crate::import_review::ImportReview::new(parsed, account, c.app.list_import_records()?)?;
        review.add_existing_parents(c.app.repository().list_transactions()?);
        for candidate in &mut review.preview.candidates {
            candidate.transaction.category_id = ledger_app::classification::suggest_category(
                &candidate.transaction,
                &c.app.repository().list_rules()?,
                &c.app.repository().list_categories()?,
            );
        }
        let preview = &review.preview;
        render_import(&w, &review);
        g.set_import_ready(preview.counts.new > 0);
        c.import = Some(review);
        #[cfg(target_os = "android")]
        {
            let _ = std::fs::remove_file(g.get_import_path().as_str());
            g.set_import_path("".into());
        }
        Ok(())
    });
    bind!(w, controller, on_select_import_row, |w, c, index| {
        let review = c.import.as_ref().ok_or("请先预览文件")?;
        w.global::<LedgerState>()
            .set_import_row_included(*review.included.get(index as usize).ok_or("请选择账单行")?);
        Ok(())
    });
    bind!(
        w,
        controller,
        on_include_import_row,
        |w, c, index, included| {
            let review = c.import.as_mut().ok_or("请先预览文件")?;
            review.set_included(index as usize, included)?;
            render_import(&w, review);
            Ok(())
        }
    );
    bind!(
        w,
        controller,
        on_map_import_refund,
        |w, c, index, parent| {
            let review = c.import.as_mut().ok_or("请先预览文件")?;
            let parent = usize::try_from(parent - 1).map_err(|_| "请选择原支出")?;
            review.map_refund(index as usize, parent)?;
            render_import(&w, review);
            Ok(())
        }
    );
    bind!(w, controller, on_resolve_import, |w, c, merge| {
        let g = w.global::<LedgerState>();
        let review = c.import.as_mut().ok_or("请先预览")?;
        if merge {
            review.resolve_merge(
                g.get_import_row_selection() as usize,
                g.get_import_merge_selection() as usize,
            )?;
        } else {
            review.resolve_distinct(g.get_import_row_selection() as usize)?;
        }
        render_import(&w, review);
        Ok(())
    });
    bind!(w, controller, on_confirm_import, |w, c| {
        let review = c.import.as_ref().ok_or("Preview a file first")?;
        let settings = c.app.settings()?;
        for row in &review.preview.candidates {
            if match row.source {
                ledger_import::ImportSource::WeChat => !settings.import_wechat,
                ledger_import::ImportSource::Alipay => !settings.import_alipay,
                ledger_import::ImportSource::BankCsv => !settings.import_bank,
            } {
                return Err("该导入来源已关闭，请重新启用后确认".into());
            }
        }
        review.confirm(&c.app)?;
        c.import = None;
        let g = w.global::<LedgerState>();
        g.set_import_ready(false);
        refresh(&w, c)?;
        g.set_page(1);
        Ok(())
    });
    bind!(w, controller, on_save_editor, |w, c| {
        let g = w.global::<LedgerState>();
        let category = category_id(c, g.get_editor_category());
        let memo = Some(g.get_editor_memo().to_string());
        if let Some(row) = &c.editing
            && row.transaction.status != TransactionStatus::Pending
        {
            c.app.edit_annotations(row.transaction.id, category, memo)?;
            refresh(&w, c)?;
            g.set_page(1);
            return Ok(());
        }
        let account =
            account_id(c, g.get_editor_account()).ok_or("Select an account before confirming")?;
        let a = c
            .snapshot
            .accounts
            .iter()
            .find(|a| a.account.id == account)
            .ok_or("Account missing")?;
        let amount = parse_money(&g.get_editor_amount(), &a.account.currency)?;
        let kind = match g.get_editor_kind() {
            1 => TransactionKind::Income,
            2 => TransactionKind::Transfer,
            3 => TransactionKind::Refund,
            _ => TransactionKind::Expense,
        };
        let cny = CurrencyCode::new("CNY", 2)?;
        let report = if kind == TransactionKind::Transfer {
            None
        } else if a.account.currency.code() == "CNY" {
            Some(amount.clone())
        } else {
            Some(parse_money(&g.get_editor_report(), &cny)?)
        };
        let selected_date = g.get_editor_date().to_string();
        let occurred_at = c
            .editing
            .as_ref()
            .map(|r| r.transaction.occurred_at)
            .filter(|t| t.with_timezone(&Local).format("%Y-%m-%d").to_string() == selected_date)
            .unwrap_or(
                if selected_date == Local::now().format("%Y-%m-%d").to_string() {
                    Utc::now()
                } else {
                    date(&selected_date)? + chrono::Duration::hours(12)
                },
            );
        let originals = c
            .app
            .repository()
            .list_transactions()?
            .into_iter()
            .filter(|t| {
                t.status == TransactionStatus::Confirmed && t.kind == TransactionKind::Expense
            })
            .collect::<Vec<_>>();
        let original = usize::try_from(g.get_editor_refund() - 1)
            .ok()
            .and_then(|i| originals.get(i))
            .map(|t| t.id);
        let draft = TransactionDraft {
            occurred_at,
            kind,
            account_id: account,
            destination_id: account_id(c, g.get_editor_destination()),
            amount,
            report_amount_cny: report,
            category_id: category,
            original_transaction_id: original,
            merchant: Some(g.get_editor_merchant().to_string()),
            memo,
        };
        if let Some(row) = &c.editing {
            c.app.confirm_pending(row.transaction.id, draft)?;
        } else {
            c.app.create_transaction(draft)?;
        }
        c.editing = None;
        refresh(&w, c)?;
        g.set_page(0);
        Ok(())
    });
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decimal_input_never_rounds_or_overflows() {
        let c = CurrencyCode::new("CNY", 2).unwrap();
        assert_eq!(parse_money("12.34", &c).unwrap().minor_units(), 1234);
        assert!(parse_money("1.001", &c).is_err());
        assert!(parse_money("NaN", &c).is_err());
        assert!(parse_money("92233720368547758.08", &c).is_err());
        let j = CurrencyCode::new("JPY", 0).unwrap();
        assert!(parse_money("1.1", &j).is_err());
    }
}

#[cfg(test)]
mod interaction_tests {
    use super::*;
    use slint::Model;
    struct TestPlatform;
    impl slint::platform::Platform for TestPlatform {
        fn create_window_adapter(
            &self,
        ) -> std::result::Result<Rc<dyn slint::platform::WindowAdapter>, slint::PlatformError>
        {
            Ok(
                slint::platform::software_renderer::MinimalSoftwareWindow::new(
                    slint::platform::software_renderer::RepaintBufferType::NewBuffer,
                ),
            )
        }
    }
    #[test]
    fn confirmed_row_displays_edited_posting_amount_instead_of_original_capture() {
        let cny = CurrencyCode::new("CNY", 2).unwrap();
        let row = TransactionRow {
            transaction: Transaction {
                id: TransactionId::new(),
                occurred_at: Utc::now(),
                kind: TransactionKind::Expense,
                status: TransactionStatus::Confirmed,
                postings: vec![Posting {
                    account_id: AccountId::new(),
                    delta: Money::new(-1500, cny.clone()),
                }],
                category_id: None,
                original_transaction_id: None,
                report_amount_cny: Some(Money::new(1500, cny.clone())),
                merchant: None,
                description: None,
            },
            source: "wechat".into(),
            details: Some(PendingDetails {
                amount: Some(Money::new(1234, cny)),
                ..Default::default()
            }),
        };
        assert_eq!(view_row(&row).amount.as_str(), "CNY 15.00");
    }
    #[test]
    fn slint_callbacks_create_account_record_expense_filter_and_update_category() {
        slint::platform::set_platform(Box::new(TestPlatform)).unwrap();
        let w = LedgerWindow::new().unwrap();
        let app = App::new(SqliteLedgerRepository::in_memory().unwrap());
        connect(&w, app).unwrap();
        let g = w.global::<LedgerState>();
        g.set_account_name("Cash".into());
        g.set_account_opening("100.00".into());
        g.set_account_date("2020-01-01".into());
        g.invoke_create_account();
        assert!(g.get_message().is_empty(), "{}", g.get_message());
        assert_eq!(g.get_accounts().row_count(), 1);
        g.set_category_name("Food".into());
        g.invoke_save_category(true);
        g.invoke_new_transaction();
        g.set_editor_account(1);
        g.set_editor_amount("12.34".into());
        g.set_editor_category(1);
        g.set_editor_merchant("Cafe".into());
        g.set_editor_date(
            (Local::now() - chrono::Duration::days(1))
                .format("%Y-%m-%d")
                .to_string()
                .into(),
        );
        g.invoke_save_editor();
        assert!(g.get_message().is_empty(), "{}", g.get_message());
        assert_eq!(g.get_transactions().row_count(), 1);
        assert!(g.get_accounts().row_data(0).unwrap().contains("87.66"));
        g.invoke_edit(0, false);
        g.set_editor_memo("Lunch".into());
        g.invoke_save_editor();
        assert!(
            g.get_transactions()
                .row_data(0)
                .unwrap()
                .subtitle
                .contains("Lunch")
        );
        g.set_filter_status(1);
        g.invoke_apply_filters();
        assert_eq!(g.get_transactions().row_count(), 0);
    }
    #[test]
    fn pending_editor_corrects_cny_report_when_amount_changes() {
        slint::platform::set_platform(Box::new(TestPlatform)).unwrap();
        let w = LedgerWindow::new().unwrap();
        let app = App::new(SqliteLedgerRepository::in_memory().unwrap());
        let cny = CurrencyCode::new("CNY", 2).unwrap();
        app.create_account(
            "Cash",
            AccountKind::Asset,
            Money::new(10000, cny.clone()),
            "2020-01-01T00:00:00Z".parse().unwrap(),
        )
        .unwrap();
        app.repository()
            .insert_transaction(Transaction {
                id: TransactionId::new(),
                occurred_at: Utc::now(),
                kind: TransactionKind::Expense,
                status: TransactionStatus::Pending,
                postings: vec![],
                category_id: None,
                original_transaction_id: None,
                report_amount_cny: Some(Money::new(1234, cny)),
                merchant: None,
                description: None,
            })
            .unwrap();
        connect(&w, app).unwrap();
        let g = w.global::<LedgerState>();
        g.invoke_edit(0, true);
        assert_eq!(g.get_editor_report().as_str(), "12.34");
        g.set_editor_account(1);
        g.set_editor_amount("15.00".into());
        g.invoke_save_editor();
        assert!(g.get_message().is_empty(), "{}", g.get_message());
        assert_eq!(g.get_expense().as_str(), "15.00");
        assert!(g.get_accounts().row_data(0).unwrap().contains("85.00"));
    }
    #[test]
    fn deletion_ui_requires_fresh_confirmation_and_resets_visible_data() {
        slint::platform::set_platform(Box::new(TestPlatform)).unwrap();
        let w = LedgerWindow::new().unwrap();
        let app = App::new(SqliteLedgerRepository::in_memory().unwrap());
        app.create_category("delete").unwrap();
        connect(&w, app).unwrap();
        let g = w.global::<LedgerState>();
        g.invoke_delete_data(2);
        assert_eq!(g.get_category_names().row_count(), 2);
        g.invoke_confirm_deletion(2, true);
        g.invoke_delete_data(2);
        assert_eq!(g.get_category_names().row_count(), 1);
        assert!(!g.get_delete_all_confirmed());
    }
    #[test]
    fn cancelling_transaction_deletion_does_not_authorize_deleting_all_data() {
        slint::platform::set_platform(Box::new(TestPlatform)).unwrap();
        let w = LedgerWindow::new().unwrap();
        let app = App::new(SqliteLedgerRepository::in_memory().unwrap());
        app.create_category("Keep this category").unwrap();
        app.repository()
            .insert_transaction(Transaction {
                id: TransactionId::new(),
                occurred_at: Utc::now(),
                kind: TransactionKind::Expense,
                status: TransactionStatus::Pending,
                postings: vec![],
                category_id: None,
                original_transaction_id: None,
                report_amount_cny: Some(Money::new(100, CurrencyCode::new("CNY", 2).unwrap())),
                merchant: Some("Keep this transaction".into()),
                description: None,
            })
            .unwrap();
        connect(&w, app).unwrap();
        let g = w.global::<LedgerState>();
        g.invoke_edit(0, true);
        g.invoke_confirm_deletion(0, true);
        g.invoke_navigate(2);
        g.invoke_navigate(4);
        g.invoke_delete_data(2);
        assert_eq!(g.get_category_names().row_count(), 2);
        assert_eq!(g.get_pending().row_count(), 1);
        assert!(!g.get_message().is_empty());
        g.invoke_edit(0, true);
        g.invoke_confirm_deletion(0, true);
        g.invoke_delete_data(0);
        assert_eq!(g.get_pending().row_count(), 0);
        assert_eq!(g.get_category_names().row_count(), 2);
        assert!(!g.get_delete_transaction_confirmed());
    }
    #[test]
    fn batch_deletion_requires_the_confirmed_id_and_fresh_selection() {
        slint::platform::set_platform(Box::new(TestPlatform)).unwrap();
        let w = LedgerWindow::new().unwrap();
        let app = App::new(SqliteLedgerRepository::in_memory().unwrap());
        let id = app
            .create_account(
                "Cash",
                AccountKind::Asset,
                Money::new(10000, CurrencyCode::new("CNY", 2).unwrap()),
                "2020-01-01T00:00:00Z".parse().unwrap(),
            )
            .unwrap();
        let account = app.repository().get_account(id).unwrap().unwrap();
        for (external_id, amount) in [("batch-a", 20), ("batch-b", 5)] {
            let csv = format!(
                "交易时间,交易类型,交易对方,商品,收/支,金额(元),支付方式,当前状态,交易单号,商户单号,备注,原交易单号\n2026-10-07 10:30:00,商户消费,Cafe,Tea,支出,{amount}.00,零钱,支付成功,{external_id},,,\n"
            );
            crate::import_review::ImportReview::new(
                ledger_import::parse_wechat_csv(csv.as_bytes()).unwrap(),
                account.clone(),
                app.list_import_records().unwrap(),
            )
            .unwrap()
            .confirm(&app)
            .unwrap();
        }
        connect(&w, app).unwrap();
        let g = w.global::<LedgerState>();
        assert_eq!(g.get_batch_names().row_count(), 2);
        g.invoke_select_delete_batch(0);
        g.invoke_confirm_deletion(1, true);
        // Even a property update that bypasses selection callback cannot
        // apply one batch's authorization to another ID.
        g.set_batch_selection(1);
        g.invoke_delete_data(1);
        assert!(!g.get_message().is_empty());
        assert_eq!(g.get_batch_names().row_count(), 2);
        g.invoke_select_delete_batch(0);
        g.invoke_confirm_deletion(1, true);
        g.invoke_select_delete_batch(1);
        g.invoke_select_delete_batch(0);
        assert!(!g.get_delete_batch_confirmed());
        g.invoke_delete_data(1);
        assert!(!g.get_message().is_empty());
        assert_eq!(g.get_batch_names().row_count(), 2);
        g.invoke_confirm_deletion(1, true);
        g.invoke_delete_data(2);
        assert!(!g.get_message().is_empty());
        assert_eq!(g.get_batch_names().row_count(), 2);
        g.invoke_confirm_deletion(1, true);
        g.invoke_refresh();
        g.invoke_delete_data(1);
        assert!(!g.get_message().is_empty());
        let retained = g.get_batch_names().row_data(1).unwrap();
        g.invoke_confirm_deletion(1, true);
        g.invoke_delete_data(1);
        assert_eq!(g.get_batch_names().row_count(), 1);
        assert_eq!(g.get_batch_names().row_data(0).unwrap(), retained);
        assert_eq!(g.get_transactions().row_count(), 1);
    }
    #[test]
    fn changing_refund_parent_catalog_requires_a_fresh_ui_selection() {
        slint::platform::set_platform(Box::new(TestPlatform)).unwrap();
        let w = LedgerWindow::new().unwrap();
        let app = App::new(SqliteLedgerRepository::in_memory().unwrap());
        app.create_account(
            "Cash",
            AccountKind::Asset,
            Money::new(10000, CurrencyCode::new("CNY", 2).unwrap()),
            "2020-01-01T00:00:00Z".parse().unwrap(),
        )
        .unwrap();
        app.save_settings(MobileSettings {
            import_wechat: true,
            ..Default::default()
        })
        .unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("refund.csv");
        std::fs::write(&path, "交易时间,交易类型,交易对方,商品,收/支,金额(元),支付方式,当前状态,交易单号,商户单号,备注,原交易单号\n2026-10-07 10:30:00,商户消费,A,Tea,支出,20.00,零钱,支付成功,a,,,\n2026-10-07 10:30:00,商户消费,B,Tea,支出,5.00,零钱,支付成功,b,,,\n2026-10-07 11:30:00,退款,B,Tea,收入,1.00,零钱,退款成功,refund,,,b\n").unwrap();
        connect(&w, app).unwrap();
        let g = w.global::<LedgerState>();
        g.set_import_account(1);
        g.set_import_path(path.to_string_lossy().as_ref().into());
        g.invoke_preview_import();
        assert!(g.get_message().is_empty(), "{}", g.get_message());
        assert_eq!(g.get_import_parent_names().row_count(), 3);
        g.set_import_parent_selection(1);
        g.invoke_include_import_row(0, false);
        assert_eq!(g.get_import_parent_selection(), 0);
        assert_eq!(g.get_import_parent_names().row_count(), 2);
        g.invoke_map_import_refund(2, g.get_import_parent_selection());
        assert!(!g.get_message().is_empty());
        g.set_import_parent_selection(1);
        g.invoke_map_import_refund(2, g.get_import_parent_selection());
        assert!(g.get_message().is_empty(), "{}", g.get_message());
        g.invoke_confirm_import();
        assert!(g.get_message().is_empty(), "{}", g.get_message());
        assert_eq!(g.get_transactions().row_count(), 2);
        assert!(g.get_accounts().row_data(0).unwrap().contains("96.00"));
    }
    #[test]
    fn rule_management_creates_updates_and_disables_real_rules() {
        slint::platform::set_platform(Box::new(TestPlatform)).unwrap();
        let w = LedgerWindow::new().unwrap();
        let app = App::new(SqliteLedgerRepository::in_memory().unwrap());
        app.create_category("Food").unwrap();
        connect(&w, app).unwrap();
        let g = w.global::<LedgerState>();
        g.set_rule_merchant("Cafe".into());
        g.set_rule_category(1);
        g.set_rule_priority("10".into());
        g.invoke_save_rule(true);
        assert_eq!(g.get_rule_names().row_count(), 1);
        g.set_rule_selection(0);
        g.invoke_select_rule(0);
        assert_eq!(g.get_rule_merchant().as_str(), "Cafe");
        g.set_rule_enabled(false);
        g.invoke_save_rule(false);
        assert!(g.get_rule_names().row_data(0).unwrap().contains("停用"));
    }
    #[test]
    fn backup_callbacks_require_confirmation_restore_data_and_clear_passwords() {
        slint::platform::set_platform(Box::new(TestPlatform)).unwrap();
        let w = LedgerWindow::new().unwrap();
        let app = App::new(SqliteLedgerRepository::in_memory().unwrap());
        app.save_settings(MobileSettings {
            backup_enabled: true,
            ..Default::default()
        })
        .unwrap();
        let archive = app
            .export_backup(&ledger_backup::EncryptedBackup, "test password", true)
            .unwrap();
        let path = std::env::temp_dir().join(format!("{}.meowm", uuid::Uuid::new_v4()));
        std::fs::write(&path, archive).unwrap();
        app.create_category("remove on restore").unwrap();
        connect(&w, app).unwrap();
        let g = w.global::<LedgerState>();
        g.set_backup_path(path.to_string_lossy().as_ref().into());
        g.set_backup_password("test password".into());
        g.invoke_restore_backup();
        assert!(!g.get_message().is_empty());
        assert_eq!(g.get_category_names().row_count(), 2);
        g.set_backup_password("test password".into());
        g.set_backup_confirmed(true);
        g.invoke_restore_backup();
        assert_eq!(g.get_category_names().row_count(), 1);
        assert!(g.get_backup_password().is_empty());
        assert!(!g.get_backup_confirmed());
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn restore_clears_stale_category_editor_before_same_index_can_be_updated() {
        slint::platform::set_platform(Box::new(TestPlatform)).unwrap();
        let w = LedgerWindow::new().unwrap();
        let source = App::new(SqliteLedgerRepository::in_memory().unwrap());
        source
            .save_settings(MobileSettings {
                backup_enabled: true,
                ..Default::default()
            })
            .unwrap();
        source.create_category("restored category").unwrap();
        let path = std::env::temp_dir().join(format!("{}.meowm", uuid::Uuid::new_v4()));
        std::fs::write(
            &path,
            source
                .export_backup(&ledger_backup::EncryptedBackup, "test password", true)
                .unwrap(),
        )
        .unwrap();
        let app = App::new(SqliteLedgerRepository::in_memory().unwrap());
        app.save_settings(MobileSettings {
            backup_enabled: true,
            ..Default::default()
        })
        .unwrap();
        app.create_category("old category").unwrap();
        connect(&w, app).unwrap();
        let g = w.global::<LedgerState>();
        g.set_category_selection(1);
        g.invoke_select_category(1);
        assert_eq!(g.get_category_name().as_str(), "old category");
        g.set_backup_path(path.to_string_lossy().as_ref().into());
        g.set_backup_password("test password".into());
        g.set_backup_confirmed(true);
        g.invoke_restore_backup();
        g.invoke_save_category(false);
        assert_eq!(
            g.get_category_names().row_data(1).unwrap().as_str(),
            "restored category"
        );
        assert!(!g.get_message().is_empty());
        assert_eq!(g.get_category_selection(), 0);
        assert!(g.get_category_name().is_empty());
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn provider_settings_save_independently_and_reopen_with_global_gate() {
        slint::platform::set_platform(Box::new(TestPlatform)).unwrap();
        let path = std::env::temp_dir().join(format!("{}.sqlite3", uuid::Uuid::new_v4()));
        let w = LedgerWindow::new().unwrap();
        connect(&w, App::new(SqliteLedgerRepository::open(&path).unwrap())).unwrap();
        let g = w.global::<LedgerState>();
        assert!(!g.get_api_enabled());
        assert!(!g.get_api_wechat());
        g.set_api_wechat(true);
        g.set_api_bank(true);
        g.invoke_save_settings();
        let settings = App::new(SqliteLedgerRepository::open(&path).unwrap())
            .settings()
            .unwrap();
        assert!(settings.api_wechat && settings.api_bank);
        assert!(!settings.api_enabled && !settings.api_alipay);
        drop(w);
        std::fs::remove_file(path).unwrap();
    }
}
