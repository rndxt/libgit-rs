use std::fmt;

use chrono::Local;

use crate::binary::{BinaryReader, u32_from_ascii, u64_from_ascii};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Time {
    pub unix_time: u64,
    pub offset: i32,
}

impl Time {
    pub fn new(seconds: u64, offset: i32) -> Time {
        Self {
            unix_time: seconds,
            offset,
        }
    }

    pub fn current_time() -> Self {
        let local_time = Local::now();
        let seconds = local_time.timestamp();
        let offset = local_time.offset().local_minus_utc();
        Self {
            unix_time: seconds as u64,
            offset,
        }
    }

    pub fn try_from_bytes(bytes: &[u8]) -> Option<Time> {
        let mut reader = BinaryReader::new(bytes);
        let unix_time = reader
            .split_until_inclusive(b' ')
            .and_then(u64_from_ascii)?;

        let sign = reader.first().and_then(|b| match b {
            b'+' => Some(1),
            b'-' => Some(-1),
            _ => None,
        })?;

        let hours = reader.split_n(2).and_then(u32_from_ascii)?;

        let minutes = reader.split_n(2).and_then(u32_from_ascii)?;

        let offset = sign * (hours * 3600 + minutes) as i32;
        let time = Time { unix_time, offset };
        Some(time)
    }
}

impl fmt::Display for Time {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (sign, offset) = if self.offset < 0 {
            ('-', -self.offset)
        } else {
            ('+', self.offset)
        };

        let hours = offset / 3600;
        let minutes = offset % 3600;
        write!(f, "{} {}{:02}{:02}", self.unix_time, sign, hours, minutes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;

    #[test]
    #[cfg_attr(miri, ignore)]
    fn print_current_time() -> testing::Result<()> {
        let time = Time::current_time();
        println!("{:?}", time);
        Ok(())
    }

    #[test]
    fn try_from_bytes_on_valid() -> testing::Result<()> {
        let bytes = b"1771253662 +0300";
        let time = Time::try_from_bytes(bytes).unwrap();
        assert_eq!(time.unix_time, 1771253662);
        assert_eq!(time.offset, 10800);
        Ok(())
    }
}
