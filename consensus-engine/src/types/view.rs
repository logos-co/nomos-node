use derive_more::{Add, AddAssign, Sub, SubAssign};

#[derive(
    Default,
    Debug,
    Copy,
    Clone,
    Eq,
    PartialEq,
    Hash,
    Ord,
    PartialOrd,
    Add,
    AddAssign,
    Sub,
    SubAssign,
)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct View(pub(crate) i64);

impl View {
    pub const ZERO: Self = Self(0);

    pub const fn new(val: i64) -> Self {
        Self(val)
    }

    pub const fn zero() -> Self {
        Self(0)
    }

    pub fn encode_var_vec(&self) -> Vec<u8> {
        use integer_encoding::VarInt;
        self.0.encode_var_vec()
    }

    pub const fn next(&self) -> Self {
        Self(self.0 + 1)
    }

    pub const fn prev(&self) -> Self {
        Self(self.0 - 1)
    }
}

impl From<i64> for View {
    fn from(id: i64) -> Self {
        Self(id)
    }
}

impl From<View> for i64 {
    fn from(id: View) -> Self {
        id.0
    }
}

impl core::fmt::Display for View {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// TODO: uncomment this when #[feature(step_trait)] is stabilized
// impl std::iter::Step for View {
//   fn steps_between(start: &Self, end: &Self) -> Option<usize> {
//       if start > end {
//           None
//       } else {
//           Some((end.0 - start.0) as usize)
//       }
//   }

//   fn forward_checked(start: Self, count: usize) -> Option<Self> {
//       start.0.checked_add(count as i64).map(View)
//   }

//   fn backward_checked(start: Self, count: usize) -> Option<Self> {
//       start.0.checked_sub(count as i64).map(View)
//   }
// }
