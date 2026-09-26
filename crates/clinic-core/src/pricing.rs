//! Bill arithmetic, integers only (paise). Prices include GST (design D2); a bill-level
//! discount is spread over the discount-eligible lines in proportion to their amounts (D6) so
//! that GST and later refunds stay exact. Consultation and procedure lines are normally not
//! eligible, so a medicine discount never reduces them (DEC-030). The total can be rounded to
//! the nearest rupee with a visible round-off (D4).

use serde::{Deserialize, Serialize};

use crate::money::{BASIS_POINTS_PER_WHOLE, BasisPoints, Paise};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineInput {
    pub unit_price: Paise,
    pub qty: u32,
    pub gst_rate: BasisPoints,
    /// Whether the bill discount may reduce this line.
    pub discount_eligible: bool,
}

/// Bill-level discount. JSON: `{"kind":"NONE"}`, `{"kind":"PERCENT","value":1000}` (basis points)
/// or `{"kind":"AMOUNT","value":1500}` (paise).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "kind", content = "value", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Discount {
    #[default]
    None,
    Percent(BasisPoints),
    Amount(Paise),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PricedLine {
    pub gross: Paise,
    pub discount_share: Paise,
    /// What the client pays for this line, GST included.
    pub net: Paise,
    /// GST contained in `net`.
    pub tax: Paise,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BillTotals {
    pub lines: Vec<PricedLine>,
    pub subtotal: Paise,
    /// Part of the subtotal the discount applies to (the discount-eligible lines).
    pub eligible_subtotal: Paise,
    pub discount: Paise,
    pub tax: Paise,
    pub round_off: Paise,
    pub total: Paise,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PricingError {
    #[error("the amount is too large")]
    Overflow,
    #[error("the discount is larger than the amount it applies to")]
    DiscountTooLarge,
    #[error("a percentage discount cannot exceed 100%")]
    InvalidPercent,
}

pub fn price_bill(lines: &[LineInput], discount: Discount, round_to_rupee: bool) -> Result<BillTotals, PricingError> {
    let gross: Vec<i64> = lines
        .iter()
        .map(|l| l.unit_price.checked_mul_qty(l.qty).map(Paise::value).ok_or(PricingError::Overflow))
        .collect::<Result<_, _>>()?;
    let subtotal = gross.iter().try_fold(0i64, |sum, g| sum.checked_add(*g)).ok_or(PricingError::Overflow)?;
    // Only eligible lines carry the discount: the others get weight 0 in the allocation.
    let weights: Vec<i64> = lines.iter().zip(&gross).map(|(l, g)| if l.discount_eligible { *g } else { 0 }).collect();
    let eligible = weights.iter().sum::<i64>();
    let discount_amount = match discount {
        Discount::None => 0,
        Discount::Percent(rate) => {
            if i64::from(rate.value()) > BASIS_POINTS_PER_WHOLE {
                return Err(PricingError::InvalidPercent);
            }
            Paise::new(eligible).percentage(rate).value()
        }
        Discount::Amount(amount) => {
            if amount.value() < 0 || amount.value() > eligible {
                return Err(PricingError::DiscountTooLarge);
            }
            amount.value()
        }
    };
    let shares = allocate_proportionally(discount_amount, &weights);
    let priced: Vec<PricedLine> = lines
        .iter()
        .zip(gross.iter().zip(&shares))
        .map(|(line, (g, share))| {
            let net = Paise::new(g - share);
            PricedLine { gross: Paise::new(*g), discount_share: Paise::new(*share), net, tax: net.included_tax(line.gst_rate) }
        })
        .collect();
    let net_total = subtotal - discount_amount;
    let round_off = if round_to_rupee { (net_total + 50) / 100 * 100 - net_total } else { 0 };
    Ok(BillTotals {
        tax: Paise::new(priced.iter().map(|l| l.tax.value()).sum()),
        lines: priced,
        subtotal: Paise::new(subtotal),
        eligible_subtotal: Paise::new(eligible),
        discount: Paise::new(discount_amount),
        round_off: Paise::new(round_off),
        total: Paise::new(net_total + round_off),
    })
}

/// Splits `amount` over `weights` proportionally; the paise left over by integer division go to
/// the largest remainders, so the shares always add up to exactly `amount`.
pub fn allocate_proportionally(amount: i64, weights: &[i64]) -> Vec<i64> {
    let total: i64 = weights.iter().sum();
    if amount == 0 || total <= 0 {
        return vec![0; weights.len()];
    }
    let exact: Vec<i128> = weights.iter().map(|w| i128::from(amount) * i128::from(*w)).collect();
    let mut shares: Vec<i64> = exact.iter().map(|e| (e / i128::from(total)) as i64).collect();
    let mut order: Vec<usize> = (0..weights.len()).collect();
    order.sort_by(|a, b| (exact[*b] % i128::from(total)).cmp(&(exact[*a] % i128::from(total))).then(a.cmp(b)));
    let mut left = amount - shares.iter().sum::<i64>();
    for index in order {
        if left <= 0 {
            break;
        }
        shares[index] += 1;
        left -= 1;
    }
    shares
}

/// Discount as basis points of the subtotal (for the receptionist cap), rounded up so that
/// 10.01% counts as over a 10% cap.
pub fn discount_rate_bp(discount: Paise, subtotal: Paise) -> u32 {
    if subtotal.value() <= 0 {
        return 0;
    }
    let bp = (i128::from(discount.value()) * i128::from(BASIS_POINTS_PER_WHOLE) + i128::from(subtotal.value()) - 1)
        / i128::from(subtotal.value());
    u32::try_from(bp).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(price: i64, qty: u32, gst_bp: u32) -> LineInput {
        LineInput { unit_price: Paise::new(price), qty, gst_rate: BasisPoints::new(gst_bp), discount_eligible: true }
    }

    fn service(price: i64) -> LineInput {
        LineInput { unit_price: Paise::new(price), qty: 1, gst_rate: BasisPoints::new(0), discount_eligible: false }
    }

    #[test]
    fn the_discount_never_reduces_consultation_or_procedures() -> Result<(), PricingError> {
        // Consultation ₹500 + dressing ₹300 + medicines ₹160, ₹10 off the medicines.
        let totals = price_bill(&[service(50_000), service(30_000), line(2_000, 2, 1_200), line(12_000, 1, 1_800)], Discount::Amount(Paise::new(1_000)), true)?;
        assert_eq!(totals.subtotal, Paise::new(96_000));
        assert_eq!(totals.eligible_subtotal, Paise::new(16_000));
        assert_eq!((totals.lines[0].discount_share, totals.lines[1].discount_share), (Paise::ZERO, Paise::ZERO));
        assert_eq!(totals.lines[2].discount_share.value() + totals.lines[3].discount_share.value(), 1_000);
        assert_eq!(totals.total, Paise::new(95_000));
        // 10% is 10% of the medicines only; more than the medicines is refused.
        let percent = price_bill(&[service(50_000), line(2_000, 5, 0)], Discount::Percent(BasisPoints::new(1_000)), false)?;
        assert_eq!(percent.discount, Paise::new(1_000));
        assert_eq!(price_bill(&[service(50_000), line(2_000, 1, 0)], Discount::Amount(Paise::new(2_001)), false), Err(PricingError::DiscountTooLarge));
        Ok(())
    }

    #[test]
    fn example_bill_from_the_brief() -> Result<(), PricingError> {
        // Paracetamol 2 x 20.00 (12%) + cream 1 x 120.00 (18%), ₹10 off.
        let totals = price_bill(&[line(2_000, 2, 1_200), line(12_000, 1, 1_800)], Discount::Amount(Paise::new(1_000)), true)?;
        assert_eq!(totals.subtotal, Paise::new(16_000));
        assert_eq!(totals.discount, Paise::new(1_000));
        assert_eq!(totals.lines[0].discount_share, Paise::new(250)); // 10 x 40/160
        assert_eq!(totals.lines[1].discount_share, Paise::new(750));
        assert_eq!(totals.total, Paise::new(15_000));
        assert_eq!(totals.round_off, Paise::ZERO);
        assert_eq!(totals.tax, Paise::new(402 + 1_716));
        Ok(())
    }

    #[test]
    fn percent_discount_and_round_off() -> Result<(), PricingError> {
        let totals = price_bill(&[line(3_333, 3, 500)], Discount::Percent(BasisPoints::new(1_000)), true)?;
        // 99.99 - 10% (10.00) = 89.99 -> 90.00
        assert_eq!(totals.discount, Paise::new(1_000));
        assert_eq!(totals.round_off, Paise::new(1));
        assert_eq!(totals.total, Paise::new(9_000));
        let exact = price_bill(&[line(3_333, 3, 500)], Discount::None, false)?;
        assert_eq!((exact.total, exact.round_off), (Paise::new(9_999), Paise::ZERO));
        Ok(())
    }

    #[test]
    fn invalid_discounts_are_refused() {
        assert_eq!(price_bill(&[line(1_000, 1, 0)], Discount::Amount(Paise::new(1_001)), false), Err(PricingError::DiscountTooLarge));
        assert_eq!(price_bill(&[line(1_000, 1, 0)], Discount::Percent(BasisPoints::new(10_001)), false), Err(PricingError::InvalidPercent));
    }

    #[test]
    fn allocation_always_adds_up() {
        for (amount, weights) in [(100, vec![1, 1, 1]), (7, vec![3, 3, 1]), (1_000, vec![4_000, 12_000]), (5, vec![0, 0]), (0, vec![5])] {
            let shares = allocate_proportionally(amount, &weights);
            let expected = if weights.iter().sum::<i64>() == 0 { 0 } else { amount };
            assert_eq!(shares.iter().sum::<i64>(), expected, "{amount} over {weights:?} -> {shares:?}");
        }
        assert_eq!(allocate_proportionally(100, &[1, 1, 1]), vec![34, 33, 33]);
    }

    #[test]
    fn discount_rate_rounds_up() {
        assert_eq!(discount_rate_bp(Paise::new(1_000), Paise::new(10_000)), 1_000);
        assert_eq!(discount_rate_bp(Paise::new(1_001), Paise::new(10_000)), 1_001);
        assert_eq!(discount_rate_bp(Paise::new(1), Paise::new(30_000)), 1);
        assert_eq!(discount_rate_bp(Paise::new(5), Paise::ZERO), 0);
    }
}
