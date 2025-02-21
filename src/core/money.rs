use crate::config::{UNIT, UNIT_ZEROS};
use std::ops::{Add, AddAssign, Div, Sub, SubAssign};
use std::str::FromStr;
use thiserror::Error;

#[derive(
    serde::Serialize,
    serde::Deserialize,
    Debug,
    Clone,
    Copy,
    PartialEq,
    PartialOrd,
    Ord,
    Eq,
    Default,
)]
pub struct Amount(pub u64);

#[derive(Error, Debug)]
pub enum ParseAmountError {
    #[error("amount invalid")]
    Invalid,
}

impl std::fmt::Display for Amount {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut s = self.0.to_string();
        while s.len() <= UNIT_ZEROS as usize {
            s.insert(0, '0');
        }
        s.insert(s.len() - UNIT_ZEROS as usize, '.');
        while let Some(last) = s.chars().last() {
            if last == '0' {
                s.pop();
            } else {
                break;
            }
        }
        if let Some(last) = s.chars().last() {
            if last == '.' {
                s.push('0');
            }
        }
        write!(f, "{}", s)
    }
}

impl FromStr for Amount {
    type Err = ParseAmountError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut s = s.trim().to_string();
        if let Some(dot_pos) = s.find('.') {
            if s == "." {
                return Err(ParseAmountError::Invalid);
            }
            let dot_rpos = s.len() - 1 - dot_pos;
            if dot_rpos > UNIT_ZEROS as usize {
                return Err(ParseAmountError::Invalid);
            }
            for _ in 0..UNIT_ZEROS as usize - dot_rpos {
                s.push('0');
            }
            s.remove(dot_pos);
            Ok(Self(s.parse().map_err(|_| ParseAmountError::Invalid)?))
        } else {
            let as_u64: u64 = s.parse().map_err(|_| ParseAmountError::Invalid)?;
            Ok(Self(as_u64 * UNIT))
        }
    }
}

impl From<Amount> for u64 {
    fn from(a: Amount) -> u64 {
        a.0
    }
}

impl From<u64> for Amount {
    fn from(val: u64) -> Self {
        Self(val)
    }
}

impl AddAssign for Amount {
    fn add_assign(&mut self, other: Self) {
        self.0 += other.0;
    }
}

impl SubAssign for Amount {
    fn sub_assign(&mut self, other: Self) {
        self.0 -= other.0;
    }
}

impl Add for Amount {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }
}

impl Sub for Amount {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Self(self.0 - other.0)
    }
}

impl Div<u64> for Amount {
    type Output = Self;

    fn div(self, other: u64) -> Self {
        Self(self.0 / other)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_amount_to_str() {
        assert_eq!(format!("{}", Amount(0)), "0.0");
        assert_eq!(format!("{}", Amount(1)), "0.000000001");
        assert_eq!(format!("{}", Amount(12)), "0.000000012");
        assert_eq!(format!("{}", Amount(1234)), "0.000001234");
        assert_eq!(format!("{}", Amount(123000000000)), "123.0");
        assert_eq!(format!("{}", Amount(123456789)), "0.123456789");
        assert_eq!(format!("{}", Amount(1234567898)), "1.234567898");
        assert_eq!(
            format!("{}", Amount(123456789987654321)),
            "123456789.987654321"
        );
    }

    #[test]
    fn test_str_to_amount() {
        assert_eq!("0".parse::<Amount>().unwrap(), Amount(0));
        assert_eq!("0.".parse::<Amount>().unwrap(), Amount(0));
        assert_eq!("0.0".parse::<Amount>().unwrap(), Amount(0));
        assert_eq!("1".parse::<Amount>().unwrap(), Amount(1000000000));
        assert_eq!("1.".parse::<Amount>().unwrap(), Amount(1000000000));
        assert_eq!("1.0".parse::<Amount>().unwrap(), Amount(1000000000));
        assert_eq!("123".parse::<Amount>().unwrap(), Amount(123000000000));
        assert_eq!("123.".parse::<Amount>().unwrap(), Amount(123000000000));
        assert_eq!("123.0".parse::<Amount>().unwrap(), Amount(123000000000));
        assert_eq!("123.1".parse::<Amount>().unwrap(), Amount(123100000000));
        assert_eq!("123.100".parse::<Amount>().unwrap(), Amount(123100000000));
        assert_eq!(
            "123.100000000".parse::<Amount>().unwrap(),
            Amount(123100000000)
        );
        assert_eq!(
            "123.123456".parse::<Amount>().unwrap(),
            Amount(123123456000)
        );
        assert_eq!(
            "123.123456000".parse::<Amount>().unwrap(),
            Amount(123123456000)
        );
        assert_eq!(
            "123.123456789".parse::<Amount>().unwrap(),
            Amount(123123456789)
        );
        assert_eq!("123.0001".parse::<Amount>().unwrap(), Amount(123000100000));
        assert_eq!(
            "123.000000001".parse::<Amount>().unwrap(),
            Amount(123000000001)
        );
        assert_eq!("0.0001".parse::<Amount>().unwrap(), Amount(100000));
        assert_eq!("0.000000001".parse::<Amount>().unwrap(), Amount(1));
        assert_eq!(".0001".parse::<Amount>().unwrap(), Amount(100000));
        assert_eq!(".000000001".parse::<Amount>().unwrap(), Amount(1));
        assert_eq!(".123456789".parse::<Amount>().unwrap(), Amount(123456789));
        assert_eq!(" 123 ".parse::<Amount>().unwrap(), Amount(123000000000));
        assert_eq!(" 123.456 ".parse::<Amount>().unwrap(), Amount(123456000000));
        assert!("123.234.123".parse::<Amount>().is_err());
        assert!("k123".parse::<Amount>().is_err());
        assert!("12 34".parse::<Amount>().is_err());
        assert!(".".parse::<Amount>().is_err());
        assert!(" . ".parse::<Amount>().is_err());
        assert!("12 .".parse::<Amount>().is_err());
        assert!(". 12".parse::<Amount>().is_err());
    }
}
