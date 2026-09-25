//! Money is stored and calculated as whole paise (`i64`), never as floating point.
//! Rates (GST, discount %) are basis points: 18% = 1800 bp.

use std::fmt;

use serde::{Deserialize, Serialize};

/// An amount of money in paise (1 rupee = 100 paise). Serialised as a plain integer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Paise(i64);

/// A rate in basis points (1% = 100 bp). Serialised as a plain integer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BasisPoints(u32);

pub const BASIS_POINTS_PER_WHOLE: i64 = 10_000;

impl Paise {
    pub const ZERO: Paise = Paise(0);

    pub const fn new(paise: i64) -> Self {
        Paise(paise)
    }

    pub const fn value(self) -> i64 {
        self.0
    }

    pub fn checked_add(self, other: Paise) -> Option<Paise> {
        self.0.checked_add(other.0).map(Paise)
    }

    pub fn checked_sub(self, other: Paise) -> Option<Paise> {
        self.0.checked_sub(other.0).map(Paise)
    }

    pub fn checked_mul_qty(self, qty: u32) -> Option<Paise> {
        self.0.checked_mul(i64::from(qty)).map(Paise)
    }

    /// `rate` of this amount, rounded half-up to the nearest paisa (for non-negative amounts).
    pub fn percentage(self, rate: BasisPoints) -> Paise {
        Paise(div_round_half_up(i128::from(self.0) * i128::from(rate.0), i128::from(BASIS_POINTS_PER_WHOLE)))
    }

    /// Tax contained in a tax-inclusive amount: `amount × r / (10000 + r)`, rounded half-up.
    pub fn included_tax(self, rate: BasisPoints) -> Paise {
        let denominator = i128::from(BASIS_POINTS_PER_WHOLE) + i128::from(rate.0);
        Paise(div_round_half_up(i128::from(self.0) * i128::from(rate.0), denominator))
    }

    /// Rupees with Indian digit grouping and two decimals: `1,23,456.78`.
    pub fn to_indian_string(self) -> String {
        let sign = if self.0 < 0 { "-" } else { "" };
        let abs = self.0.unsigned_abs();
        let rupees = (abs / 100).to_string();
        let grouped = if rupees.len() <= 3 {
            rupees
        } else {
            let (head, last3) = rupees.split_at(rupees.len() - 3);
            let mut groups: Vec<&str> = Vec::new();
            let mut end = head.len();
            while end > 0 {
                let start = end.saturating_sub(2);
                groups.push(&head[start..end]);
                end = start;
            }
            groups.reverse();
            format!("{},{last3}", groups.join(","))
        };
        format!("{sign}{grouped}.{:02}", abs % 100)
    }
}

impl BasisPoints {
    pub const fn new(bp: u32) -> Self {
        BasisPoints(bp)
    }

    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Integer division rounding half away from zero. Results always fit in i64 for realistic
/// clinic amounts; values are clamped rather than wrapped as a last line of defence.
fn div_round_half_up(numerator: i128, denominator: i128) -> i64 {
    let half = denominator / 2;
    let rounded = if numerator >= 0 { (numerator + half) / denominator } else { (numerator - half) / denominator };
    i64::try_from(rounded).unwrap_or(if rounded > 0 { i64::MAX } else { i64::MIN })
}

impl fmt::Display for Paise {
    /// Formats as rupees with two decimals, e.g. `1234.50` (currency symbol is added by the UI).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.0 < 0 { "-" } else { "" };
        let abs = self.0.unsigned_abs();
        write!(f, "{sign}{}.{:02}", abs / 100, abs % 100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn displays_rupees_with_two_decimals() {
        assert_eq!(Paise::new(0).to_string(), "0.00");
        assert_eq!(Paise::new(5).to_string(), "0.05");
        assert_eq!(Paise::new(15_000).to_string(), "150.00");
        assert_eq!(Paise::new(-1_050).to_string(), "-10.50");
    }

    #[test]
    fn formats_with_indian_grouping() {
        assert_eq!(Paise::new(0).to_indian_string(), "0.00");
        assert_eq!(Paise::new(99_999).to_indian_string(), "999.99");
        assert_eq!(Paise::new(100_000).to_indian_string(), "1,000.00");
        assert_eq!(Paise::new(12_345_678).to_indian_string(), "1,23,456.78");
        assert_eq!(Paise::new(1_234_567_890).to_indian_string(), "1,23,45,678.90");
        assert_eq!(Paise::new(-150_000).to_indian_string(), "-1,500.00");
    }

    #[test]
    fn multiplies_by_quantity() {
        assert_eq!(Paise::new(2_000).checked_mul_qty(3), Some(Paise::new(6_000)));
        assert_eq!(Paise::new(i64::MAX).checked_mul_qty(2), None);
    }

    #[test]
    fn percentage_rounds_half_up() {
        // 10% of 160.00 = 16.00
        assert_eq!(Paise::new(16_000).percentage(BasisPoints::new(1_000)), Paise::new(1_600));
        // 12.5% of 0.99 = 0.12375 -> 0.12
        assert_eq!(Paise::new(99).percentage(BasisPoints::new(1_250)), Paise::new(12));
        // 5% of 0.10 = 0.005 -> 0.01 (half up)
        assert_eq!(Paise::new(10).percentage(BasisPoints::new(500)), Paise::new(1));
    }

    #[test]
    fn included_tax_is_back_calculated() {
        // ₹112.00 inclusive of 12% GST contains ₹12.00 tax.
        assert_eq!(Paise::new(11_200).included_tax(BasisPoints::new(1_200)), Paise::new(1_200));
        // ₹40.00 at 12%: 40 × 12 / 112 = 4.2857 -> 4.29
        assert_eq!(Paise::new(4_000).included_tax(BasisPoints::new(1_200)), Paise::new(429));
        // 0% GST -> no tax
        assert_eq!(Paise::new(4_000).included_tax(BasisPoints::new(0)), Paise::ZERO);
    }
}
