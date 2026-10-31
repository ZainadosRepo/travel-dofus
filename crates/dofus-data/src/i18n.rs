//! `Content/I18n/<lang>.bin`: a language code, an `(id, offset)` table, then 7-bit-length UTF-8 strings.

use crate::{err, Result};
use std::collections::HashMap;

pub struct Texts {
    data: Vec<u8>,
    offsets: HashMap<i32, usize>,
}

fn varint(d: &[u8], mut p: usize) -> Option<(usize, usize)> {
    let (mut value, mut shift) = (0usize, 0);
    loop {
        let b = *d.get(p)?;
        p += 1;
        value |= usize::from(b & 0x7f) << shift;
        if b < 0x80 {
            return Some((value, p));
        }
        shift += 7;
        if shift > 28 {
            return None;
        }
    }
}

impl Texts {
    pub fn parse(data: Vec<u8>) -> Result<Self> {
        let Some((lang_len, p)) = varint(&data, 0) else {
            return err("i18n: bad header");
        };
        let mut p = p + lang_len;
        let int = |at: usize| {
            data.get(at..at + 4)
                .map(|b| i32::from_le_bytes(b.try_into().expect("4 bytes")))
        };
        let Some(count) = int(p).and_then(|c| usize::try_from(c).ok()) else {
            return err("i18n: bad count");
        };
        p += 4;
        if data.len() < p + count * 8 {
            return err("i18n: table truncated");
        }
        let mut offsets = HashMap::with_capacity(count);
        for k in 0..count {
            let (id, off) = (int(p + 8 * k), int(p + 8 * k + 4));
            if let (Some(id), Some(off)) = (id, off.and_then(|o| usize::try_from(o).ok())) {
                offsets.insert(id, off);
            }
        }
        Ok(Texts { data, offsets })
    }

    pub fn get(&self, id: i64) -> Option<String> {
        let at = *self.offsets.get(&i32::try_from(id).ok()?)?;
        let (len, start) = varint(&self.data, at)?;
        let bytes = self.data.get(start..start + len)?;
        Some(String::from_utf8_lossy(bytes).into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_texts_by_id() {
        let mut d = vec![2, b'f', b'r'];
        d.extend(2i32.to_le_bytes());
        let first: i32 = 3 + 4 + 16;
        d.extend(7i32.to_le_bytes());
        d.extend(first.to_le_bytes());
        d.extend(9i32.to_le_bytes());
        d.extend((first + 6).to_le_bytes());
        d.push(5);
        d.extend(b"Zaap!");
        d.push(6);
        d.extend("Écume".as_bytes());
        let t = Texts::parse(d).unwrap();
        assert_eq!(t.get(7).as_deref(), Some("Zaap!"));
        assert_eq!(t.get(9).as_deref(), Some("Écume"));
        assert_eq!(t.get(8), None);
    }
}
