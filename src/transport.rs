use std::str;
use std::fmt;
use std::net::{UdpSocket, ToSocketAddrs};
use std::io::Result as IOResult;

pub struct Buffer([u8; 1024]);

impl Buffer {
    fn new() -> Self {
        Buffer([0; 1024])
    }
}

impl fmt::Debug for Buffer {
    fn fmt(&self, w: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(w, "Buffer({:?}…)", &self.0[..8])
    }
}

#[derive(Debug)]
pub struct Server {
    socket: UdpSocket,
    buffer: Buffer,
}

impl Server {
    pub fn new(socket: UdpSocket) -> Self {
        Server {
            socket: socket,
            buffer: Buffer::new(),
        }
    }

    pub fn from_address<T: ToSocketAddrs>(address: T) -> IOResult<Self> {
        let socket = UdpSocket::bind(address)?;
        Ok(Server::new(socket))
    }

    pub fn listen(&mut self) -> IOResult<()> {
        loop {
            let (bytes, src) = self.socket.recv_from(&mut self.buffer.0)?;
            println!("Got {} bytes from {}: {:?}",
                     bytes,
                     src,
                     &self.buffer.0[..bytes]);
            self.socket.send_to(&self.buffer.0[..bytes], src)?;
        }
    }
}
