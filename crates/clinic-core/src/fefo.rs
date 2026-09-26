//! First-Expiry-First-Out batch selection for sales (design D9/D10): never sells expired stock,
//! uses the batch that expires soonest first, batches without an expiry date last.

use std::cmp::Ordering;

use crate::money::Paise;
use crate::time::Date;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchStock {
    pub batch_id: i64,
    /// Last day the batch may be sold; `None` for products without expiry.
    pub expiry: Option<Date>,
    pub quantity: i64,
    pub price: Paise,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Allocation {
    pub batch_id: i64,
    pub qty: i64,
    pub price: Paise,
}

pub fn is_sellable(batch: &BatchStock, today: Date) -> bool {
    batch.quantity > 0 && batch.expiry.is_none_or(|expiry| expiry >= today)
}

/// Quantity that can be sold today.
pub fn available(batches: &[BatchStock], today: Date) -> i64 {
    batches.iter().filter(|b| is_sellable(b, today)).map(|b| b.quantity).sum()
}

fn fefo_order(a: &BatchStock, b: &BatchStock) -> Ordering {
    let by_expiry = match (a.expiry, b.expiry) {
        (Some(x), Some(y)) => x.cmp(&y),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    };
    by_expiry.then(a.batch_id.cmp(&b.batch_id))
}

/// Picks batches for `qty` units. `Err(available)` if there is not enough sellable stock.
pub fn allocate(batches: &[BatchStock], today: Date, qty: i64) -> Result<Vec<Allocation>, i64> {
    let mut candidates: Vec<&BatchStock> = batches.iter().filter(|b| is_sellable(b, today)).collect();
    candidates.sort_by(|a, b| fefo_order(a, b));
    let total: i64 = candidates.iter().map(|b| b.quantity).sum();
    if qty <= 0 || total < qty {
        return Err(total);
    }
    let mut remaining = qty;
    let mut picks = Vec::new();
    for batch in candidates {
        if remaining == 0 {
            break;
        }
        let take = remaining.min(batch.quantity);
        picks.push(Allocation { batch_id: batch.batch_id, qty: take, price: batch.price });
        remaining -= take;
    }
    Ok(picks)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn batch(id: i64, expiry: Option<&str>, qty: i64) -> BatchStock {
        BatchStock { batch_id: id, expiry: expiry.and_then(Date::parse), quantity: qty, price: Paise::new(2_000) }
    }

    fn today() -> Date {
        Date::parse("2026-09-25").unwrap_or(Date { year: 2026, month: 9, day: 25 })
    }

    #[test]
    fn earliest_expiry_is_used_first_and_expired_stock_never() {
        let batches = [batch(1, Some("2027-06-30"), 50), batch(2, Some("2026-12-31"), 20), batch(3, Some("2026-09-24"), 99), batch(4, None, 10)];
        assert_eq!(available(&batches, today()), 80, "batch 3 expired yesterday");
        let picks = allocate(&batches, today(), 25).map_err(|a| format!("available {a}"));
        assert_eq!(picks.map(|p| p.iter().map(|a| (a.batch_id, a.qty)).collect::<Vec<_>>()), Ok(vec![(2, 20), (1, 5)]));
        let all = allocate(&batches, today(), 80).unwrap_or_default();
        assert_eq!(all.last().map(|a| a.batch_id), Some(4), "no-expiry batches come last");
    }

    #[test]
    fn a_batch_expiring_today_can_still_be_sold() {
        assert!(is_sellable(&batch(1, Some("2026-09-25"), 1), today()));
    }

    #[test]
    fn shortfall_reports_what_is_available() {
        assert_eq!(allocate(&[batch(1, Some("2027-01-31"), 2)], today(), 5), Err(2));
        assert_eq!(allocate(&[batch(1, Some("2027-01-31"), 2)], today(), 0), Err(2));
    }
}
