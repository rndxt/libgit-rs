use std::str::FromStr;

pub fn u32_from_ascii(bytes: &[u8]) -> Option<u32> {
    let str = str::from_utf8(bytes).ok()?;
    u32::from_str(str).ok()
}

pub fn u64_from_ascii(bytes: &[u8]) -> Option<u64> {
    let str = str::from_utf8(bytes).ok()?;
    u64::from_str(str).ok()
}

pub struct BinaryReader<'a> {
    data: &'a [u8],
}

impl<'a> BinaryReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data }
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn skip_prefix(&mut self, prefix: &[u8]) -> Option<()> {
        self.data = self.data.strip_prefix(prefix)?;
        Some(())
    }

    pub fn skip_byte(&mut self, byte: u8) -> Option<()> {
        self.skip_prefix(&[byte])
    }

    pub fn split_n(&mut self, n: usize) -> Option<&'a [u8]> {
        self.data.split_off(..n)
    }

    pub fn split_until_inclusive(&mut self, byte: u8) -> Option<&'a [u8]> {
        let pos = self.data.iter().position(|b| *b == byte)?;
        let (left, rest) = self.data.split_at(pos);
        self.data = &rest[1..];
        Some(left)
    }

    pub fn first(&mut self) -> Option<u8> {
        self.split_n(1).map(|first| first[0])
    }

    pub fn bytes(self) -> &'a [u8] {
        self.data
    }
}
