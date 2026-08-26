use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;

pub const METHODOLOGY_VERSION: &str = "casp-daily-activity-v1";
pub const CONVERSION_METHODOLOGY: &str = "demo-usd-eur-parity-v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportingEvent {
    pub date_utc: String,
    pub operation_id: String,
    pub classification: String,
    pub value_raw: u64,
    pub fee_raw: u64,
    pub known_onchain_overlap: bool,
}

pub trait ReportingStore: Send + Sync {
    fn events(&self, from: &str, to: &str) -> Result<Vec<ReportingEvent>, ReportingError>;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ClassificationAggregate {
    pub classification: String,
    pub operation_count: u64,
    pub value_raw: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DailyTransactionAggregate {
    pub date_utc: String,
    pub asset_symbol: String,
    pub currency_area: String,
    pub total_operation_count: u64,
    pub total_value_raw: String,
    pub total_value_usd_minor: String,
    pub means_of_exchange_count: u64,
    pub means_of_exchange_value_raw: String,
    pub means_of_exchange_value_usd_minor: String,
    pub means_of_exchange_value_eur_minor: String,
    pub excluded_operation_count: u64,
    pub known_onchain_overlap_count: u64,
    pub known_onchain_overlap_value_raw: String,
    pub classifications: Vec<ClassificationAggregate>,
    pub methodology_version: String,
    pub conversion_methodology: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DailyTransactionReport {
    pub from_date_utc: String,
    pub to_date_utc: String,
    pub days: Vec<DailyTransactionAggregate>,
}

#[derive(Clone)]
pub struct ReportingService {
    store: Arc<dyn ReportingStore>,
}

impl ReportingService {
    pub fn new(store: Arc<dyn ReportingStore>) -> Self {
        Self { store }
    }

    pub fn daily(&self, from: &str, to: &str) -> Result<DailyTransactionReport, ReportingError> {
        validate_date(from)?;
        validate_date(to)?;
        if from > to {
            return Err(ReportingError::InvalidDateRange);
        }
        let events = self.store.events(from, to)?;
        Ok(DailyTransactionReport {
            from_date_utc: from.into(),
            to_date_utc: to.into(),
            days: aggregate(events)?,
        })
    }
}

fn aggregate(
    events: Vec<ReportingEvent>,
) -> Result<Vec<DailyTransactionAggregate>, ReportingError> {
    use std::collections::BTreeMap;
    let mut days: BTreeMap<String, Vec<ReportingEvent>> = BTreeMap::new();
    for event in events {
        days.entry(event.date_utc.clone()).or_default().push(event);
    }
    days.into_iter()
        .map(|(date, events)| aggregate_day(date, events))
        .collect()
}

fn aggregate_day(
    date: String,
    events: Vec<ReportingEvent>,
) -> Result<DailyTransactionAggregate, ReportingError> {
    use std::collections::BTreeMap;
    let mut total_value = 0_u64;
    let mut means_count = 0_u64;
    let mut means_value = 0_u64;
    let mut overlap_count = 0_u64;
    let mut overlap_value = 0_u64;
    let mut classes: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    for event in &events {
        total_value = checked_add(total_value, event.value_raw)?;
        let entry = classes.entry(event.classification.clone()).or_default();
        entry.0 = checked_add(entry.0, 1)?;
        entry.1 = checked_add(entry.1, event.value_raw)?;
        if event.classification == "goods_or_services" {
            means_count = checked_add(means_count, 1)?;
            means_value = checked_add(means_value, event.value_raw)?;
        }
        if event.known_onchain_overlap {
            overlap_count = checked_add(overlap_count, 1)?;
            overlap_value = checked_add(overlap_value, event.value_raw)?;
        }
        if event.fee_raw > 0 {
            let fee = classes.entry("fee".into()).or_default();
            fee.0 = checked_add(fee.0, 1)?;
            fee.1 = checked_add(fee.1, event.fee_raw)?;
        }
    }
    let total_count = events.len() as u64;
    let classifications = ALL_CLASSIFICATIONS
        .iter()
        .map(|classification| {
            let (count, value) = classes.get(*classification).copied().unwrap_or_default();
            ClassificationAggregate {
                classification: (*classification).into(),
                operation_count: count,
                value_raw: value.to_string(),
            }
        })
        .collect();
    Ok(DailyTransactionAggregate {
        date_utc: date,
        asset_symbol: "rUSD".into(),
        currency_area: "USD".into(),
        total_operation_count: total_count,
        total_value_raw: total_value.to_string(),
        total_value_usd_minor: raw_to_minor(total_value)?.to_string(),
        means_of_exchange_count: means_count,
        means_of_exchange_value_raw: means_value.to_string(),
        means_of_exchange_value_usd_minor: raw_to_minor(means_value)?.to_string(),
        means_of_exchange_value_eur_minor: raw_to_minor(means_value)?.to_string(),
        excluded_operation_count: total_count - means_count,
        known_onchain_overlap_count: overlap_count,
        known_onchain_overlap_value_raw: overlap_value.to_string(),
        classifications,
        methodology_version: METHODOLOGY_VERSION.into(),
        conversion_methodology: CONVERSION_METHODOLOGY.into(),
    })
}

const ALL_CLASSIFICATIONS: [&str; 7] = [
    "custody_rebalancing",
    "exchange_for_funds",
    "fee",
    "goods_or_services",
    "private_transfer",
    "same_owner_transfer",
    "unknown",
];

fn raw_to_minor(raw: u64) -> Result<u64, ReportingError> {
    if !raw.is_multiple_of(10_000) {
        return Err(ReportingError::InvalidAmount);
    }
    Ok(raw / 10_000)
}

fn checked_add(left: u64, right: u64) -> Result<u64, ReportingError> {
    left.checked_add(right).ok_or(ReportingError::Overflow)
}

fn validate_date(value: &str) -> Result<(), ReportingError> {
    let bytes = value.as_bytes();
    let shape = bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit());
    if !shape {
        return Err(ReportingError::InvalidDateRange);
    }
    let year = value[0..4]
        .parse::<u32>()
        .map_err(|_| ReportingError::InvalidDateRange)?;
    let month = value[5..7]
        .parse::<u32>()
        .map_err(|_| ReportingError::InvalidDateRange)?;
    let day = value[8..10]
        .parse::<u32>()
        .map_err(|_| ReportingError::InvalidDateRange)?;
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let maximum = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => 0,
    };
    if day > 0 && day <= maximum {
        Ok(())
    } else {
        Err(ReportingError::InvalidDateRange)
    }
}

#[derive(Debug, Error)]
pub enum ReportingError {
    #[error("from/to must form an ordered YYYY-MM-DD UTC date range")]
    InvalidDateRange,
    #[error("reporting values must represent whole USD cents")]
    InvalidAmount,
    #[error("daily reporting arithmetic overflow")]
    Overflow,
    #[error("daily reporting persistence failed: {0}")]
    Storage(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Store(Vec<ReportingEvent>);
    impl ReportingStore for Store {
        fn events(&self, _: &str, _: &str) -> Result<Vec<ReportingEvent>, ReportingError> {
            Ok(self.0.clone())
        }
    }

    #[test]
    fn separates_total_activity_from_means_of_exchange_and_overlap() {
        let report = ReportingService::new(Arc::new(Store(vec![
            ReportingEvent {
                date_utc: "2026-08-26".into(),
                operation_id: "purchase".into(),
                classification: "exchange_for_funds".into(),
                value_raw: 10_000_000,
                fee_raw: 0,
                known_onchain_overlap: false,
            },
            ReportingEvent {
                date_utc: "2026-08-26".into(),
                operation_id: "goods".into(),
                classification: "goods_or_services".into(),
                value_raw: 5_000_000,
                fee_raw: 5_000,
                known_onchain_overlap: false,
            },
            ReportingEvent {
                date_utc: "2026-08-26".into(),
                operation_id: "redemption".into(),
                classification: "exchange_for_funds".into(),
                value_raw: 2_000_000,
                fee_raw: 0,
                known_onchain_overlap: true,
            },
        ])))
        .daily("2026-08-26", "2026-08-26")
        .unwrap();
        let day = &report.days[0];
        assert_eq!(day.total_operation_count, 3);
        assert_eq!(day.means_of_exchange_count, 1);
        assert_eq!(day.means_of_exchange_value_eur_minor, "500");
        assert_eq!(day.excluded_operation_count, 2);
        assert_eq!(day.known_onchain_overlap_count, 1);
        assert_eq!(
            day.classifications
                .iter()
                .find(|v| v.classification == "fee")
                .unwrap()
                .value_raw,
            "5000"
        );
    }
}
