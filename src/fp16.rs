//! Bit-exact IEEE-754 half-precision operations for the RISC-V Zfh extension.
//!
//! Architectural arithmetic is performed with integer significands so results do
//! not depend on the host floating-point mode or implementation.

use crate::fp::RoundingMode;

/// Sign bit in a half-precision value.
pub(crate) const SIGN_MASK: u16 = 1 << 15;
/// Exponent field in a half-precision value.
pub(crate) const EXPONENT_MASK: u16 = 0x1f << 10;
/// Fraction field in a half-precision value.
pub(crate) const FRACTION_MASK: u16 = (1 << 10) - 1;
/// Canonical quiet NaN required as the result of most invalid operations.
pub(crate) const CANONICAL_NAN: u16 = 0x7e00;
/// Inexact flag.
pub(crate) const FLAG_NX: u8 = 1 << 0;
/// Underflow flag.
pub(crate) const FLAG_UF: u8 = 1 << 1;
/// Overflow flag.
pub(crate) const FLAG_OF: u8 = 1 << 2;
/// Divide-by-zero flag.
pub(crate) const FLAG_DZ: u8 = 1 << 3;
/// Invalid-operation flag.
pub(crate) const FLAG_NV: u8 = 1 << 4;

const EXPONENT_BITS: u32 = 5;
const FRACTION_BITS: u32 = 10;
const EXPONENT_BIAS: i32 = 15;
const MIN_NORMAL_EXPONENT: i32 = -14;
const MAX_NORMAL_EXPONENT: i32 = 15;
const MIN_SUBNORMAL_EXPONENT: i32 = -24;
const ALL_ONES_EXPONENT: u32 = (1 << EXPONENT_BITS) - 1;
const HIDDEN_BIT: u128 = 1 << FRACTION_BITS;
const MAX_FINITE: u16 = (EXPONENT_MASK - (1 << FRACTION_BITS)) | FRACTION_MASK;
const DIVISION_GUARD_BITS: u32 = 64;
const SQRT_TARGET_BITS: u32 = 64;
const EXACT_ALIGNMENT_BITS: u32 = 64;
const BITS_PER_U128: u32 = u128::BITS;
const SIGNED_WORD_BITS: u32 = 32;
const SIGNED_DOUBLEWORD_BITS: u32 = 64;
const SINGLE_BITS: u32 = 32;
const SINGLE_EXPONENT_BITS: u32 = 8;
const SINGLE_FRACTION_BITS: u32 = 23;
const SINGLE_EXPONENT_BIAS: i32 = 127;
const DOUBLE_BITS: u32 = 64;
const DOUBLE_EXPONENT_BITS: u32 = 11;
const DOUBLE_FRACTION_BITS: u32 = 52;
const DOUBLE_EXPONENT_BIAS: i32 = 1023;

/// Result bits and newly raised IEEE flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FpResult {
    pub(crate) bits: u16,
    pub(crate) flags: u8,
}

/// Integer conversion result and newly raised IEEE flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct IntResult {
    pub(crate) value: u64,
    pub(crate) flags: u8,
}

impl FpResult {
    const fn exact(bits: u16) -> Self {
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

/// Adds two half-precision values.
pub(crate) fn add(a: u16, b: u16, mode: RoundingMode) -> FpResult {
    if let Some(result) = nan_binary(a, b) {
        return result;
    }
    match (classify(a), classify(b)) {
        (Value::Infinity(a_negative), Value::Infinity(b_negative)) if a_negative != b_negative => {
            FpResult::invalid()
        }
        (Value::Infinity(negative), _) => FpResult::exact(infinity(negative)),
        (_, Value::Infinity(negative)) => FpResult::exact(infinity(negative)),
        (Value::Finite(a), Value::Finite(b)) => {
            if a.significand == 0 && b.significand == 0 {
                let negative = if a.negative == b.negative {
                    a.negative
                } else {
                    matches!(mode, RoundingMode::Down)
                };
                return FpResult::exact(if negative { SIGN_MASK } else { 0 });
            }
            round_prepared(exact_add(a, b, mode), mode)
        }
        _ => unreachable!("NaNs were handled above"),
    }
}

/// Subtracts two half-precision values.
pub(crate) fn sub(a: u16, b: u16, mode: RoundingMode) -> FpResult {
    add(a, b ^ SIGN_MASK, mode)
}

/// Multiplies two half-precision values.
pub(crate) fn mul(a: u16, b: u16, mode: RoundingMode) -> FpResult {
    if let Some(result) = nan_binary(a, b) {
        return result;
    }
    match (classify(a), classify(b)) {
        (Value::Infinity(a_negative), Value::Finite(b)) if b.significand == 0 => {
            let _ = a_negative;
            FpResult::invalid()
        }
        (Value::Finite(a), Value::Infinity(_)) if a.significand == 0 => FpResult::invalid(),
        (Value::Infinity(a_negative), Value::Finite(b))
        | (Value::Finite(b), Value::Infinity(a_negative)) => {
            FpResult::exact(infinity(a_negative ^ b.negative))
        }
        (Value::Infinity(a_negative), Value::Infinity(b_negative)) => {
            FpResult::exact(infinity(a_negative ^ b_negative))
        }
        (Value::Finite(a), Value::Finite(b)) => {
            let negative = a.negative ^ b.negative;
            if a.significand == 0 || b.significand == 0 {
                return FpResult::exact(if negative { SIGN_MASK } else { 0 });
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

/// Divides two half-precision values.
pub(crate) fn div(a: u16, b: u16, mode: RoundingMode) -> FpResult {
    if let Some(result) = nan_binary(a, b) {
        return result;
    }
    match (classify(a), classify(b)) {
        (Value::Infinity(_), Value::Infinity(_)) => FpResult::invalid(),
        (Value::Finite(a), Value::Finite(b)) if a.significand == 0 && b.significand == 0 => {
            FpResult::invalid()
        }
        (Value::Infinity(a_negative), Value::Finite(b)) => {
            FpResult::exact(infinity(a_negative ^ b.negative))
        }
        (Value::Finite(a), Value::Infinity(b_negative)) => {
            FpResult::exact(if a.negative ^ b_negative {
                SIGN_MASK
            } else {
                0
            })
        }
        (Value::Finite(a), Value::Finite(b)) if b.significand == 0 => FpResult {
            bits: infinity(a.negative ^ b.negative),
            flags: FLAG_DZ,
        },
        (Value::Finite(a), Value::Finite(b)) => {
            if a.significand == 0 {
                return FpResult::exact(if a.negative ^ b.negative {
                    SIGN_MASK
                } else {
                    0
                });
            }
            let numerator = a.significand << DIVISION_GUARD_BITS;
            let significand = numerator / b.significand;
            let remainder = numerator % b.significand;
            round_prepared(
                Prepared {
                    negative: a.negative ^ b.negative,
                    significand,
                    exponent: a.exponent - b.exponent - DIVISION_GUARD_BITS as i32,
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

/// Computes the square root of a half-precision value.
pub(crate) fn sqrt(a: u16, mode: RoundingMode) -> FpResult {
    match classify(a) {
        Value::NaN { quiet, .. } => FpResult {
            bits: CANONICAL_NAN,
            flags: if quiet { 0 } else { FLAG_NV },
        },
        Value::Infinity(false) => FpResult::exact(a),
        Value::Infinity(true) => FpResult::invalid(),
        Value::Finite(a) if a.significand == 0 => FpResult::exact(a.bits()),
        Value::Finite(a) if a.negative => FpResult::invalid(),
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
    a: u16,
    b: u16,
    c: u16,
    mode: RoundingMode,
    negate_product: bool,
    negate_addend: bool,
) -> FpResult {
    if let Some(result) = nan_ternary(a, b, c) {
        return result;
    }

    let product = classify_product(a, b, negate_product);
    let addend_bits = if negate_addend { c ^ SIGN_MASK } else { c };
    match (product, classify(addend_bits)) {
        (Product::Invalid, _) => FpResult::invalid(),
        (Product::Infinity(negative), Value::Infinity(addend_negative))
            if negative != addend_negative =>
        {
            FpResult::invalid()
        }
        (Product::Infinity(negative), _) => FpResult::exact(infinity(negative)),
        (_, Value::Infinity(negative)) => FpResult::exact(infinity(negative)),
        (Product::Finite(product), Value::Finite(addend)) => {
            if product.significand == 0 && addend.significand == 0 {
                let negative = if product.negative == addend.negative {
                    product.negative
                } else {
                    matches!(mode, RoundingMode::Down)
                };
                return FpResult::exact(if negative { SIGN_MASK } else { 0 });
            }
            round_prepared(exact_add(product, addend, mode), mode)
        }
        _ => unreachable!("NaNs were handled above"),
    }
}

/// Returns the smaller value with RISC-V NaN and signed-zero rules.
pub(crate) fn min(a: u16, b: u16) -> FpResult {
    min_max(a, b, true)
}

/// Returns the larger value with RISC-V NaN and signed-zero rules.
pub(crate) fn max(a: u16, b: u16) -> FpResult {
    min_max(a, b, false)
}

fn min_max(a: u16, b: u16, minimum: bool) -> FpResult {
    let a_nan = is_nan(a);
    let b_nan = is_nan(b);
    if a_nan && b_nan {
        return FpResult {
            bits: CANONICAL_NAN,
            flags: if is_signaling_nan(a) || is_signaling_nan(b) {
                FLAG_NV
            } else {
                0
            },
        };
    }
    if a_nan || b_nan {
        return FpResult {
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
        return FpResult::exact(if negative { SIGN_MASK } else { 0 });
    }
    let ordering = compare_non_nan(a, b);
    let use_a = if minimum {
        ordering.is_lt()
    } else {
        ordering.is_gt()
    };
    FpResult::exact(if use_a { a } else { b })
}

/// Compares two values. Returns `None` when either input is NaN.
pub(crate) fn compare(a: u16, b: u16, kind: Compare) -> (Option<bool>, u8) {
    if is_nan(a) || is_nan(b) {
        let flags = if matches!(kind, Compare::Equal) {
            if is_signaling_nan(a) || is_signaling_nan(b) {
                FLAG_NV
            } else {
                0
            }
        } else if is_nan(a) || is_nan(b) {
            FLAG_NV
        } else {
            0
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

/// Classifies a half-precision value using the FCLASS.H bit assignments.
pub(crate) fn classify_bits(a: u16) -> u32 {
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

/// Converts an integer to half precision.
pub(crate) fn from_int(value: u64, width: u32, unsigned: bool, mode: RoundingMode) -> FpResult {
    let value = match (width, unsigned) {
        (SIGNED_WORD_BITS, false) => i64::from(value as u32 as i32) as u64,
        (SIGNED_WORD_BITS, true) => u64::from(value as u32),
        (SIGNED_DOUBLEWORD_BITS, false) => value,
        (SIGNED_DOUBLEWORD_BITS, true) => value,
        _ => unreachable!("only 32- and 64-bit conversions exist"),
    };
    let negative = !unsigned && (value as i64) < 0;
    let magnitude = if negative {
        value.wrapping_neg()
    } else {
        value
    };
    if magnitude == 0 {
        return FpResult::exact(0);
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

/// Narrows a single-precision value to half precision.
pub(crate) fn from_single(a: u32, mode: RoundingMode) -> FpResult {
    from_wide(
        u64::from(a),
        SINGLE_BITS,
        SINGLE_EXPONENT_BITS,
        SINGLE_FRACTION_BITS,
        SINGLE_EXPONENT_BIAS,
        mode,
    )
}

/// Narrows a double-precision value to half precision.
pub(crate) fn from_double(a: u64, mode: RoundingMode) -> FpResult {
    from_wide(
        a,
        DOUBLE_BITS,
        DOUBLE_EXPONENT_BITS,
        DOUBLE_FRACTION_BITS,
        DOUBLE_EXPONENT_BIAS,
        mode,
    )
}

/// Converts a half-precision value to single precision exactly.
pub(crate) fn to_single(a: u16) -> crate::fp::FpResult {
    let (negative, exponent, fraction) = half_fields(a);
    if exponent == ALL_ONES_EXPONENT {
        if fraction == 0 {
            return crate::fp::FpResult {
                bits: (if negative { crate::fp::SIGN_MASK } else { 0 }) | crate::fp::EXPONENT_MASK,
                flags: 0,
            };
        }
        return crate::fp::FpResult {
            bits: crate::fp::CANONICAL_NAN,
            flags: if is_signaling_nan(a) { FLAG_NV } else { 0 },
        };
    }
    let destination_fraction_bits = SINGLE_FRACTION_BITS;
    let destination_exponent_bias = SINGLE_EXPONENT_BIAS;
    let exact = widen_to_bits(
        negative,
        exponent,
        fraction,
        destination_fraction_bits,
        destination_exponent_bias,
        u64::from(crate::fp::SIGN_MASK),
        u64::from(crate::fp::FRACTION_MASK),
    );
    crate::fp::FpResult {
        bits: exact as u32,
        flags: 0,
    }
}

/// Converts a half-precision value to double precision exactly.
pub(crate) fn to_double(a: u16) -> crate::fp64::DoubleResult {
    let (negative, exponent, fraction) = half_fields(a);
    let canonical_exponent = crate::fp64::EXPONENT_MASK;
    let canonical_sign = if negative { crate::fp64::SIGN_MASK } else { 0 };
    if exponent == ALL_ONES_EXPONENT {
        if fraction == 0 {
            return crate::fp64::DoubleResult {
                bits: canonical_sign | canonical_exponent,
                flags: 0,
            };
        }
        return crate::fp64::DoubleResult {
            bits: crate::fp64::CANONICAL_NAN,
            flags: if is_signaling_nan(a) { FLAG_NV } else { 0 },
        };
    }
    let exact = widen_to_bits(
        negative,
        exponent,
        fraction,
        DOUBLE_FRACTION_BITS,
        DOUBLE_EXPONENT_BIAS,
        crate::fp64::SIGN_MASK,
        (1_u64 << DOUBLE_FRACTION_BITS) - 1,
    );
    crate::fp64::DoubleResult {
        bits: exact,
        flags: 0,
    }
}

fn from_wide(
    bits: u64,
    width: u32,
    exponent_bits: u32,
    fraction_bits: u32,
    exponent_bias: i32,
    mode: RoundingMode,
) -> FpResult {
    let sign_mask = 1_u64 << (width - 1);
    let exponent_mask = ((1_u64 << exponent_bits) - 1) << fraction_bits;
    let fraction_mask = (1_u64 << fraction_bits) - 1;
    let all_ones_exponent = (1_u64 << exponent_bits) - 1;
    let negative = bits & sign_mask != 0;
    let exponent = (bits & exponent_mask) >> fraction_bits;
    let fraction = bits & fraction_mask;

    if exponent == all_ones_exponent {
        if fraction == 0 {
            return FpResult::exact(infinity(negative));
        }
        let quiet_bit = 1_u64 << (fraction_bits - 1);
        return FpResult {
            bits: CANONICAL_NAN,
            flags: if fraction & quiet_bit == 0 {
                FLAG_NV
            } else {
                0
            },
        };
    }
    if exponent == 0 && fraction == 0 {
        return FpResult::exact(if negative { SIGN_MASK } else { 0 });
    }

    let (significand, exponent) = if exponent == 0 {
        (
            u128::from(fraction),
            1 - exponent_bias - fraction_bits as i32,
        )
    } else {
        (
            (1_u128 << fraction_bits) | u128::from(fraction),
            exponent as i32 - exponent_bias - fraction_bits as i32,
        )
    };
    round_prepared(
        Prepared {
            negative,
            significand,
            exponent,
            sticky: Sticky::None,
            tiny_subtract: false,
        },
        mode,
    )
}

fn half_fields(bits: u16) -> (bool, u32, u16) {
    (
        bits & SIGN_MASK != 0,
        exponent_field(bits),
        bits & FRACTION_MASK,
    )
}

fn widen_to_bits(
    negative: bool,
    exponent: u32,
    fraction: u16,
    destination_fraction_bits: u32,
    destination_exponent_bias: i32,
    destination_sign_mask: u64,
    destination_fraction_mask: u64,
) -> u64 {
    let sign = if negative { destination_sign_mask } else { 0 };
    if exponent == 0 && fraction == 0 {
        return sign;
    }
    let source_min_subnormal_exponent = 1 - EXPONENT_BIAS - FRACTION_BITS as i32;
    let (significand, source_exponent) = if exponent == 0 {
        (u128::from(fraction), source_min_subnormal_exponent)
    } else {
        (
            HIDDEN_BIT | u128::from(fraction),
            exponent as i32 - EXPONENT_BIAS - FRACTION_BITS as i32,
        )
    };
    let normalized_exponent = source_exponent + bit_length(significand) as i32 - 1;
    let destination_exponent = (normalized_exponent + destination_exponent_bias) as u64;
    let fraction_shift = destination_fraction_bits + 1 - bit_length(significand);
    let normalized_fraction = (significand << fraction_shift) as u64;
    sign | (destination_exponent << destination_fraction_bits)
        | (normalized_fraction & destination_fraction_mask)
}

/// Converts a half-precision value to a sign-extended integer.
pub(crate) fn to_int(a: u16, width: u32, unsigned: bool, mode: RoundingMode) -> IntResult {
    let signed_max = match width {
        SIGNED_WORD_BITS => (1_u64 << (SIGNED_WORD_BITS - 1)) - 1,
        SIGNED_DOUBLEWORD_BITS => (1_u64 << (SIGNED_DOUBLEWORD_BITS - 1)) - 1,
        _ => unreachable!("only 32- and 64-bit conversions exist"),
    };
    let signed_negative_limit = signed_max + 1;
    let unsigned_max = match width {
        SIGNED_WORD_BITS => u64::from(u32::MAX),
        SIGNED_DOUBLEWORD_BITS => u64::MAX,
        _ => unreachable!("only 32- and 64-bit conversions exist"),
    };
    let invalid = match (width, unsigned) {
        (SIGNED_WORD_BITS, false) => sign_extend_word(signed_max as u32),
        (SIGNED_WORD_BITS, true) => sign_extend_word(u32::MAX),
        (SIGNED_DOUBLEWORD_BITS, false) => signed_max,
        (SIGNED_DOUBLEWORD_BITS, true) => u64::MAX,
        _ => unreachable!(),
    };
    let negative_invalid = match width {
        SIGNED_WORD_BITS => sign_extend_word(1 << (SIGNED_WORD_BITS - 1)),
        SIGNED_DOUBLEWORD_BITS => 1 << (SIGNED_DOUBLEWORD_BITS - 1),
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

fn encode_integer(value: i128, width: u32) -> u64 {
    match width {
        SIGNED_WORD_BITS => sign_extend_word(value as u32),
        SIGNED_DOUBLEWORD_BITS => value as u64,
        _ => unreachable!(),
    }
}

fn sign_extend_word(value: u32) -> u64 {
    i64::from(value as i32) as u64
}

fn classify(bits: u16) -> Value {
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
    fn bits(self) -> u16 {
        let sign = if self.negative { SIGN_MASK } else { 0 };
        if self.significand == 0 {
            return sign;
        }
        if self.exponent == MIN_SUBNORMAL_EXPONENT {
            sign | self.significand as u16
        } else {
            let exponent = (self.exponent + EXPONENT_BIAS + FRACTION_BITS as i32) as u16;
            sign | (exponent << FRACTION_BITS) | (self.significand as u16 & FRACTION_MASK)
        }
    }
}

fn exponent_field(bits: u16) -> u32 {
    u32::from((bits & EXPONENT_MASK) >> FRACTION_BITS)
}

fn infinity(negative: bool) -> u16 {
    (if negative { SIGN_MASK } else { 0 }) | EXPONENT_MASK
}

fn is_nan(bits: u16) -> bool {
    exponent_field(bits) == ALL_ONES_EXPONENT && bits & FRACTION_MASK != 0
}

fn is_signaling_nan(bits: u16) -> bool {
    is_nan(bits) && bits & (1 << (FRACTION_BITS - 1)) == 0
}

fn is_zero(bits: u16) -> bool {
    bits & !SIGN_MASK == 0
}

fn is_infinity(bits: u16) -> bool {
    bits & !SIGN_MASK == EXPONENT_MASK
}

fn nan_binary(a: u16, b: u16) -> Option<FpResult> {
    if !is_nan(a) && !is_nan(b) {
        return None;
    }
    Some(FpResult {
        bits: CANONICAL_NAN,
        flags: if is_signaling_nan(a) || is_signaling_nan(b) {
            FLAG_NV
        } else {
            0
        },
    })
}

fn nan_ternary(a: u16, b: u16, c: u16) -> Option<FpResult> {
    if !is_nan(a) && !is_nan(b) && !is_nan(c) {
        return None;
    }
    let invalid_product = (is_infinity(a) && is_zero(b)) || (is_zero(a) && is_infinity(b));
    Some(FpResult {
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

fn classify_product(a: u16, b: u16, negate: bool) -> Product {
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
    let difference = (a.exponent - b.exponent).unsigned_abs();
    if difference > EXACT_ALIGNMENT_BITS {
        let (large, small) = if a.exponent > b.exponent {
            (a, b)
        } else {
            (b, a)
        };
        if large.negative == small.negative {
            return Prepared {
                negative: large.negative,
                significand: large.significand,
                exponent: large.exponent,
                sticky: Sticky::BelowHalf,
                tiny_subtract: false,
            };
        }
        return Prepared {
            negative: large.negative,
            significand: large.significand,
            exponent: large.exponent,
            sticky: Sticky::None,
            tiny_subtract: true,
        };
    }

    let common = a.exponent.min(b.exponent);
    let a_magnitude = a.significand << (a.exponent - common) as u32;
    let b_magnitude = b.significand << (b.exponent - common) as u32;
    if a.negative == b.negative {
        Prepared {
            negative: a.negative,
            significand: a_magnitude + b_magnitude,
            exponent: common,
            sticky: Sticky::None,
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
        let (negative, larger, smaller) = if a_magnitude > b_magnitude {
            (a.negative, a_magnitude, b_magnitude)
        } else {
            (b.negative, b_magnitude, a_magnitude)
        };
        Prepared {
            negative,
            significand: larger - smaller,
            exponent: common,
            sticky: Sticky::None,
            tiny_subtract: false,
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

fn round_prepared(value: Prepared, mode: RoundingMode) -> FpResult {
    if value.significand == 0 {
        return FpResult::exact(if value.negative { SIGN_MASK } else { 0 });
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
        return FpResult {
            bits: if toward_infinity {
                infinity(value.negative)
            } else {
                let sign = if value.negative { SIGN_MASK } else { 0 };
                sign | MAX_FINITE
            },
            flags: FLAG_OF | FLAG_NX,
        };
    }

    let mut flags = if inexact { FLAG_NX } else { 0 };
    let bits = if exponent < MIN_NORMAL_EXPONENT || significand < HIDDEN_BIT {
        if inexact && is_tiny_after_rounding(value, mode) {
            flags |= FLAG_UF;
        }
        (if value.negative { SIGN_MASK } else { 0 }) | significand as u16
    } else {
        let unbiased = exponent;
        let exponent_bits = (unbiased + EXPONENT_BIAS) as u16;
        (if value.negative { SIGN_MASK } else { 0 })
            | (exponent_bits << FRACTION_BITS)
            | (significand as u16 & FRACTION_MASK)
    };
    FpResult { bits, flags }
}

fn round_at_unit(value: Prepared, unit_exponent: i32, mode: RoundingMode) -> (u128, bool) {
    let shift = unit_exponent - value.exponent;
    let (mut significand, inexact, half_order, exact_half) = if value.tiny_subtract {
        if shift <= 0 {
            let significand = (value.significand << (-shift) as u32) - 1;
            (significand, true, std::cmp::Ordering::Greater, false)
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

    let unbounded_quantum = MIN_SUBNORMAL_EXPONENT - 1;
    let (significand, _) = round_at_unit(value, unbounded_quantum, mode);
    significand < HIDDEN_BIT * 2
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
        return (value.significand << value.exponent as u32, false);
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

fn compare_non_nan(a: u16, b: u16) -> std::cmp::Ordering {
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
    let a_magnitude = a & !SIGN_MASK;
    let b_magnitude = b & !SIGN_MASK;
    let ordering = a_magnitude.cmp(&b_magnitude);
    if a_negative {
        ordering.reverse()
    } else {
        ordering
    }
}

fn bit_length(value: u128) -> u32 {
    BITS_PER_U128 - value.leading_zeros()
}

fn integer_sqrt(value: u128) -> u128 {
    if value < 2 {
        return value;
    }
    let mut low = 1_u128;
    let mut high = 1_u128 << (bit_length(value).div_ceil(2));
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

    const ONE: u16 = 0x3c00;
    const TWO: u16 = 0x4000;
    const THREE: u16 = 0x4200;
    const FOUR: u16 = 0x4400;
    const HALF: u16 = 0x3800;
    const HALF_ULP_AT_ONE: u16 = 0x1000;
    const NEGATIVE_ONE: u16 = 0xbc00;
    const QUIET_NAN: u16 = 0x7e01;
    const SIGNALING_NAN: u16 = 0x7c01;
    const MIN_NORMAL: u16 = 0x0400;
    const MAX_FINITE_VALUE: u16 = 0x7bff;

    #[test]
    fn basic_arithmetic_is_exact() {
        assert_eq!(add(ONE, ONE, RoundingMode::NearestEven).bits, TWO);
        assert_eq!(sub(TWO, ONE, RoundingMode::NearestEven).bits, ONE);
        assert_eq!(mul(TWO, HALF, RoundingMode::NearestEven).bits, ONE);
        assert_eq!(div(ONE, TWO, RoundingMode::NearestEven).bits, HALF);
        assert_eq!(sqrt(FOUR, RoundingMode::NearestEven).bits, TWO);
    }

    #[test]
    fn signed_zero_rules_match_risc_v() {
        assert_eq!(
            add(SIGN_MASK, SIGN_MASK, RoundingMode::NearestEven).bits,
            SIGN_MASK
        );
        assert_eq!(add(SIGN_MASK, 0, RoundingMode::NearestEven).bits, 0);
        assert_eq!(add(SIGN_MASK, 0, RoundingMode::Down).bits, SIGN_MASK);
        assert_eq!(
            mul(SIGN_MASK, TWO, RoundingMode::NearestEven).bits,
            SIGN_MASK
        );
    }

    #[test]
    fn invalid_operations_raise_nv_and_return_canonical_nan() {
        for result in [
            div(0, 0, RoundingMode::NearestEven),
            div(infinity(false), infinity(false), RoundingMode::NearestEven),
            mul(0, infinity(false), RoundingMode::NearestEven),
            sqrt(NEGATIVE_ONE, RoundingMode::NearestEven),
            add(infinity(false), infinity(true), RoundingMode::NearestEven),
        ] {
            assert_eq!(result.bits, CANONICAL_NAN);
            assert_eq!(result.flags & FLAG_NV, FLAG_NV);
        }
    }

    #[test]
    fn flags_and_subnormal_boundaries_are_preserved() {
        let exact = div(MIN_NORMAL, TWO, RoundingMode::NearestEven);
        assert_eq!(exact.bits, MIN_NORMAL >> 1);
        assert_eq!(exact.flags, 0);

        let tiny = div(MIN_NORMAL, THREE, RoundingMode::NearestEven);
        assert_eq!(exponent_field(tiny.bits), 0);
        assert_eq!(tiny.flags, FLAG_NX | FLAG_UF);

        let overflow = mul(MAX_FINITE_VALUE, TWO, RoundingMode::NearestEven);
        assert_eq!(overflow.bits, infinity(false));
        assert_eq!(overflow.flags, FLAG_NX | FLAG_OF);
    }

    #[test]
    fn all_rounding_modes_differ_at_a_tie() {
        assert_eq!(
            add(ONE, HALF_ULP_AT_ONE, RoundingMode::NearestEven).bits,
            ONE
        );
        assert_eq!(
            add(ONE, HALF_ULP_AT_ONE, RoundingMode::NearestMaxMagnitude).bits,
            0x3c01
        );
        assert_eq!(add(ONE, HALF_ULP_AT_ONE, RoundingMode::Up).bits, 0x3c01);
        assert_eq!(add(ONE, HALF_ULP_AT_ONE, RoundingMode::Down).bits, ONE);
        assert_eq!(
            add(ONE, HALF_ULP_AT_ONE, RoundingMode::TowardZero).bits,
            ONE
        );
    }

    #[test]
    fn comparison_classification_and_conversions_follow_risc_v() {
        assert_eq!(compare(SIGNALING_NAN, ONE, Compare::Equal), (None, FLAG_NV));
        assert_eq!(compare(QUIET_NAN, ONE, Compare::Less), (None, FLAG_NV));
        assert_eq!(classify_bits(infinity(true)), 1 << 0);
        assert_eq!(classify_bits(SIGN_MASK), 1 << 3);
        assert_eq!(classify_bits(SIGNALING_NAN), 1 << 8);
        assert_eq!(classify_bits(QUIET_NAN), 1 << 9);

        assert_eq!(
            from_single(1.0_f32.to_bits(), RoundingMode::NearestEven).bits,
            ONE
        );
        assert_eq!(to_single(ONE).bits, 1.0_f32.to_bits());
        assert_eq!(to_double(ONE).bits, 1.0_f64.to_bits());
        assert_eq!(
            from_double(
                (1.0_f64 + 1.0 / 2048.0).to_bits(),
                RoundingMode::NearestEven
            ),
            FpResult {
                bits: ONE,
                flags: FLAG_NX
            }
        );
    }
}
