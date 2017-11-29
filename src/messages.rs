use std::error::Error;
use std::fmt;
use std::io;
use std::marker::Sized;
use std::net::SocketAddr;
use std::str;
use tokio_core::net::UdpCodec;

pub trait Pushable {
    /// Push the value in the given frame
    fn push_in_frame(&self, frame: &mut Vec<u8>);
    /// Get the value length
    fn frame_len(&self) -> usize;
    fn pull(buf: &[u8]) -> Result<Self, DecodeError> where Self: Sized;
}

const HASH_SIZE: usize = 8;
#[derive(Clone, PartialEq, Eq, Hash)]
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
            Some(Hash::new(hash))
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

    pub fn from_slice(hash: &[u8]) -> Option<Self> {
        hash.get(..HASH_SIZE).map(|h| Hash::new([h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]]))
    }
}

impl Pushable for Hash {
    fn push_in_frame(&self, frame: &mut Vec<u8>) {
        frame.extend_from_slice(&self.0);
    }

    fn frame_len(&self) -> usize {
        HASH_SIZE as usize
    }

    fn pull(buf: &[u8]) -> Result<Self, DecodeError> {
        buf.get(..HASH_SIZE)
            .ok_or(DecodeError::MessageTooShort)
            .map(|hash| Hash::from_slice(hash).unwrap())
    }
}

impl fmt::Debug for Hash {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        for i in 0..HASH_SIZE {
            write!(f, "{:02x}", self.0[i])?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct Payload(pub Vec<u8>);

impl Default for Payload {
    fn default() -> Self {
        Payload(Vec::new())
    }
}

impl Pushable for Payload {
    fn push_in_frame(&self, frame: &mut Vec<u8>) {
        let len = self.0.len();
        frame.push((len >> 8) as u8);
        frame.push(len as u8);
        frame.extend(&self.0);
    }

    fn frame_len(&self) -> usize {
        (self.0.len() + 2) as usize
    }

    fn pull(buf: &[u8]) -> Result<Self, DecodeError> {
        let length = try!(buf.get(0..2).ok_or(DecodeError::MessageTooShort));
        let length = ((length[0] as u64) << 8) + (length[1] as u64);
        let data = try!(buf.get(2..(2 + length as usize)).ok_or(DecodeError::MessageTooShort));
        let mut vec = Vec::with_capacity(length as usize);
        vec.extend_from_slice(data);
        Ok(Payload(vec))
    }
}

impl Pushable for u8 {
    fn push_in_frame(&self, frame: &mut Vec<u8>) {
        frame.push(*self);
    }

    fn frame_len(&self) -> usize {
        1
    }

    fn pull(buf: &[u8]) -> Result<Self, DecodeError> {
        Ok(*try!(buf.get(0).ok_or(DecodeError::MessageTooShort)))
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Get(Hash),
    Put(Hash, Payload),
    KeepAlive,
}

#[derive(Debug)]
pub enum DecodeError {
    MessageTooLong,
    MessageTooShort,
    InvalidMessageType,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{}", Error::description(self))
    }
}

impl Error for DecodeError {
    fn description(&self) -> &str {
        match *self {
            DecodeError::MessageTooLong => "input message is too long",
            DecodeError::MessageTooShort => "input message is too short",
            DecodeError::InvalidMessageType => "message type unknown",
        }
    }
}

impl From<DecodeError> for io::Error {
    fn from(src: DecodeError) -> io::Error {
        io::Error::new(io::ErrorKind::Other, src)
    }
}


pub struct UdpMessage;

impl UdpCodec for UdpMessage {
    type In = (SocketAddr, Message);
    type Out = (SocketAddr, Message);

    fn decode(&mut self, addr: &SocketAddr, buf: &[u8]) -> io::Result<Self::In> {
        match Message::deserialize(&buf) {
            Ok(msg) => Ok((*addr, msg)),
            Err(e) => Err(e.into()),
        }
    }

    fn encode(&mut self, (addr, msg): Self::Out, buf: &mut Vec<u8>) -> SocketAddr {
        buf.extend(msg.serialize());
        addr
    }
}

impl Message {
    pub fn serialize(&self) -> Vec<u8> {
        match *self {
            Message::Get(ref hash) => {
                let msg_type = self.type_identifier();
                let mut msg = Vec::with_capacity(msg_type.frame_len() + hash.frame_len());

                msg_type.push_in_frame(&mut msg);
                hash.push_in_frame(&mut msg);
                msg
            }
            Message::Put(ref hash, ref payload) => {
                let msg_type = self.type_identifier();
                let mut msg = Vec::with_capacity(msg_type.frame_len() + hash.frame_len() +
                                                 payload.frame_len());

                msg_type.push_in_frame(&mut msg);
                hash.push_in_frame(&mut msg);
                payload.push_in_frame(&mut msg);
                msg
            }
            Message::KeepAlive => {
                let msg_type = self.type_identifier();
                let mut msg = Vec::with_capacity(msg_type.frame_len());
                msg_type.push_in_frame(&mut msg);
                msg
            }
        }
    }

    fn deserialize(buf: &[u8]) -> Result<Self, DecodeError> {
        let id = try!(u8::pull(buf));

        let msg = match id {
            0 => {
                let hash = try!(Hash::pull(&buf[1..]));
                Message::Get(hash)
            }
            1 => {
                let hash = try!(Hash::pull(&buf[1..]));
                let payload = try!(Payload::pull(&buf[(1 + HASH_SIZE)..]));
                Message::Put(hash, payload)
            }
            _ => return Err(DecodeError::InvalidMessageType),
        };

        Ok(msg)
    }

    fn type_identifier(&self) -> u8 {
        match *self {
            Message::Get(_) => 0,
            Message::Put(_, _) => 1,
            Message::KeepAlive => 2,
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
