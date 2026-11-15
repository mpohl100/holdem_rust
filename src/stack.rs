//! Chip-stack arithmetic translated from `Stack.h` and `Stack.cpp`.

use std::fmt;
use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Sub, SubAssign};

/// A stack of poker chips, corresponding to C++ `game52::Stack`.
///
/// The public tuple field maps one-to-one to C++ public field `amount_`.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Stack(
    /// C++ `Stack::amount_`, the number of chips in this stack.
    pub i32,
);

impl Stack {
    /// Returns a stack containing `amount` chips.
    ///
    /// C++ mapping: `Stack(int)`.
    #[must_use]
    pub const fn new(amount: i32) -> Self {
        Self(amount)
    }

    /// Mutates this stack by withdrawing up to `requested` chips.
    ///
    /// C++ mapping: `Stack::getAmount(Stack)`. When the request exceeds the
    /// available amount, the C++ implementation clears this stack and returns
    /// that now-zero stack; this source behavior is retained.
    #[must_use]
    pub fn get_amount(&mut self, requested: Self) -> Self {
        if requested.0 > self.0 {
            self.0 = 0;
            *self
        } else {
            self.0 -= requested.0;
            requested
        }
    }
}

impl fmt::Display for Stack {
    /// Formats the chip amount as decimal digits.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, formatter)
    }
}

impl AddAssign for Stack {
    /// Adds another stack's chip amount in place.
    fn add_assign(&mut self, other: Self) {
        self.0 += other.0;
    }
}

impl SubAssign for Stack {
    /// Subtracts another stack's chip amount in place.
    fn sub_assign(&mut self, other: Self) {
        self.0 -= other.0;
    }
}

impl MulAssign<i32> for Stack {
    /// Multiplies this stack's chip amount in place.
    fn mul_assign(&mut self, multiplier: i32) {
        self.0 *= multiplier;
    }
}

impl DivAssign<i32> for Stack {
    /// Divides this stack's chip amount in place using integer division.
    fn div_assign(&mut self, divisor: i32) {
        self.0 /= divisor;
    }
}

impl Add for Stack {
    type Output = Self;

    /// Adds two stacks and returns the resulting amount.
    fn add(mut self, other: Self) -> Self::Output {
        self += other;
        self
    }
}

impl Sub for Stack {
    type Output = Self;

    /// Subtracts the right stack from the left stack.
    fn sub(mut self, other: Self) -> Self::Output {
        self -= other;
        self
    }
}

impl Mul<i32> for Stack {
    type Output = Self;

    /// Multiplies a stack by an integer.
    fn mul(mut self, multiplier: i32) -> Self::Output {
        self *= multiplier;
        self
    }
}

impl Div<i32> for Stack {
    type Output = Self;

    /// Divides a stack by an integer using integer division.
    fn div(mut self, divisor: i32) -> Self::Output {
        self /= divisor;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::Stack;

    /// Checks the source stack operators and withdrawal behavior.
    #[test]
    fn stack_arithmetic_and_withdrawal() {
        let mut stack = Stack(20);
        assert_eq!(stack.get_amount(Stack(7)), Stack(7));
        assert_eq!(stack, Stack(13));
        assert_eq!(stack + Stack(2), Stack(15));
        assert_eq!(stack * 2, Stack(26));
        assert_eq!((stack / 2).to_string(), "6");
        assert_eq!(stack.get_amount(Stack(99)), Stack(0));
        assert_eq!(stack, Stack(0));
    }
}
