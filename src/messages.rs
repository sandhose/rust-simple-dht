use std::fmt::{Debug, Formatter, Error};
use std::str;

pub trait Pushable {
    /// Push the value in the given frame
    fn push_in_frame(&self, frame: &mut Vec<u8>);
    /// Get the value length
    fn frame_len(&self) -> usize;
}

const HASH_SIZE: usize = 8;
#[derive(Clone, PartialEq)]
pub struct Hash([u8; HASH_SIZE]);

impl Hash {
    /// Convert a string into a Hash
    ///
    /// # Examples
    ///
    /// ```
    /// use simple_dht::messages::Hash;
    /// assert_eq!(Hash::from_hex("0123456789abcdef"),
    ///            Some(Hash::new([0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef])));
    /// ```
    ///
    /// ---
    ///
    /// TODO: Allow smaller hash input (`42` = `0000000000000042`)
    pub fn from_hex(_input: &str) -> Option<Self> {
        if let Some(input) = _input.as_bytes().get(0..(HASH_SIZE * 2)) {
            let mut hash = [0; HASH_SIZE];
            for i in 0..HASH_SIZE {
                if let (Some(upper), Some(lower)) =
                    ((input[i * 2] as char).to_digit(16), (input[i * 2 + 1] as char).to_digit(16)) {
                    hash[i] = (lower + (upper << 4)) as u8;
                } else {
                    return None;
                }
            }
            Some(Hash(hash))
        } else {
            None
        }
    }

    /// Create a new Hash
    ///
    /// # Examples
    ///
    /// ```
    /// use simple_dht::messages::Hash;
    /// Hash::new([0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef]);
    /// ```
    pub fn new(hash: [u8; HASH_SIZE]) -> Self {
        Hash(hash)
    }
}

impl Pushable for Hash {
    fn push_in_frame(&self, frame: &mut Vec<u8>) {
        frame.extend_from_slice(&self.0);
    }

    fn frame_len(&self) -> usize {
        HASH_SIZE as usize
    }
}

impl Debug for Hash {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        for i in 0..HASH_SIZE {
            write!(f, "{:02x}", self.0[i])?;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct Payload(Vec<u8>);

impl Pushable for Payload {
    fn push_in_frame(&self, frame: &mut Vec<u8>) {
        let len = self.0.len();
        frame.push((len << 8) as u8);
        frame.push(len as u8);
        frame.extend(&self.0);
    }

    fn frame_len(&self) -> usize {
        (self.0.len() + 2) as usize
    }
}

impl Pushable for u8 {
    fn push_in_frame(&self, frame: &mut Vec<u8>) {
        frame.push(*self);
    }

    fn frame_len(&self) -> usize {
        1
    }
}

#[derive(Debug)]
pub enum Message {
    Get(Hash),
    Put(Hash, Payload),
}

impl Message {
    pub fn serialize(&self) -> Vec<u8> {
        match *self {
            Message::Get(ref hash) => {
                let id = self.type_identifier();
                let mut msg = Vec::with_capacity(id.frame_len() + hash.frame_len());

                id.push_in_frame(&mut msg);
                hash.push_in_frame(&mut msg);
                msg
            }
            Message::Put(ref hash, ref payload) => {
                let id = self.type_identifier();
                let mut msg = Vec::with_capacity(id.frame_len() + hash.frame_len() +
                                                 payload.frame_len());

                id.push_in_frame(&mut msg);
                hash.push_in_frame(&mut msg);
                payload.push_in_frame(&mut msg);
                msg
            }
        }
    }

    fn type_identifier(&self) -> u8 {
        match *self {
            Message::Get(_) => 0,
            Message::Put(_, _) => 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_get() {
        let hash = Hash([0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef]);
        assert_eq!(Message::Get(hash).serialize(),
                   [0x00, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef]);
    }

    #[test]
    fn serialize_put() {
        let hash = Hash([0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef]);
        let payload = "Hello, world!".as_bytes();

        let message = Message::Put(hash.clone(), Payload(payload.clone().to_vec()));
        let frame = message.serialize();
        assert_eq!(frame[0], 1); // Check message type
        assert_eq!(frame[1..(1 + HASH_SIZE)], hash.0); // Check hash
        assert_eq!(frame[(1 + HASH_SIZE)..(3 + HASH_SIZE)], [0, 13]); // Check payload length
        assert_eq!(frame[(3 + HASH_SIZE)..], *payload); // Check payload payload
    }

    #[test]
    fn format_hash() {
        use std::fmt::Write;
        let hash = Hash::new([0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef]);
        let mut w = String::new();
        write!(&mut w, "{:?}", hash).unwrap();
        assert_eq!(&w, "0123456789abcdef");
    }
}
