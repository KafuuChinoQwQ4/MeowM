//! Device-local dates converted once to half-open UTC reporting ranges.
use crate::{AppError, MonthBoundary, ReportQuery, RepositoryError};
use chrono::{Datelike, Duration, NaiveDate, TimeZone, Utc};

pub struct CalendarRange {
    pub query: ReportQuery,
}
pub fn local_midnight<T: TimeZone>(
    date: NaiveDate,
    zone: &T,
) -> Result<crate::Timestamp, AppError> {
    zone.from_local_datetime(&date.and_hms_opt(0, 0, 0).ok_or_else(invalid_range)?)
        .earliest()
        .map(|dt| dt.with_timezone(&Utc))
        .ok_or_else(invalid_range)
}
fn invalid_range() -> AppError {
    RepositoryError::InvalidData("日期范围或本地时间不可用".into()).into()
}
impl CalendarRange {
    pub fn new<T: TimeZone>(from: NaiveDate, until: NaiveDate, zone: T) -> Result<Self, AppError> {
        if from >= until || until.signed_duration_since(from) > Duration::days(3660) {
            return Err(invalid_range());
        }
        let start = local_midnight(from, &zone)?;
        let end = local_midnight(until, &zone)?;
        let mut cursor = from.with_day(1).ok_or_else(invalid_range)?;
        let mut boundaries = Vec::new();
        while cursor < until {
            let next = cursor
                .checked_add_months(chrono::Months::new(1))
                .ok_or_else(invalid_range)?;
            boundaries.push(MonthBoundary::new(
                cursor.format("%Y-%m").to_string(),
                local_midnight(cursor.max(from), &zone)?,
                local_midnight(next.min(until), &zone)?,
            ));
            cursor = next;
        }
        Ok(Self {
            query: ReportQuery {
                from_utc_inclusive: start,
                until_utc_exclusive: end,
                month_boundaries_utc: boundaries,
            },
        })
    }
}
