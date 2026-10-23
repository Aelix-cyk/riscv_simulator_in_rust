//! Bit-exact IEEE-754 double-precision operations for the RISC-V D extension.

use crate::fp::{FLAG_DZ, FLAG_NV, FLAG_NX, FLAG_OF, FLAG_UF, FpResult, IntResult, RoundingMode};

/// Canonical double-precision quiet NaN.
pub(crate) const CANONICAL_NAN: u64 = 0x7ff8_0000_0000_0000;
/// Sign bit in a double-precision value.
pub(crate) const SIGN_MASK: u64 = 1 << 63;

const EXPONENT_MASK: u64 = 0x7ff << 52;
const FRACTION_MASK: u64 = (1 << 52) - 1;
const EXPONENT_BITS: u32 = 11;
const FRACTION_BITS: u32 = 52;
const EXPONENT_BIAS: i32 = 1023;
const MIN_NORMAL_EXPONENT: i32 = -1022;
const MAX_NORMAL_EXPONENT: i32 = 1023;
const MIN_SUBNORMAL_EXPONENT: i32 = -1074;
const ALL_ONES_EXPONENT: u32 = (1 << EXPONENT_BITS) - 1;
const HIDDEN_BIT: u128 = 1 << FRACTION_BITS;
const MAX_FINITE: u64 = (EXPONENT_MASK - (1 << FRACTION_BITS)) | FRACTION_MASK;
const DIVISION_GUARD_BITS: u32 = 64;
const SQRT_TARGET_BITS: u32 = 126;
const WORKING_BITS: u32 = 126;
const BITS_PER_U128: u32 = u128::BITS;
const WORD_BITS: u32 = 32;
const DOUBLEWORD_BITS: u32 = 64;
const SINGLE_FRACTION_BITS: u32 = 23;
const SINGLE_EXPONENT_BIAS: i32 = 127;
const SINGLE_MIN_NORMAL_EXPONENT: i32 = -126;
const SINGLE_MAX_NORMAL_EXPONENT: i32 = 127;
const SINGLE_MIN_SUBNORMAL_EXPONENT: i32 = -149;

/// Result bits and newly raised IEEE flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DoubleResult {
    pub(crate) bits: u64,
    pub(crate) flags: u8,
}

impl DoubleResult {
    const fn exact(bits: u64) -> Self {
        Self { bits, flags: 0 }
    }

    const fn invalid() -> Self {
        Self {
            bits: CANONICAL_NAN,
            flags: FLAG_NV,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Finite {
    negative: bool,
    significand: u128,
    exponent: i32,
}

#[derive(Debug, Clone, Copy)]
struct Prepared {
    negative: bool,
    significand: u128,
    exponent: i32,
    sticky: Sticky,
    tiny_subtract: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sticky {
    None,
    BelowHalf,
}

impl Sticky {
    fn is_set(self) -> bool {
        self != Self::None
    }
}

#[derive(Debug, Clone, Copy)]
enum Value {
    Finite(Finite),
    Infinity(bool),
    NaN { quiet: bool },
}

/// Adds two double-precision values.
pub(crate) fn add(a: u64, b: u64, mode: RoundingMode) -> DoubleResult {
    if let Some(result) = nan_binary(a, b) {
        return result;
    }
    match (classify(a), classify(b)) {
        (Value::Infinity(a_negative), Value::Infinity(b_negative)) if a_negative != b_negative => {
            DoubleResult::invalid()
        }
        (Value::Infinity(negative), _) => DoubleResult::exact(infinity(negative)),
        (_, Value::Infinity(negative)) => DoubleResult::exact(infinity(negative)),
        (Value::Finite(a), Value::Finite(b)) => {
            if a.significand == 0 && b.significand == 0 {
                let negative = if a.negative == b.negative {
                    a.negative
                } else {
                    matches!(mode, RoundingMode::Down)
                };
                return DoubleResult::exact(if negative { SIGN_MASK } else { 0 });
            }
            round_prepared(exact_add(a, b, mode), mode)
        }
        _ => unreachable!("NaNs were handled above"),
    }
}

/// Subtracts two double-precision values.
pub(crate) fn sub(a: u64, b: u64, mode: RoundingMode) -> DoubleResult {
    add(a, b ^ SIGN_MASK, mode)
}

/// Multiplies two double-precision values.
pub(crate) fn mul(a: u64, b: u64, mode: RoundingMode) -> DoubleResult {
    if let Some(result) = nan_binary(a, b) {
        return result;
    }
    match (classify(a), classify(b)) {
        (Value::Infinity(_), Value::Finite(b)) if b.significand == 0 => DoubleResult::invalid(),
        (Value::Finite(a), Value::Infinity(_)) if a.significand == 0 => DoubleResult::invalid(),
        (Value::Infinity(a_negative), Value::Finite(b))
        | (Value::Finite(b), Value::Infinity(a_negative)) => {
            DoubleResult::exact(infinity(a_negative ^ b.negative))
        }
        (Value::Infinity(a_negative), Value::Infinity(b_negative)) => {
            DoubleResult::exact(infinity(a_negative ^ b_negative))
        }
        (Value::Finite(a), Value::Finite(b)) => {
            let negative = a.negative ^ b.negative;
            if a.significand == 0 || b.significand == 0 {
                return DoubleResult::exact(if negative { SIGN_MASK } else { 0 });
            }
            round_prepared(
                Prepared {
                    negative,
                    significand: a.significand * b.significand,
                    exponent: a.exponent + b.exponent,
                    sticky: Sticky::None,
                    tiny_subtract: false,
                },
                mode,
            )
        }
        _ => unreachable!("NaNs, infinities, and finite values exhaust the cases"),
    }
}

/// Divides two double-precision values.
pub(crate) fn div(a: u64, b: u64, mode: RoundingMode) -> DoubleResult {
    if let Some(result) = nan_binary(a, b) {
        return result;
    }
    match (classify(a), classify(b)) {
        (Value::Infinity(_), Value::Infinity(_)) => DoubleResult::invalid(),
        (Value::Finite(a), Value::Finite(b)) if a.significand == 0 && b.significand == 0 => {
            DoubleResult::invalid()
        }
        (Value::Infinity(a_negative), Value::Finite(b)) => {
            DoubleResult::exact(infinity(a_negative ^ b.negative))
        }
        (Value::Finite(a), Value::Infinity(b_negative)) => {
            DoubleResult::exact(if a.negative ^ b_negative {
                SIGN_MASK
            } else {
                0
            })
        }
        (Value::Finite(a), Value::Finite(b)) if b.significand == 0 => DoubleResult {
            bits: infinity(a.negative ^ b.negative),
            flags: FLAG_DZ,
        },
        (Value::Finite(a), Value::Finite(b)) => {
            if a.significand == 0 {
                return DoubleResult::exact(if a.negative ^ b.negative {
                    SIGN_MASK
                } else {
                    0
                });
            }
            let scale = (i64::from(DIVISION_GUARD_BITS) + i64::from(bit_length(b.significand))
                - i64::from(bit_length(a.significand)))
            .max(0) as u32;
            let numerator = a.significand << scale;
            let significand = numerator / b.significand;
            let remainder = numerator % b.significand;
            round_prepared(
                Prepared {
                    negative: a.negative ^ b.negative,
                    significand,
                    exponent: a.exponent - b.exponent - scale as i32,
                    sticky: if remainder == 0 {
                        Sticky::None
                    } else {
                        Sticky::BelowHalf
                    },
                    tiny_subtract: false,
                },
                mode,
            )
        }
        _ => unreachable!("NaNs and infinities were handled above"),
    }
}

/// Computes the square root of a double-precision value.
pub(crate) fn sqrt(a: u64, mode: RoundingMode) -> DoubleResult {
    match classify(a) {
        Value::NaN { quiet } => DoubleResult {
            bits: CANONICAL_NAN,
            flags: if quiet { 0 } else { FLAG_NV },
        },
        Value::Infinity(false) => DoubleResult::exact(a),
        Value::Infinity(true) => DoubleResult::invalid(),
        Value::Finite(a) if a.significand == 0 => DoubleResult::exact(a.bits()),
        Value::Finite(a) if a.negative => DoubleResult::invalid(),
        Value::Finite(a) => {
            let bits = bit_length(a.significand);
            let mut shift = (SQRT_TARGET_BITS.saturating_sub(bits)) & !1;
            if (a.exponent - shift as i32).rem_euclid(2) != 0 {
                shift -= 1;
            }
            let radicand = a.significand << shift;
            let significand = integer_sqrt(radicand);
            round_prepared(
                Prepared {
                    negative: false,
                    significand,
                    exponent: (a.exponent - shift as i32) / 2,
                    sticky: if significand * significand == radicand {
                        Sticky::None
                    } else {
                        Sticky::BelowHalf
                    },
                    tiny_subtract: false,
                },
                mode,
            )
        }
    }
}

/// Computes `a * b + c` with one final rounding.
pub(crate) fn fma(
    a: u64,
    b: u64,
    c: u64,
    mode: RoundingMode,
    negate_product: bool,
    negate_addend: bool,
) -> DoubleResult {
    if let Some(result) = nan_ternary(a, b, c) {
        return result;
    }

    let product = classify_product(a, b, negate_product);
    let addend_bits = if negate_addend { c ^ SIGN_MASK } else { c };
    match (product, classify(addend_bits)) {
        (Product::Invalid, _) => DoubleResult::invalid(),
        (Product::Infinity(negative), Value::Infinity(addend_negative))
            if negative != addend_negative =>
        {
            DoubleResult::invalid()
        }
        (Product::Infinity(negative), _) => DoubleResult::exact(infinity(negative)),
        (_, Value::Infinity(negative)) => DoubleResult::exact(infinity(negative)),
        (Product::Finite(product), Value::Finite(addend)) => {
            if product.significand == 0 && addend.significand == 0 {
                let negative = if product.negative == addend.negative {
                    product.negative
                } else {
                    matches!(mode, RoundingMode::Down)
                };
                return DoubleResult::exact(if negative { SIGN_MASK } else { 0 });
            }
            round_prepared(exact_add(product, addend, mode), mode)
        }
        _ => unreachable!("NaNs were handled above"),
    }
}

/// Returns the smaller value with RISC-V NaN and signed-zero rules.
pub(crate) fn min(a: u64, b: u64) -> DoubleResult {
    min_max(a, b, true)
}

/// Returns the larger value with RISC-V NaN and signed-zero rules.
pub(crate) fn max(a: u64, b: u64) -> DoubleResult {
    min_max(a, b, false)
}

fn min_max(a: u64, b: u64, minimum: bool) -> DoubleResult {
    let a_nan = is_nan(a);
    let b_nan = is_nan(b);
    if a_nan && b_nan {
        return DoubleResult {
            bits: CANONICAL_NAN,
            flags: if is_signaling_nan(a) || is_signaling_nan(b) {
                FLAG_NV
            } else {
                0
            },
        };
    }
    if a_nan || b_nan {
        return DoubleResult {
            bits: if a_nan { b } else { a },
            flags: if is_signaling_nan(a) || is_signaling_nan(b) {
                FLAG_NV
            } else {
                0
            },
        };
    }
    if is_zero(a) && is_zero(b) {
        let negative = if minimum {
            a & SIGN_MASK != 0 || b & SIGN_MASK != 0
        } else {
            a & SIGN_MASK != 0 && b & SIGN_MASK != 0
        };
        return DoubleResult::exact(if negative { SIGN_MASK } else { 0 });
    }
    let ordering = compare_non_nan(a, b);
    let use_a = if minimum {
        ordering.is_lt()
    } else {
        ordering.is_gt()
    };
    DoubleResult::exact(if use_a { a } else { b })
}

/// Compares two values. Returns `None` when either input is NaN.
pub(crate) fn compare(a: u64, b: u64, kind: Compare) -> (Option<bool>, u8) {
    if is_nan(a) || is_nan(b) {
        let flags = if matches!(kind, Compare::Equal) {
            if is_signaling_nan(a) || is_signaling_nan(b) {
                FLAG_NV
            } else {
                0
            }
        } else {
            FLAG_NV
        };
        return (None, flags);
    }
    let ordering = compare_non_nan(a, b);
    let result = match kind {
        Compare::Equal => ordering.is_eq(),
        Compare::Less => ordering.is_lt(),
        Compare::LessEqual => !ordering.is_gt(),
    };
    (Some(result), 0)
}

/// The relation requested by an F comparison instruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Compare {
    Equal,
    Less,
    LessEqual,
}

/// Classifies a double-precision value using the FCLASS.D bit assignments.
pub(crate) fn classify_bits(a: u64) -> u64 {
    if is_signaling_nan(a) {
        return 1 << 8;
    }
    if is_nan(a) {
        return 1 << 9;
    }
    match classify(a) {
        Value::Infinity(true) => 1 << 0,
        Value::Infinity(false) => 1 << 7,
        Value::Finite(a) => {
            if a.significand == 0 {
                if a.negative { 1 << 3 } else { 1 << 4 }
            } else if exponent_field(a.bits()) == 0 {
                if a.negative { 1 << 2 } else { 1 << 5 }
            } else if a.negative {
                1 << 1
            } else {
                1 << 6
            }
        }
        Value::NaN { .. } => unreachable!("NaNs were handled above"),
    }
}

/// Converts an integer to double precision.
pub(crate) fn from_int(value: u64, width: u32, unsigned: bool, mode: RoundingMode) -> DoubleResult {
    let value = match (width, unsigned) {
        (WORD_BITS, false) => i64::from(value as u32 as i32) as u64,
        (WORD_BITS, true) => u64::from(value as u32),
        (DOUBLEWORD_BITS, false) => value,
        (DOUBLEWORD_BITS, true) => value,
        _ => unreachable!("only 32- and 64-bit conversions exist"),
    };
    let negative = !unsigned && (value as i64) < 0;
    let magnitude = if negative {
        value.wrapping_neg()
    } else {
        value
    };
    if magnitude == 0 {
        return DoubleResult::exact(0);
    }
    round_prepared(
        Prepared {
            negative,
            significand: u128::from(magnitude),
            exponent: 0,
            sticky: Sticky::None,
            tiny_subtract: false,
        },
        mode,
    )
}

/// Converts a double-precision value to a sign-extended integer.
pub(crate) fn to_int(a: u64, width: u32, unsigned: bool, mode: RoundingMode) -> IntResult {
    let signed_max = match width {
        WORD_BITS => (1_u64 << (WORD_BITS - 1)) - 1,
        DOUBLEWORD_BITS => (1_u64 << (DOUBLEWORD_BITS - 1)) - 1,
        _ => unreachable!("only 32- and 64-bit conversions exist"),
    };
    let signed_negative_limit = signed_max + 1;
    let unsigned_max = match width {
        WORD_BITS => u64::from(u32::MAX),
        DOUBLEWORD_BITS => u64::MAX,
        _ => unreachable!("only 32- and 64-bit conversions exist"),
    };
    let invalid = match (width, unsigned) {
        (WORD_BITS, false) => sign_extend_word(signed_max as u32),
        (WORD_BITS, true) => sign_extend_word(u32::MAX),
        (DOUBLEWORD_BITS, false) => signed_max,
        (DOUBLEWORD_BITS, true) => u64::MAX,
        _ => unreachable!(),
    };
    let negative_invalid = match width {
        WORD_BITS => sign_extend_word(1 << (WORD_BITS - 1)),
        DOUBLEWORD_BITS => 1 << (DOUBLEWORD_BITS - 1),
        _ => unreachable!(),
    };

    let finite = match classify(a) {
        Value::NaN { .. } => {
            return IntResult {
                value: invalid,
                flags: FLAG_NV,
            };
        }
        Value::Infinity(negative) => {
            return IntResult {
                value: if !negative {
                    invalid
                } else if unsigned {
                    0
                } else {
                    negative_invalid
                },
                flags: FLAG_NV,
            };
        }
        Value::Finite(finite) => finite,
    };
    let (magnitude, inexact) = round_integer(finite, mode);
    let mut flags = if inexact { FLAG_NX } else { 0 };
    let bits = if finite.negative {
        if magnitude == 0 {
            0
        } else if unsigned || magnitude > signed_negative_limit as u128 {
            flags = FLAG_NV;
            if unsigned { 0 } else { negative_invalid }
        } else {
            encode_integer(-(magnitude as i128), width)
        }
    } else if unsigned {
        if magnitude > unsigned_max as u128 {
            flags = FLAG_NV;
            invalid
        } else {
            encode_integer(magnitude as i128, width)
        }
    } else if magnitude > signed_max as u128 {
        flags = FLAG_NV;
        invalid
    } else {
        encode_integer(magnitude as i128, width)
    };
    IntResult { value: bits, flags }
}

/// Widens a single-precision value to double precision.
pub(crate) fn from_single(a: u32) -> DoubleResult {
    let sign = u64::from(a & 0x8000_0000) << 32;
    let exponent = (a >> 23) & 0xff;
    let fraction = a & 0x007f_ffff;
    let bits = match exponent {
        0xff if fraction == 0 => sign | EXPONENT_MASK,
        0xff => {
            let flags = if fraction & (1 << 22) == 0 {
                FLAG_NV
            } else {
                0
            };
            return DoubleResult {
                bits: CANONICAL_NAN,
                flags,
            };
        }
        0 if fraction == 0 => sign,
        0 => {
            let significand_bits = u32::BITS - fraction.leading_zeros();
            let shift = FRACTION_BITS + 1 - significand_bits;
            let normalized = u64::from(fraction) << shift;
            let exponent = SINGLE_MIN_SUBNORMAL_EXPONENT - shift as i32;
            let exponent_field = (exponent + EXPONENT_BIAS + FRACTION_BITS as i32) as u64;
            sign | (exponent_field << FRACTION_BITS) | (normalized & FRACTION_MASK)
        }
        _ => {
            let exponent = exponent as i32 - 127 + EXPONENT_BIAS;
            sign | (u64::from(exponent as u32) << FRACTION_BITS) | (u64::from(fraction) << 29)
        }
    };
    DoubleResult::exact(bits)
}

/// Narrows a double-precision value to a NaN-boxed single-precision value.
pub(crate) fn to_single(a: u64, mode: RoundingMode) -> FpResult {
    let negative = a & SIGN_MASK != 0;
    let exponent = exponent_field(a);
    let fraction = a & FRACTION_MASK;
    if exponent == ALL_ONES_EXPONENT {
        if fraction == 0 {
            return fp_exact(infinity_single(negative));
        }
        return FpResult {
            bits: crate::fp::CANONICAL_NAN,
            flags: if fraction & (1 << (FRACTION_BITS - 1)) == 0 {
                FLAG_NV
            } else {
                0
            },
        };
    }
    let finite = match classify(a) {
        Value::Finite(value) => value,
        _ => unreachable!(),
    };
    round_prepared_to_single(
        Prepared {
            negative: finite.negative,
            significand: finite.significand,
            exponent: finite.exponent,
            sticky: Sticky::None,
            tiny_subtract: false,
        },
        mode,
    )
}

fn classify(bits: u64) -> Value {
    let negative = bits & SIGN_MASK != 0;
    let exponent = exponent_field(bits);
    let fraction = bits & FRACTION_MASK;
    if exponent == ALL_ONES_EXPONENT {
        if fraction == 0 {
            Value::Infinity(negative)
        } else {
            Value::NaN {
                quiet: fraction & (1 << (FRACTION_BITS - 1)) != 0,
            }
        }
    } else if exponent == 0 {
        Value::Finite(Finite {
            negative,
            significand: u128::from(fraction),
            exponent: MIN_SUBNORMAL_EXPONENT,
        })
    } else {
        Value::Finite(Finite {
            negative,
            significand: HIDDEN_BIT | u128::from(fraction),
            exponent: exponent as i32 - EXPONENT_BIAS - FRACTION_BITS as i32,
        })
    }
}

impl Finite {
    fn bits(self) -> u64 {
        let sign = if self.negative { SIGN_MASK } else { 0 };
        if self.significand == 0 {
            return sign;
        }
        if self.exponent == MIN_SUBNORMAL_EXPONENT {
            sign | self.significand as u64
        } else {
            let exponent = (self.exponent + EXPONENT_BIAS + FRACTION_BITS as i32) as u64;
            sign | (exponent << FRACTION_BITS) | (self.significand as u64 & FRACTION_MASK)
        }
    }
}

fn exponent_field(bits: u64) -> u32 {
    ((bits & EXPONENT_MASK) >> FRACTION_BITS) as u32
}

fn infinity(negative: bool) -> u64 {
    (if negative { SIGN_MASK } else { 0 }) | EXPONENT_MASK
}

fn infinity_single(negative: bool) -> u32 {
    (if negative { crate::fp::SIGN_MASK } else { 0 }) | 0x7f80_0000
}

fn is_nan(bits: u64) -> bool {
    exponent_field(bits) == ALL_ONES_EXPONENT && bits & FRACTION_MASK != 0
}

fn is_signaling_nan(bits: u64) -> bool {
    is_nan(bits) && bits & (1 << (FRACTION_BITS - 1)) == 0
}

fn is_zero(bits: u64) -> bool {
    bits & !SIGN_MASK == 0
}

fn is_infinity(bits: u64) -> bool {
    bits & !SIGN_MASK == EXPONENT_MASK
}

fn nan_binary(a: u64, b: u64) -> Option<DoubleResult> {
    if !is_nan(a) && !is_nan(b) {
        return None;
    }
    Some(DoubleResult {
        bits: CANONICAL_NAN,
        flags: if is_signaling_nan(a) || is_signaling_nan(b) {
            FLAG_NV
        } else {
            0
        },
    })
}

fn nan_ternary(a: u64, b: u64, c: u64) -> Option<DoubleResult> {
    if !is_nan(a) && !is_nan(b) && !is_nan(c) {
        return None;
    }
    let invalid_product = (is_infinity(a) && is_zero(b)) || (is_zero(a) && is_infinity(b));
    Some(DoubleResult {
        bits: CANONICAL_NAN,
        flags: if invalid_product
            || is_signaling_nan(a)
            || is_signaling_nan(b)
            || is_signaling_nan(c)
        {
            FLAG_NV
        } else {
            0
        },
    })
}

#[derive(Debug, Clone, Copy)]
enum Product {
    Finite(Finite),
    Infinity(bool),
    Invalid,
}

fn classify_product(a: u64, b: u64, negate: bool) -> Product {
    match (classify(a), classify(b)) {
        (Value::Infinity(_), Value::Finite(b)) if b.significand == 0 => Product::Invalid,
        (Value::Finite(a), Value::Infinity(_)) if a.significand == 0 => Product::Invalid,
        (Value::Infinity(a_negative), Value::Finite(b))
        | (Value::Finite(b), Value::Infinity(a_negative)) => {
            Product::Infinity(a_negative ^ b.negative ^ negate)
        }
        (Value::Infinity(a_negative), Value::Infinity(b_negative)) => {
            Product::Infinity(a_negative ^ b_negative ^ negate)
        }
        (Value::Finite(a), Value::Finite(b)) => Product::Finite(Finite {
            negative: a.negative ^ b.negative ^ negate,
            significand: a.significand * b.significand,
            exponent: a.exponent + b.exponent,
        }),
        _ => unreachable!("NaNs were handled before product classification"),
    }
}

fn exact_add(a: Finite, b: Finite, mode: RoundingMode) -> Prepared {
    if a.significand == 0 {
        return prepared(b);
    }
    if b.significand == 0 {
        return prepared(a);
    }
    let a_top = a.exponent + bit_length(a.significand) as i32 - 1;
    let b_top = b.exponent + bit_length(b.significand) as i32 - 1;
    let common = a_top.max(b_top) - (WORKING_BITS - 1) as i32;
    let (a_magnitude, a_sticky) = scale_to_common(a, common);
    let (b_magnitude, b_sticky) = scale_to_common(b, common);
    if a.negative == b.negative {
        Prepared {
            negative: a.negative,
            significand: a_magnitude + b_magnitude,
            exponent: common,
            sticky: if a_sticky || b_sticky {
                Sticky::BelowHalf
            } else {
                Sticky::None
            },
            tiny_subtract: false,
        }
    } else if a_magnitude == b_magnitude {
        Prepared {
            negative: matches!(mode, RoundingMode::Down),
            significand: 0,
            exponent: common,
            sticky: Sticky::None,
            tiny_subtract: false,
        }
    } else {
        let (negative, larger, smaller, smaller_sticky) = if a_magnitude > b_magnitude {
            (a.negative, a_magnitude, b_magnitude, b_sticky)
        } else {
            (b.negative, b_magnitude, a_magnitude, a_sticky)
        };
        Prepared {
            negative,
            significand: larger - smaller,
            exponent: common,
            sticky: Sticky::None,
            tiny_subtract: smaller_sticky,
        }
    }
}

fn scale_to_common(value: Finite, common: i32) -> (u128, bool) {
    let shift = value.exponent - common;
    if shift >= 0 {
        (value.significand << shift as u32, false)
    } else {
        let shift = shift.unsigned_abs();
        if shift >= BITS_PER_U128 {
            (0, value.significand != 0)
        } else {
            (
                value.significand >> shift,
                value.significand & ((1_u128 << shift) - 1) != 0,
            )
        }
    }
}

fn prepared(value: Finite) -> Prepared {
    Prepared {
        negative: value.negative,
        significand: value.significand,
        exponent: value.exponent,
        sticky: Sticky::None,
        tiny_subtract: false,
    }
}

fn round_prepared(value: Prepared, mode: RoundingMode) -> DoubleResult {
    if value.significand == 0 {
        return DoubleResult::exact(if value.negative { SIGN_MASK } else { 0 });
    }
    let mut exponent = value.exponent + bit_length(value.significand) as i32 - 1;
    let unit_exponent = if exponent < MIN_NORMAL_EXPONENT {
        MIN_SUBNORMAL_EXPONENT
    } else {
        exponent - FRACTION_BITS as i32
    };
    let (mut significand, inexact) = round_at_unit(value, unit_exponent, mode);
    if value.tiny_subtract && exponent > MIN_NORMAL_EXPONENT && significand < HIDDEN_BIT {
        significand = ((significand + 1) << 1) - 1;
        exponent -= 1;
    }
    if exponent >= MIN_NORMAL_EXPONENT && significand >= HIDDEN_BIT * 2 {
        significand >>= 1;
        exponent += 1;
    }
    if exponent > MAX_NORMAL_EXPONENT {
        let toward_infinity = match mode {
            RoundingMode::NearestEven | RoundingMode::NearestMaxMagnitude => true,
            RoundingMode::TowardZero => false,
            RoundingMode::Down => value.negative,
            RoundingMode::Up => !value.negative,
        };
        return DoubleResult {
            bits: if toward_infinity {
                infinity(value.negative)
            } else {
                (if value.negative { SIGN_MASK } else { 0 }) | MAX_FINITE
            },
            flags: FLAG_OF | FLAG_NX,
        };
    }
    let mut flags = if inexact { FLAG_NX } else { 0 };
    let bits = if exponent < MIN_NORMAL_EXPONENT || significand < HIDDEN_BIT {
        if inexact && is_tiny_after_rounding(value, mode) {
            flags |= FLAG_UF;
        }
        (if value.negative { SIGN_MASK } else { 0 }) | significand as u64
    } else {
        let exponent_bits = (exponent + EXPONENT_BIAS) as u64;
        (if value.negative { SIGN_MASK } else { 0 })
            | (exponent_bits << FRACTION_BITS)
            | (significand as u64 & FRACTION_MASK)
    };
    DoubleResult { bits, flags }
}

fn round_at_unit(value: Prepared, unit_exponent: i32, mode: RoundingMode) -> (u128, bool) {
    let shift = unit_exponent - value.exponent;
    let (mut significand, inexact, half_order, exact_half) = if value.tiny_subtract {
        if shift <= 0 {
            (
                (value.significand << (-shift) as u32) - 1,
                true,
                std::cmp::Ordering::Greater,
                false,
            )
        } else {
            let shift = shift as u32;
            let significand = if shift >= BITS_PER_U128 {
                0
            } else {
                value.significand >> shift
            };
            let low_zero = if shift >= BITS_PER_U128 {
                value.significand == 0
            } else {
                value.significand & ((1_u128 << shift) - 1) == 0
            };
            if low_zero {
                (
                    significand.saturating_sub(1),
                    true,
                    std::cmp::Ordering::Greater,
                    false,
                )
            } else {
                (
                    significand,
                    true,
                    match remainder_order(value.significand, shift, Sticky::None) {
                        std::cmp::Ordering::Equal => std::cmp::Ordering::Less,
                        ordering => ordering,
                    },
                    false,
                )
            }
        }
    } else if shift <= 0 {
        (
            value.significand << (-shift) as u32,
            value.sticky.is_set(),
            match value.sticky {
                Sticky::None => std::cmp::Ordering::Equal,
                Sticky::BelowHalf => std::cmp::Ordering::Less,
            },
            false,
        )
    } else {
        let shift = shift as u32;
        let significand = if shift >= BITS_PER_U128 {
            0
        } else {
            value.significand >> shift
        };
        let low_nonzero = if shift >= BITS_PER_U128 {
            value.significand != 0
        } else {
            value.significand & ((1_u128 << shift) - 1) != 0
        };
        (
            significand,
            low_nonzero || value.sticky.is_set(),
            remainder_order(value.significand, shift, value.sticky),
            remainder_order(value.significand, shift, Sticky::None).is_eq(),
        )
    };
    let increment = inexact
        && match mode {
            RoundingMode::NearestEven => {
                half_order.is_gt() || half_order.is_eq() && significand & 1 != 0
            }
            RoundingMode::TowardZero => false,
            RoundingMode::Down => value.negative,
            RoundingMode::Up => !value.negative,
            RoundingMode::NearestMaxMagnitude => half_order.is_gt() || exact_half,
        };
    if increment {
        significand += 1;
    }
    (significand, inexact)
}

fn is_tiny_after_rounding(value: Prepared, mode: RoundingMode) -> bool {
    let mut normalized_exponent = value.exponent + bit_length(value.significand) as i32 - 1;
    if value.tiny_subtract && value.significand.is_power_of_two() {
        normalized_exponent -= 1;
    }
    if normalized_exponent < MIN_NORMAL_EXPONENT - 1 {
        return true;
    }
    if normalized_exponent > MIN_NORMAL_EXPONENT - 1 {
        return false;
    }
    let (significand, _) = round_at_unit(value, MIN_SUBNORMAL_EXPONENT - 1, mode);
    significand < HIDDEN_BIT * 2
}

fn round_prepared_to_single(value: Prepared, mode: RoundingMode) -> FpResult {
    const SINGLE_HIDDEN_BIT: u128 = 1 << SINGLE_FRACTION_BITS;
    const SINGLE_MAX_FINITE: u32 = 0x7f7f_ffff;

    if value.significand == 0 {
        return FpResult {
            bits: if value.negative {
                crate::fp::SIGN_MASK
            } else {
                0
            },
            flags: 0,
        };
    }
    let mut exponent = value.exponent + bit_length(value.significand) as i32 - 1;
    let unit_exponent = if exponent < SINGLE_MIN_NORMAL_EXPONENT {
        SINGLE_MIN_SUBNORMAL_EXPONENT
    } else {
        exponent - SINGLE_FRACTION_BITS as i32
    };
    let (mut significand, inexact) = round_at_unit(value, unit_exponent, mode);
    if value.tiny_subtract
        && exponent > SINGLE_MIN_NORMAL_EXPONENT
        && significand < SINGLE_HIDDEN_BIT
    {
        significand = ((significand + 1) << 1) - 1;
        exponent -= 1;
    }
    if exponent >= SINGLE_MIN_NORMAL_EXPONENT && significand >= SINGLE_HIDDEN_BIT * 2 {
        significand >>= 1;
        exponent += 1;
    }
    if exponent > SINGLE_MAX_NORMAL_EXPONENT {
        let toward_infinity = match mode {
            RoundingMode::NearestEven | RoundingMode::NearestMaxMagnitude => true,
            RoundingMode::TowardZero => false,
            RoundingMode::Down => value.negative,
            RoundingMode::Up => !value.negative,
        };
        return FpResult {
            bits: if toward_infinity {
                infinity_single(value.negative)
            } else {
                (if value.negative {
                    crate::fp::SIGN_MASK
                } else {
                    0
                }) | SINGLE_MAX_FINITE
            },
            flags: FLAG_OF | FLAG_NX,
        };
    }
    let mut flags = if inexact { FLAG_NX } else { 0 };
    let bits = if exponent < SINGLE_MIN_NORMAL_EXPONENT || significand < SINGLE_HIDDEN_BIT {
        if inexact && is_tiny_after_rounding_to_single(value, mode) {
            flags |= FLAG_UF;
        }
        (if value.negative {
            crate::fp::SIGN_MASK
        } else {
            0
        }) | significand as u32
    } else {
        let exponent_bits = (exponent + SINGLE_EXPONENT_BIAS) as u32;
        (if value.negative {
            crate::fp::SIGN_MASK
        } else {
            0
        }) | (exponent_bits << SINGLE_FRACTION_BITS)
            | (significand as u32 & ((1 << SINGLE_FRACTION_BITS) - 1))
    };
    FpResult { bits, flags }
}

fn is_tiny_after_rounding_to_single(value: Prepared, mode: RoundingMode) -> bool {
    let mut normalized_exponent = value.exponent + bit_length(value.significand) as i32 - 1;
    if value.tiny_subtract && value.significand.is_power_of_two() {
        normalized_exponent -= 1;
    }
    if normalized_exponent < SINGLE_MIN_NORMAL_EXPONENT - 1 {
        return true;
    }
    if normalized_exponent > SINGLE_MIN_NORMAL_EXPONENT - 1 {
        return false;
    }
    let (significand, _) = round_at_unit(value, SINGLE_MIN_SUBNORMAL_EXPONENT - 1, mode);
    significand < (1_u128 << (SINGLE_FRACTION_BITS + 1))
}

fn remainder_order(significand: u128, shift: u32, sticky: Sticky) -> std::cmp::Ordering {
    if shift == 0 {
        return match sticky {
            Sticky::None => std::cmp::Ordering::Equal,
            Sticky::BelowHalf => std::cmp::Ordering::Less,
        };
    }
    let half_bit = shift - 1;
    let bits = bit_length(significand);
    if bits < half_bit + 1 {
        return std::cmp::Ordering::Less;
    }
    if bits == half_bit + 1 && significand == 1_u128 << half_bit {
        return match sticky {
            Sticky::None => std::cmp::Ordering::Equal,
            Sticky::BelowHalf => std::cmp::Ordering::Greater,
        };
    }
    if half_bit >= BITS_PER_U128 {
        return std::cmp::Ordering::Less;
    }
    let remainder = if shift >= BITS_PER_U128 {
        significand
    } else {
        significand & ((1_u128 << shift) - 1)
    };
    match remainder.cmp(&(1_u128 << half_bit)) {
        std::cmp::Ordering::Equal => match sticky {
            Sticky::None => std::cmp::Ordering::Equal,
            Sticky::BelowHalf => std::cmp::Ordering::Greater,
        },
        ordering => ordering,
    }
}

fn round_integer(value: Finite, mode: RoundingMode) -> (u128, bool) {
    if value.significand == 0 {
        return (0, false);
    }
    if value.exponent >= 0 {
        return (
            value
                .significand
                .checked_shl(value.exponent as u32)
                .unwrap_or(u128::MAX),
            false,
        );
    }
    let shift = value.exponent.unsigned_abs();
    let significand = if shift >= BITS_PER_U128 {
        0
    } else {
        value.significand >> shift
    };
    let inexact = if shift >= BITS_PER_U128 {
        true
    } else {
        value.significand & ((1_u128 << shift) - 1) != 0
    };
    let half_order = remainder_order(value.significand, shift, Sticky::None);
    let increment = inexact
        && match mode {
            RoundingMode::NearestEven => {
                half_order.is_gt() || half_order.is_eq() && significand & 1 != 0
            }
            RoundingMode::TowardZero => false,
            RoundingMode::Down => value.negative,
            RoundingMode::Up => !value.negative,
            RoundingMode::NearestMaxMagnitude => !half_order.is_lt(),
        };
    (significand + u128::from(increment), inexact)
}

fn compare_non_nan(a: u64, b: u64) -> std::cmp::Ordering {
    if is_zero(a) && is_zero(b) {
        return std::cmp::Ordering::Equal;
    }
    let a_negative = a & SIGN_MASK != 0;
    let b_negative = b & SIGN_MASK != 0;
    if a_negative != b_negative {
        return if a_negative {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Greater
        };
    }
    let ordering = (a & !SIGN_MASK).cmp(&(b & !SIGN_MASK));
    if a_negative {
        ordering.reverse()
    } else {
        ordering
    }
}

fn encode_integer(value: i128, width: u32) -> u64 {
    match width {
        WORD_BITS => sign_extend_word(value as u32),
        DOUBLEWORD_BITS => value as u64,
        _ => unreachable!(),
    }
}

fn sign_extend_word(value: u32) -> u64 {
    i64::from(value as i32) as u64
}

fn fp_exact(bits: u32) -> FpResult {
    FpResult { bits, flags: 0 }
}

fn bit_length(value: u128) -> u32 {
    BITS_PER_U128 - value.leading_zeros()
}

fn integer_sqrt(value: u128) -> u128 {
    if value < 2 {
        return value;
    }
    let mut low = 1_u128;
    let mut high = 1_u128 << bit_length(value).div_ceil(2);
    while low + 1 < high {
        let middle = (low + high) / 2;
        if middle <= value / middle {
            low = middle;
        } else {
            high = middle;
        }
    }
    low
}

#[cfg(test)]
mod tests {
    use super::*;

    const ONE: u64 = 0x3ff0_0000_0000_0000;
    const TWO: u64 = 0x4000_0000_0000_0000;
    const HALF: u64 = 0x3fe0_0000_0000_0000;
    const FOUR: u64 = 0x4010_0000_0000_0000;
    const NEGATIVE_ONE: u64 = 0xbff0_0000_0000_0000;
    const QUIET_NAN: u64 = 0x7ff8_0000_0000_1234;
    const SIGNALING_NAN: u64 = 0x7ff0_0000_0000_1234;
    const MIN_NORMAL: u64 = 0x0010_0000_0000_0000;
    const RANDOM_SEED: u64 = 0x6d2b_79f5_3a4c_918e;
    const RANDOM_ITERATIONS: u32 = 20_000;
    const XORSHIFT_A: u32 = 13;
    const XORSHIFT_B: u32 = 7;
    const XORSHIFT_C: u32 = 17;

    #[test]
    fn basic_arithmetic_is_exact() {
        assert_eq!(add(ONE, ONE, RoundingMode::NearestEven).bits, TWO);
        assert_eq!(sub(TWO, ONE, RoundingMode::NearestEven).bits, ONE);
        assert_eq!(mul(TWO, HALF, RoundingMode::NearestEven).bits, ONE);
        assert_eq!(div(ONE, TWO, RoundingMode::NearestEven).bits, HALF);
        assert_eq!(sqrt(FOUR, RoundingMode::NearestEven).bits, TWO);
        assert_eq!(
            add(0, 0xc69617be7c83ba8d, RoundingMode::NearestEven).bits,
            0xc69617be7c83ba8d
        );
    }

    #[test]
    fn invalid_operations_and_nans_follow_risc_v() {
        for result in [
            div(0, 0, RoundingMode::NearestEven),
            div(infinity(false), infinity(false), RoundingMode::NearestEven),
            mul(0, infinity(false), RoundingMode::NearestEven),
            sqrt(NEGATIVE_ONE, RoundingMode::NearestEven),
            add(infinity(false), infinity(true), RoundingMode::NearestEven),
        ] {
            assert_eq!(result.bits, CANONICAL_NAN);
            assert_eq!(result.flags, FLAG_NV);
        }
        assert_eq!(add(QUIET_NAN, ONE, RoundingMode::NearestEven).flags, 0);
        assert_eq!(
            add(SIGNALING_NAN, ONE, RoundingMode::NearestEven).flags,
            FLAG_NV
        );
    }

    #[test]
    fn nearest_even_results_match_host_ieee_operations() {
        let mut state = RANDOM_SEED;
        for _ in 0..RANDOM_ITERATIONS {
            state = xorshift(state);
            let a = state;
            state = xorshift(state);
            let b = state;
            state = xorshift(state);
            let c = state;
            if exponent_field(a) == ALL_ONES_EXPONENT
                || exponent_field(b) == ALL_ONES_EXPONENT
                || exponent_field(c) == ALL_ONES_EXPONENT
                || is_zero(b)
            {
                continue;
            }
            let av = f64::from_bits(a);
            let bv = f64::from_bits(b);
            let cv = f64::from_bits(c);
            assert_eq!(
                add(a, b, RoundingMode::NearestEven).bits,
                (av + bv).to_bits(),
                "add {a:#018x} + {b:#018x}"
            );
            assert_eq!(
                mul(a, b, RoundingMode::NearestEven).bits,
                (av * bv).to_bits(),
                "mul {a:#018x} * {b:#018x}"
            );
            assert_eq!(
                div(a, b, RoundingMode::NearestEven).bits,
                (av / bv).to_bits(),
                "div {a:#018x} / {b:#018x}"
            );
            assert_eq!(
                fma(a, b, c, RoundingMode::NearestEven, false, false).bits,
                av.mul_add(bv, cv).to_bits(),
                "fma {a:#018x} * {b:#018x} + {c:#018x}"
            );
            if a & SIGN_MASK == 0 {
                assert_eq!(
                    sqrt(a, RoundingMode::NearestEven).bits,
                    av.sqrt().to_bits(),
                    "sqrt {a:#018x}"
                );
            }
        }
    }

    #[test]
    fn minimum_normal_tininess_uses_unbounded_rounding() {
        let result = mul(0x0010_0000_0000_0001, 0x3feffffffffffffe, RoundingMode::Up);

        assert_eq!(result.bits, MIN_NORMAL);
        assert_eq!(result.flags, FLAG_NX);
    }

    #[test]
    fn integer_and_single_conversions_cover_boundaries() {
        assert_eq!(
            to_int(infinity(false), WORD_BITS, false, RoundingMode::NearestEven).value,
            sign_extend_word(i32::MAX as u32)
        );
        assert_eq!(
            from_int(16_777_217, WORD_BITS, false, RoundingMode::NearestEven).flags,
            0
        );
        assert_eq!(from_single(0x3f80_0000).bits, ONE);
        assert_eq!(from_single(0x007f_ffff).bits, 0x380f_ffff_c000_0000);
        assert_eq!(to_single(ONE, RoundingMode::NearestEven).bits, 0x3f80_0000);
        let underflow = to_single(1, RoundingMode::NearestEven);
        assert_eq!(underflow.bits, 0);
        assert_eq!(underflow.flags, FLAG_NX | FLAG_UF);
        let overflow = to_single(0x7da2_dd80_dfb5_f2fc, RoundingMode::NearestEven);
        assert_eq!(overflow.bits, 0x7f80_0000);
        assert_eq!(overflow.flags, FLAG_NX | FLAG_OF);
    }

    fn xorshift(mut value: u64) -> u64 {
        value ^= value << XORSHIFT_A;
        value ^= value >> XORSHIFT_B;
        value ^= value << XORSHIFT_C;
        value
    }
}
