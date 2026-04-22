// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::num::NonZeroUsize;
use std::ops::{Add, AddAssign, Sub};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LineNumber(NonZeroUsize);

impl LineNumber {
    pub const ONE: Self = match NonZeroUsize::new(1) {
        Some(n) => Self(n),
        None => unreachable!(),
    };

    pub fn new(n: usize) -> Option<Self> {
        NonZeroUsize::new(n).map(Self)
    }

    pub fn from_0_indexed(i: usize) -> Self {
        Self(NonZeroUsize::new(i + 1).expect("0-indexed line overflow"))
    }

    pub fn get(self) -> usize {
        self.0.get()
    }

    pub fn checked_add(self, n: usize) -> Option<Self> {
        self.0.get().checked_add(n).and_then(Self::new)
    }

    pub fn checked_sub(self, n: usize) -> Option<Self> {
        self.0.get().checked_sub(n).and_then(Self::new)
    }

    pub fn next(self) -> Self {
        self.checked_add(1).expect("valid line number")
    }

    pub fn prev(self) -> Option<Self> {
        self.checked_sub(1)
    }

    pub fn saturating_add(self, n: usize) -> Self {
        Self(NonZeroUsize::new(self.0.get().saturating_add(n)).expect("LineNumber overflow"))
    }

    pub fn saturating_sub(self, n: usize) -> usize {
        self.0.get().saturating_sub(n)
    }

    pub fn abs_diff(self, other: Self) -> usize {
        self.0.get().abs_diff(other.0.get())
    }
}

impl Add<usize> for LineNumber {
    type Output = Self;
    fn add(self, rhs: usize) -> Self::Output {
        Self(NonZeroUsize::new(self.0.get() + rhs).expect("LineNumber overflow"))
    }
}

impl AddAssign<usize> for LineNumber {
    fn add_assign(&mut self, rhs: usize) {
        *self = *self + rhs;
    }
}

impl Sub<usize> for LineNumber {
    type Output = usize;
    fn sub(self, rhs: usize) -> Self::Output {
        self.0.get() - rhs
    }
}

impl Sub for LineNumber {
    type Output = usize;
    fn sub(self, rhs: Self) -> Self::Output {
        self.0.get() - rhs.0.get()
    }
}

impl std::fmt::Display for LineNumber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
