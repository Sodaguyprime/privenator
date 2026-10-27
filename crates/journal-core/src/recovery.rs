//! The recovery key: 128 random bits shown once as Crockford base32,
//! 26 characters in groups of 4 (`K7QF-…`). See docs/SPEC.md "First run".

use std::fmt;

use zeroize::Zeroizing;

use crate::crypto;

const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const CHARS: usize = 26; // ceil(128 / 5)

pub struct RecoveryKey(Zeroizing<[u8; 16]>);

#[derive(Debug, thiserror::Error)]
#[error("that doesn't look like a recovery key")]
pub struct InvalidRecoveryKey;

impl RecoveryKey {
    pub(crate) fn generate() -> Self {
        Self(Zeroizing::new(crypto::random()))
    }

    pub(crate) fn bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// Accepts any case, dashes or spaces, and the usual Crockford confusions
    /// (O for 0, I or L for 1).
    pub fn parse(input: &str) -> Result<Self, InvalidRecoveryKey> {
        let mut acc: u128 = 0;
        let mut count = 0;
        let mut last = 0u8;
        for c in input.chars().filter(|c| !c.is_whitespace() && *c != '-') {
            let c = match c.to_ascii_uppercase() {
                'O' => '0',
                'I' | 'L' => '1',
                c => c,
            };
            let v = ALPHABET.iter().position(|&a| a as char == c).ok_or(InvalidRecoveryKey)? as u8;
            count += 1;
            if count > CHARS {
                return Err(InvalidRecoveryKey);
            }
            if count < CHARS {
                acc = (acc << 5) | v as u128;
            } else {
                last = v;
            }
        }
        // The final symbol carries the last 3 bits plus 2 zero padding bits.
        if count != CHARS || last & 0b11 != 0 {
            return Err(InvalidRecoveryKey);
        }
        let value = (acc << 3) | (last >> 2) as u128;
        Ok(Self(Zeroizing::new(value.to_be_bytes())))
    }
}

impl fmt::Display for RecoveryKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = u128::from_be_bytes(*self.0);
        for i in 0..CHARS {
            if i > 0 && i % 4 == 0 {
                f.write_str("-")?;
            }
            // 25 full 5-bit groups, then 3 bits shifted left as padding.
            let v = if i < CHARS - 1 { (value >> (128 - 5 * (i + 1))) & 0x1f } else { (value & 0b111) << 2 };
            write!(f, "{}", ALPHABET[v as usize] as char)?;
        }
        Ok(())
    }
}

impl fmt::Debug for RecoveryKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RecoveryKey(<redacted>)")
    }
}
