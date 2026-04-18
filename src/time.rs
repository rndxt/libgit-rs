use std::fmt;

use chrono::Local;

#[derive(Debug, Clone, Copy)]
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
    fn time() -> testing::Result<()> {
        let time = Time::current_time();
        println!("{:?}", time);
        Ok(())
    }
}
