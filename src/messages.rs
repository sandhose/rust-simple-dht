use std::error::Error;
use std::fmt;
use std::iter;
use std::str::FromStr;
use std::io;
use std::marker::Sized;
use std::net::{IpAddr, SocketAddr};
use std::str;
use tokio_core::net::UdpCodec;

/// A Pushable object can be encoded and decoded from a frame
pub trait Pushable {
    /// Push the value in the given frame
    fn push_in_frame(&self, frame: &mut Vec<u8>);
    /// Get the value length
    fn frame_len(&self) -> usize;
    /// Pull the value from a slice of bytes
    fn pull(buf: &[u8]) -> Result<Self, DecodeError>
    where
        Self: Sized;
}

/// Utility macro to pull data from buffer
macro_rules! pull {
    ($buf:expr, $range:expr) => {$buf.get($range).ok_or(DecodeError::MessageTooShort)}
}

/// Hashes are 8 bytes long
const HASH_SIZE: usize = 8;

/// Stores a hash
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct Hash([u8; HASH_SIZE]);

impl Hash {
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
        hash.get(..HASH_SIZE)
            .map(|h| Hash::new([h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]]))
    }
}

impl FromStr for Hash {
    type Err = HashParseError;

    /// Create a Hash from a string
    ///
    /// # Examples
    ///
    /// ```
    /// use std::str::FromStr;
    /// use simple_dht::messages::Hash;
    /// assert_eq!(Hash::from_str("0123456789abcdef"),
    ///            Ok(Hash::new([0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef])));
    /// ```
    fn from_str(s: &str) -> Result<Hash, HashParseError> {
        let count = s.chars().count();
        if count > HASH_SIZE * 2 {
            return Err(HashParseError);
        }

        // Fill with leading zeros
        let chars: Vec<char> = iter::repeat('0')
            .take(HASH_SIZE * 2 - count)
            .chain(s.chars())
            .collect();

        let mut hash = [0; HASH_SIZE];

        for i in 0..HASH_SIZE {
            if let (Some(upper), Some(lower)) =
                (chars[i * 2].to_digit(16), chars[i * 2 + 1].to_digit(16))
            {
                hash[i] = (lower + (upper << 4)) as u8;
            } else {
                return Err(HashParseError);
            }
        }

        Ok(Hash::new(hash))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HashParseError;

impl fmt::Display for HashParseError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.write_str(self.description())
    }
}

impl Error for HashParseError {
    fn description(&self) -> &str {
        "invalid hash syntax"
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
        pull!(buf, ..HASH_SIZE).map(|hash| Hash::from_slice(hash).unwrap())
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

/// The payload inside PUT messages
#[derive(Debug, Clone, PartialEq)]
pub struct Payload(pub Vec<u8>);

impl Default for Payload {
    fn default() -> Self {
        Payload(Vec::new())
    }
}

impl fmt::Display for Payload {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", String::from_utf8_lossy(self.0.as_slice()))
    }
}

impl FromStr for Payload {
    type Err = io::Error;
    fn from_str(s: &str) -> Result<Payload, io::Error> {
        Ok(Payload(s.as_bytes().to_vec()))
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
        // The first two bytes are the payload length
        let length = pull!(buf, 0..2)?;
        let length = (u64::from(length[0]) << 8) + u64::from(length[1]);
        // the rest is the payload itself
        let data = pull!(buf, 2..(2 + length as usize))?;
        let mut vec = Vec::with_capacity(length as usize);
        vec.extend_from_slice(data);
        Ok(Payload(vec))
    }
}

// The message type is stored as a u8
impl Pushable for u8 {
    fn push_in_frame(&self, frame: &mut Vec<u8>) {
        frame.push(*self);
    }

    fn frame_len(&self) -> usize {
        1
    }

    fn pull(buf: &[u8]) -> Result<Self, DecodeError> {
        pull!(buf, ..1).map(|v| v[0])
    }
}

impl Pushable for SocketAddr {
    fn push_in_frame(&self, frame: &mut Vec<u8>) {
        match *self {
            SocketAddr::V4(sock) => {
                frame.push(4);
                frame.extend_from_slice(&sock.ip().octets()[..]);
            }
            SocketAddr::V6(sock) => {
                frame.push(6);
                frame.extend_from_slice(&sock.ip().octets()[..]);
            }
        }

        let port = self.port();
        frame.push((port >> 8) as u8);
        frame.push(port as u8);
    }

    fn frame_len(&self) -> usize {
        match *self {
            SocketAddr::V4(_) => 1 + 2 + 4,
            SocketAddr::V6(_) => 1 + 2 + 16,
        }
    }

    fn pull(buf: &[u8]) -> Result<Self, DecodeError> {
        let family = pull!(buf, ..1)?[0];
        let (ip, port) = match family {
            4 => {
                let mut ip: [u8; 4] = [0; 4];
                let octets = pull!(buf, 1..5)?;
                ip.copy_from_slice(octets);
                (IpAddr::from(ip), pull!(buf, 5..7)?)
            }
            6 => {
                let mut ip: [u8; 16] = [0; 16];
                let octets = pull!(buf, 1..17)?;
                ip.copy_from_slice(octets);
                (IpAddr::from(ip), pull!(buf, 17..19)?)
            }
            _ => return Err(DecodeError::InvalidContent),
        };

        let port = (u16::from(port[0]) << 8) + u16::from(port[1]);

        Ok(SocketAddr::from((ip, port)))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    Get(Hash),
    Put(Hash, Payload),
    KeepAlive,
    IHave(Hash),
    Discover(SocketAddr),
}

#[derive(Debug, PartialEq, Eq)]
pub enum DecodeError {
    MessageTooLong,
    MessageTooShort,
    InvalidMessageType,
    InvalidContent,
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
            DecodeError::InvalidContent => "invalid message content",
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
        match Message::deserialize(buf) {
            Ok(msg) => Ok((*addr, msg)),
            Err(e) => Err(e.into()),
        }
    }

    fn encode(&mut self, (addr, msg): Self::Out, buf: &mut Vec<u8>) -> SocketAddr {
        buf.extend(msg.serialize());
        addr
    }
}

/// Build a message from list of parts
macro_rules! build_msg {
    ($( $x:expr ),*) => ({
        let mut size = 0;
        $(
            size += $x.frame_len();
        )*
        let mut msg = Vec::with_capacity(size);
        $(
            $x.push_in_frame(&mut msg);
        )*
        msg
    })
}

impl Message {
    /// Serialize a message into a vector of bytes
    ///
    /// ```
    /// use simple_dht::messages::Message;
    /// let buf = Message::KeepAlive.serialize();
    /// assert_eq!(buf, vec![2]);
    /// ```
    pub fn serialize(&self) -> Vec<u8> {
        let id = self.type_identifier();
        match *self {
            Message::Get(ref hash) => build_msg!(id, hash),
            Message::Put(ref hash, ref payload) => build_msg!(id, hash, payload),
            Message::KeepAlive => build_msg!(id),
            Message::IHave(ref hash) => build_msg!(id, hash),
            Message::Discover(ref addr) => build_msg!(id, addr),
        }
    }

    /// Deserialize a buffer into a message
    ///
    /// ```
    /// use simple_dht::messages::Message;
    /// let buf = &[2];
    /// assert_eq!(Message::deserialize(buf), Ok(Message::KeepAlive));
    /// ```
    pub fn deserialize(buf: &[u8]) -> Result<Self, DecodeError> {
        let id = u8::pull(buf)?;

        let msg = match id {
            0 => {
                let hash = Hash::pull(&buf[1..])?;
                Message::Get(hash)
            }
            1 => {
                let hash = Hash::pull(&buf[1..])?;
                let payload = Payload::pull(&buf[(1 + HASH_SIZE)..])?;
                Message::Put(hash, payload)
            }
            2 => Message::KeepAlive,
            3 => {
                let hash = Hash::pull(&buf[1..])?;
                Message::IHave(hash)
            }
            4 => {
                let addr = SocketAddr::pull(&buf[1..])?;
                Message::Discover(addr)
            }
            _ => return Err(DecodeError::InvalidMessageType),
        };

        Ok(msg)
    }

    /// The message type -> id conversion
    fn type_identifier(&self) -> u8 {
        match *self {
            Message::Get(_) => 0,
            Message::Put(_, _) => 1,
            Message::KeepAlive => 2,
            Message::IHave(_) => 3,
            Message::Discover(_) => 4,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_get() {
        let hash = Hash([0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef]);
        assert_eq!(
            Message::Get(hash).serialize(),
            [0x00, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef]
        );
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
