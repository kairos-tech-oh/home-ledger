//! Money, as decimal rather than binary floating point.

use rust_decimal::Decimal;
use rust_decimal::prelude::*;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Neg, Sub, SubAssign};

/// A ceiling that is absurd for money but finite, so one bad input cannot
/// produce an amount that poisons every total derived from it.
pub const MONEY_MAX: i64 = 1_000_000_000_000_000;

/// Displayed money is two decimals; a share price needs four, because rounding
/// 292.8571 to 292.86 and multiplying by 140 shares loses real money.
pub const DISPLAY_SCALE: u32 = 2;
pub const PRICE_SCALE: u32 = 4;

/// An amount of money.
///
/// The Python helper this is ported from used `float` with `round(n, 2)`. That
/// is a bug waiting to happen for a ledger: `0.1 + 0.2 != 0.3` in binary, and a
/// bucket that is adjusted thousands of times accumulates the drift. Decimal
/// arithmetic is exact for the values money actually takes.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Money(Decimal);

impl Money {
    pub const ZERO: Money = Money(Decimal::ZERO);

    /// Clamp to the ceiling and round to cents. Every amount entering the
    /// ledger goes through here, so nothing downstream has to re-check.
    pub fn new(value: Decimal) -> Self {
        Self(round(clamp(value), DISPLAY_SCALE))
    }

    /// Prices keep four decimals instead of two.
    pub fn price(value: Decimal) -> Self {
        Self(round(clamp(value), PRICE_SCALE))
    }

    /// Parse whatever the UI sent: a number, a numeric string, or nothing.
    /// Anything unreadable is the default rather than an error, matching the
    /// helper's rule that bad input is cleaned, not refused.
    pub fn parse(raw: &serde_json::Value, default: Money) -> Money {
        match raw {
            serde_json::Value::Number(n) => n
                .as_f64()
                .and_then(Decimal::from_f64)
                .map(Money::new)
                .unwrap_or(default),
            serde_json::Value::String(s) if !s.is_empty() => s
                .trim()
                .parse::<Decimal>()
                .map(Money::new)
                .unwrap_or(default),
            _ => default,
        }
    }

    pub fn is_zero(self) -> bool {
        self.0.is_zero()
    }

    pub fn is_negative(self) -> bool {
        self.0.is_sign_negative() && !self.0.is_zero()
    }

    pub fn abs(self) -> Self {
        Self(self.0.abs())
    }

    /// Never below zero: a savings bucket cannot hold a negative balance.
    pub fn floor_at_zero(self) -> Self {
        if self.is_negative() {
            Money::ZERO
        } else {
            self
        }
    }

    pub fn inner(self) -> Decimal {
        self.0
    }

    pub fn to_f64(self) -> f64 {
        self.0.to_f64().unwrap_or(0.0)
    }

    /// Multiply by a plain count, such as shares held or months accrued.
    pub fn scale(self, factor: Decimal) -> Self {
        Money::new(self.0 * factor)
    }
}

fn clamp(value: Decimal) -> Decimal {
    let ceiling = Decimal::from(MONEY_MAX);
    value.clamp(-ceiling, ceiling)
}

/// Half away from zero, so 1.005 reads as 1.01 the way a person expects.
/// Deliberately not the library default, which is banker's rounding.
///
/// This is the right rule for an amount shown to someone. It is the wrong rule
/// for splitting one amount into shares — rounding each share independently
/// loses or invents a cent — so splits must distribute the remainder instead.
/// See `ledger_math::split`.
fn round(value: Decimal, scale: u32) -> Decimal {
    value.round_dp_with_strategy(scale, RoundingStrategy::MidpointAwayFromZero)
}

impl Add for Money {
    type Output = Money;
    fn add(self, rhs: Money) -> Money {
        Money::new(self.0 + rhs.0)
    }
}

impl Sub for Money {
    type Output = Money;
    fn sub(self, rhs: Money) -> Money {
        Money::new(self.0 - rhs.0)
    }
}

impl Neg for Money {
    type Output = Money;
    fn neg(self) -> Money {
        Money(-self.0)
    }
}

impl AddAssign for Money {
    fn add_assign(&mut self, rhs: Money) {
        *self = *self + rhs;
    }
}

impl SubAssign for Money {
    fn sub_assign(&mut self, rhs: Money) {
        *self = *self - rhs;
    }
}

impl Sum for Money {
    fn sum<I: Iterator<Item = Money>>(iter: I) -> Money {
        iter.fold(Money::ZERO, |acc, m| acc + m)
    }
}

impl From<i64> for Money {
    fn from(value: i64) -> Money {
        Money::new(Decimal::from(value))
    }
}

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Rounded before formatting, because `{:.2}` on a Decimal truncates
        // rather than rounds and a four-decimal price would show a cent low.
        // The width is still needed after that, to pad 5 out to 5.00.
        write!(f, "{:.2}", round(self.0, DISPLAY_SCALE))
    }
}

impl fmt::Debug for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Money({})", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn decimal_addition_is_exact() {
        // The reason this type exists: in f64 this assertion fails.
        let sum = Money::new(dec!(0.1)) + Money::new(dec!(0.2));
        assert_eq!(sum, Money::new(dec!(0.3)));
    }

    #[test]
    fn repeated_adjustment_does_not_drift() {
        let mut bucket = Money::ZERO;
        for _ in 0..10_000 {
            bucket += Money::new(dec!(0.01));
        }
        assert_eq!(bucket, Money::new(dec!(100)));
    }

    #[test]
    fn amounts_are_rounded_to_cents_half_away_from_zero() {
        // Not the library default: banker's rounding would make this 1.00,
        // which reads as a bug to anyone looking at their own money.
        assert_eq!(Money::new(dec!(1.005)).to_string(), "1.01");
        assert_eq!(Money::new(dec!(1.004)).to_string(), "1.00");
        assert_eq!(Money::new(dec!(2.005)).to_string(), "2.01");
        assert_eq!(Money::new(dec!(-1.005)).to_string(), "-1.01");
    }

    #[test]
    fn a_four_decimal_price_displays_rounded_not_truncated() {
        // `{:.2}` on a Decimal truncates, so this would read 292.85 — a cent
        // low on every share price shown.
        assert_eq!(Money::price(dec!(292.8571)).to_string(), "292.86");
        assert_eq!(Money::price(dec!(292.8512)).to_string(), "292.85");
        assert_eq!(Money::price(dec!(-292.8571)).to_string(), "-292.86");
    }

    #[test]
    fn displaying_an_amount_always_shows_two_decimals() {
        assert_eq!(Money::from(5).to_string(), "5.00");
        assert_eq!(Money::new(dec!(5.5)).to_string(), "5.50");
    }

    #[test]
    fn prices_keep_four_decimals() {
        let price = Money::price(dec!(292.85714));
        assert_eq!(price.inner(), dec!(292.8571));
    }

    #[test]
    fn a_bucket_cannot_go_negative() {
        assert_eq!(Money::new(dec!(-5)).floor_at_zero(), Money::ZERO);
        assert_eq!(Money::new(dec!(5)).floor_at_zero(), Money::new(dec!(5)));
    }

    #[test]
    fn absurd_input_is_clamped_not_propagated() {
        let huge = Money::new(Decimal::MAX);
        assert_eq!(huge.inner(), Decimal::from(MONEY_MAX));
    }

    #[test]
    fn unreadable_input_falls_back_to_the_default() {
        let bad = serde_json::json!("not a number");
        assert_eq!(Money::parse(&bad, Money::ZERO), Money::ZERO);
        let good = serde_json::json!("12.34");
        assert_eq!(good_value(&good), "12.34");
    }

    fn good_value(v: &serde_json::Value) -> String {
        Money::parse(v, Money::ZERO).to_string()
    }
}
