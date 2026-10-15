//! Advance from the original anchor; short months never move later occurrences earlier.
use super::*;
use chrono::{Days, Months};
use ledger_domain::RecurringCadence;
pub fn next_occurrence(
    cadence: RecurringCadence,
    interval: u32,
    anchor: NaiveDate,
    current: NaiveDate,
) -> Result<NaiveDate, RepositoryError> {
    if !(1..=120).contains(&interval) || current < anchor {
        return Err(invalid("周期日期或间隔无效"));
    }
    let next = match cadence {
        RecurringCadence::Weekly => current.checked_add_days(Days::new(u64::from(interval) * 7)),
        RecurringCadence::Monthly | RecurringCadence::Yearly => {
            let months = interval
                * if cadence == RecurringCadence::Yearly {
                    12
                } else {
                    1
                };
            let first = current
                .with_day(1)
                .and_then(|d| d.checked_add_months(Months::new(months)));
            first.and_then(|d| {
                let month = if cadence == RecurringCadence::Yearly {
                    anchor.month()
                } else {
                    d.month()
                };
                (1..=anchor.day())
                    .rev()
                    .find_map(|day| NaiveDate::from_ymd_opt(d.year(), month, day))
            })
        }
    }
    .ok_or_else(|| invalid("周期日期超出范围"))?;
    if next <= current || next.year() > 9999 {
        return Err(invalid("周期日期超出范围"));
    }
    Ok(next)
}
