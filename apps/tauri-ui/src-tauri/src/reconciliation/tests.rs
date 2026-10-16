use super::*;
use chrono::{Local, TimeZone};
use ledger_app::LedgerRepository;
use ledger_domain::{Account, AccountKind, CurrencyCode};
#[test]
fn ipc_uses_local_day_end_signed_amounts_and_survives_restart() {
    let path = std::env::temp_dir().join(format!(
        "meowm-reconciliation-ipc-{}.sqlite3",
        uuid::Uuid::new_v4()
    ));
    let ledger = Ledger::open(&path).unwrap();
    let mut a = Account::new(
        "对账测试",
        AccountKind::Asset,
        CurrencyCode::new("CNY", 2).unwrap(),
    );
    a.opening_balance = Some(money(-10000));
    a.opening_balance_effective_at = Some(
        Local
            .with_ymd_and_hms(2026, 10, 1, 0, 0, 0)
            .unwrap()
            .with_timezone(&Utc),
    );
    ledger.repo().unwrap().insert_account(a.clone()).unwrap();
    let s = ledger
        .reconciliation_start(&a.id.as_uuid().to_string(), "2026-10-16", "-101.01")
        .unwrap();
    assert_eq!(s.record.expected, "-100.00");
    assert_eq!(s.record.difference, "-1.01");
    assert_eq!(
        s.record.cutoff,
        time(date("2026-10-16").unwrap(), true)
            .unwrap()
            .with_timezone(&Utc)
            .to_rfc3339()
    );
    let id = s.record.id;
    assert!(ledger.reconciliation_adjust(&id, 0, "").is_err());
    ledger
        .reconciliation_adjust(&id, 0, "线下余额核对")
        .unwrap();
    let changed = ledger.reconciliation_summary(&id).unwrap();
    ledger
        .reconciliation_close(&id, changed.record.version)
        .unwrap();
    drop(ledger);
    let ledger = Ledger::open(&path).unwrap();
    let saved = ledger.reconciliation_summary(&id).unwrap();
    assert_eq!(saved.record.status, "closed");
    assert_eq!(saved.record.expected, "-100.00");
    assert_eq!(saved.record.actual, "-101.01");
    assert!(
        saved
            .events
            .iter()
            .any(|e| e.reason.as_deref() == Some("线下余额核对"))
    );
    assert_eq!(
        ledger
            .day(&a.id.as_uuid().to_string(), "2026-10-16")
            .unwrap()
            .expense,
        "0.00"
    );
    assert_eq!(
        ledger
            .day(&a.id.as_uuid().to_string(), "2026-10-16")
            .unwrap()
            .balance
            .as_deref(),
        Some("-101.01")
    );
    drop(ledger);
    std::fs::remove_file(path).unwrap();
}
