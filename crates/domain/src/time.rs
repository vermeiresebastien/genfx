//! Exact rational time for the Flux timeline. All authored project time is a
//! reduced rational `numerator/denominator` and is the single source of truth.
//! Floating-point seconds are derived only for rendering/evaluation.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Signed rational time. Always stored reduced with `denominator > 0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RationalTime {
    numerator: i64,
    denominator: u32,
}

/// Errors produced by exact rational arithmetic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RationalError {
    #[error("denominator must be positive")]
    ZeroDenominator,
    #[error("rational time overflow")]
    Overflow,
}

impl RationalTime {
    pub const ZERO: RationalTime = RationalTime {
        numerator: 0,
        denominator: 1,
    };

    pub fn new(numerator: i64, denominator: u32) -> Result<Self, RationalError> {
        if denominator == 0 {
            return Err(RationalError::ZeroDenominator);
        }
        Ok(Self::reduce(numerator, denominator))
    }

    pub fn from_secs(secs: i64) -> Self {
        RationalTime {
            numerator: secs,
            denominator: 1,
        }
    }

    /// Exact conversion from an integer frame index and exact rate.
    pub fn from_frames(frame: i64, rate: FrameRate) -> Result<Self, RationalError> {
        let num = frame
            .checked_mul(rate.den as i64)
            .ok_or(RationalError::Overflow)?;
        Self::new(num, rate.num)
    }

    /// Duration of one frame at `rate`.
    pub fn frame_duration(rate: FrameRate) -> Self {
        Self::new(rate.den as i64, rate.num).expect("rate validated nonzero")
    }

    pub fn numerator(&self) -> i64 {
        self.numerator
    }

    pub fn denominator(&self) -> u32 {
        self.denominator
    }

    /// Exact rational `num/den` in seconds, reduced.
    pub fn as_ratio(&self) -> (i64, u32) {
        (self.numerator, self.denominator)
    }

    fn gcd(mut a: u128, mut b: u128) -> u128 {
        while b != 0 {
            let t = a % b;
            a = b;
            b = t;
        }
        a
    }

    fn reduce(numerator: i64, denominator: u32) -> Self {
        let g = Self::gcd(numerator.unsigned_abs() as u128, denominator as u128) as i64;
        let g = if g == 0 { 1 } else { g };
        RationalTime {
            numerator: numerator / g,
            denominator: (denominator as i64 / g) as u32,
        }
    }

    pub fn add(self, other: Self) -> Result<Self, RationalError> {
        let a = self.numerator as i128 * other.denominator as i128;
        let b = other.numerator as i128 * self.denominator as i128;
        let num = a.checked_add(b).ok_or(RationalError::Overflow)?;
        let den = self.denominator as i128 * other.denominator as i128;
        Self::from_i128(num, den)
    }

    pub fn sub(self, other: Self) -> Result<Self, RationalError> {
        self.add(other.neg())
    }

    pub fn neg(self) -> Self {
        RationalTime {
            numerator: self.numerator.saturating_neg(),
            denominator: self.denominator,
        }
    }

    pub fn mul_int(self, factor: i64) -> Result<Self, RationalError> {
        let num = self.numerator as i128 * factor as i128;
        Self::from_i128(num, self.denominator as i128)
    }

    fn from_i128(num: i128, den: i128) -> Result<Self, RationalError> {
        let (num, den) = if den < 0 { (-num, -den) } else { (num, den) };
        if den == 0 {
            return Err(RationalError::ZeroDenominator);
        }
        let g = Self::gcd(num.unsigned_abs(), den as u128) as i128;
        let g = if g == 0 { 1 } else { g };
        let rn = num / g;
        let rd = den / g;
        if rn > i64::MAX as i128 || rn < i64::MIN as i128 || rd > u32::MAX as i128 {
            return Err(RationalError::Overflow);
        }
        Ok(RationalTime {
            numerator: rn as i64,
            denominator: rd as u32,
        })
    }

    /// Compare against another rational exactly.
    pub fn cmp_exact(&self, other: &Self) -> std::cmp::Ordering {
        let a = self.numerator as i128 * other.denominator as i128;
        let b = other.numerator as i128 * self.denominator as i128;
        a.cmp(&b)
    }

    pub fn is_negative(&self) -> bool {
        self.numerator < 0
    }

    pub fn is_zero(&self) -> bool {
        self.numerator == 0
    }

    /// Floor `self / step` as an integer (used for frame/sample indices).
    pub fn div_floor(&self, step: Self) -> i64 {
        // (a/b) / (c/d) = ad / bc, floor division of i128.
        let a = self.numerator as i128 * step.denominator as i128;
        let b = self.denominator as i128 * step.numerator as i128;
        let q = a.div_euclid(b);
        q.clamp(i64::MIN as i128, i64::MAX as i128) as i64
    }

    /// Round `self / step` to nearest integer.
    pub fn div_round(&self, step: Self) -> i64 {
        let a = self.numerator as i128 * step.denominator as i128;
        let b = self.denominator as i128 * step.numerator as i128;
        let q = (2 * a + b).div_euclid(2 * b);
        q.clamp(i64::MIN as i128, i64::MAX as i128) as i64
    }

    /// Ceil `self / step` as an integer.
    pub fn div_ceil(&self, step: Self) -> i64 {
        let a = self.numerator as i128 * step.denominator as i128;
        let b = self.denominator as i128 * step.numerator as i128;
        let q = (-((-a).div_euclid(b))).clamp(i64::MIN as i128, i64::MAX as i128);
        q as i64
    }

    /// Approximate seconds. For rendering only; never the timeline authority.
    pub fn as_f64(&self) -> f64 {
        self.numerator as f64 / self.denominator as f64
    }
}

impl fmt::Display for RationalTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.numerator, self.denominator)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FrameRate {
    pub num: u32,
    pub den: u32,
}

impl FrameRate {
    pub const FPS_60: FrameRate = FrameRate { num: 60, den: 1 };

    pub fn new(num: u32, den: u32) -> Result<Self, RationalError> {
        if num == 0 || den == 0 {
            return Err(RationalError::ZeroDenominator);
        }
        let g = cat_gcd(num as i64, den as i64) as u32;
        Ok(FrameRate {
            num: num / g,
            den: den / g,
        })
    }

    pub fn as_f64(&self) -> f64 {
        self.num as f64 / self.den as f64
    }
}

fn cat_gcd(a: i64, b: i64) -> i64 {
    let (mut a, mut b) = (a.unsigned_abs() as i64, b.unsigned_abs() as i64);
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a
}

// Wire format: numerator is a decimal string; denominator is a u32 (see CONTRACTS).
#[derive(Serialize, Deserialize)]
struct RationalWire {
    numerator: String,
    denominator: u32,
}

impl Serialize for FrameRate {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("Rate", 2)?;
        st.serialize_field("num", &self.num)?;
        st.serialize_field("den", &self.den)?;
        st.end()
    }
}

impl<'de> Deserialize<'de> for FrameRate {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct RateWire {
            num: u32,
            den: u32,
        }
        let w = RateWire::deserialize(d)?;
        FrameRate::new(w.num, w.den).map_err(serde::de::Error::custom)
    }
}

impl Serialize for RationalTime {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        RationalWire {
            numerator: self.numerator.to_string(),
            denominator: self.denominator,
        }
        .serialize(s)
    }
}

impl<'de> Deserialize<'de> for RationalTime {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let w = RationalWire::deserialize(d)?;
        let n: i64 = w.numerator.parse().map_err(serde::de::Error::custom)?;
        RationalTime::new(n, w.denominator).map_err(serde::de::Error::custom)
    }
}

/// Half-open rational range `[start, start+duration)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeRange {
    pub start: RationalTime,
    pub duration: RationalTime,
}

impl TimeRange {
    pub fn new(start: RationalTime, duration: RationalTime) -> Self {
        Self { start, duration }
    }

    pub fn end(&self) -> Result<RationalTime, RationalError> {
        self.start.add(self.duration)
    }

    pub fn contains(&self, t: RationalTime) -> bool {
        self.start.cmp_exact(&t) != std::cmp::Ordering::Greater
            && t.cmp_exact(&self.end().unwrap_or(self.start)) == std::cmp::Ordering::Less
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rt(n: i64, d: u32) -> RationalTime {
        RationalTime::new(n, d).unwrap()
    }

    #[test]
    fn reduces_on_construction() {
        assert_eq!(rt(2, 4), rt(1, 2));
        assert_eq!(rt(0, 7), RationalTime::ZERO);
        assert_eq!(rt(-6, 4), rt(-3, 2));
    }

    #[test]
    fn exact_addition_has_no_float_error() {
        let a = rt(1, 3);
        let b = rt(1, 3);
        assert_eq!(a.add(b).unwrap(), rt(2, 3));
        // NaN-style accumulation check: forty 1/60s sums to 2/3 exactly.
        let fr = RationalTime::frame_duration(FrameRate::FPS_60);
        let mut acc = RationalTime::ZERO;
        for _ in 0..40 {
            acc = acc.add(fr).unwrap();
        }
        assert_eq!(acc, rt(2, 3));
    }

    #[test]
    fn frame_indices_round_trip() {
        let fr = FrameRate::new(30000, 1001).unwrap();
        for f in [0i64, 1, 29, 30, 599, 1000, 12345] {
            let t = RationalTime::from_frames(f, fr).unwrap();
            assert_eq!(t.div_floor(RationalTime::frame_duration(fr)), f);
        }
    }

    #[test]
    fn frame_start_seconds() {
        // Frame n starts at n * fps.den / fps.num.
        let fr = FrameRate::new(60000, 1001).unwrap();
        let t = RationalTime::from_frames(60, fr).unwrap();
        assert_eq!(t, rt(1001, 1000));
    }

    #[test]
    fn export_frame_count_ceils() {
        // ceil(D * fps.num / fps.den)
        let fr = FrameRate::FPS_60;
        let d = rt(103, 10);
        assert_eq!(d.div_ceil(RationalTime::frame_duration(fr)), 618);
        let exact = rt(10, 1);
        assert_eq!(exact.div_ceil(RationalTime::frame_duration(fr)), 600);
    }

    #[test]
    fn sample_count_rounds() {
        let d = rt(105, 10);
        assert_eq!(d.div_round(rt(1, 48000)), 504_000);
        let d2 = rt(1, 1);
        assert_eq!(d2.div_round(rt(1, 48000)), 48_000);
    }

    #[test]
    fn overflow_is_reported_not_wrapped() {
        let big = rt(i64::MAX, 3);
        assert_eq!(big.add(big), Err(RationalError::Overflow));
    }

    #[test]
    fn serde_uses_decimal_numerator_string() {
        let t = rt(-5, 2);
        let j = serde_json::to_string(&t).unwrap();
        assert_eq!(j, r#"{"numerator":"-5","denominator":2}"#);
        let back: RationalTime = serde_json::from_str(&j).unwrap();
        assert_eq!(back, t);
    }

    #[test]
    fn serde_rejects_zero_denominator() {
        let r: Result<RationalTime, _> =
            serde_json::from_str(r#"{"numerator":"1","denominator":0}"#);
        assert!(r.is_err());
    }

    #[test]
    fn half_open_ranges() {
        let r = TimeRange::new(rt(1, 1), rt(2, 1));
        assert!(r.contains(rt(1, 1)));
        assert!(r.contains(rt(3, 1).sub(rt(1, 1000)).unwrap()));
        assert!(!r.contains(rt(3, 1)));
    }
}
