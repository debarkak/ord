use crate::protocol::packet::{OrdHeader, OrdPacket, ORD_HEADER_SIZE};
use anyhow::{anyhow, Context, Result};
use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::info;

pub struct FramedStream {
    stream: TcpStream,
    read_buf: Vec<u8>,
}

impl FramedStream {
    pub fn new(stream: TcpStream) -> Result<Self> {
        stream.set_nodelay(true).context("Failed to set TCP_NODELAY")?;
        Ok(Self {
            stream,
            read_buf: Vec::with_capacity(64 * 1024),
        })
    }

    pub fn peer_addr(&self) -> Result<SocketAddr> {
        self.stream.peer_addr().map_err(Into::into)
    }

    /// Read next complete OrdPacket from the stream
    pub async fn read_packet(&mut self) -> Result<OrdPacket> {
        loop {
            // Check if we have at least header
            if self.read_buf.len() >= ORD_HEADER_SIZE {
                let header = OrdHeader::decode(&self.read_buf[..ORD_HEADER_SIZE])?;
                let total_len = ORD_HEADER_SIZE + header.payload_len as usize;

                if self.read_buf.len() >= total_len {
                    let payload = self.read_buf[ORD_HEADER_SIZE..total_len].to_vec();
                    self.read_buf.drain(..total_len);
                    return Ok(OrdPacket { header, payload });
                }
            }

            // Read more data
            let mut temp_buf = [0u8; 16384];
            let n = self.stream.read(&mut temp_buf).await?;
            if n == 0 {
                return Err(anyhow!("Connection closed by remote peer"));
            }
            self.read_buf.extend_from_slice(&temp_buf[..n]);
        }
    }

    /// Send a complete OrdPacket over the stream
    pub async fn write_packet(&mut self, packet: &OrdPacket) -> Result<()> {
        let encoded = packet.encode();
        self.stream.write_all(&encoded).await?;
        self.stream.flush().await?;
        Ok(())
    }

    /// Fast send raw header + payload slice without extra vector copies
    pub async fn write_frame_data(
        &mut self,
        sequence: u32,
        flags: u16,
        payload: &[u8],
    ) -> Result<()> {
        let header = OrdHeader::new(crate::protocol::types::MSG_VIDEO_DATA, flags, sequence, payload.len() as u32);
        let header_bytes = header.encode();
        self.stream.write_all(&header_bytes).await?;
        self.stream.write_all(payload).await?;
        self.stream.flush().await?;
        Ok(())
    }
}

pub struct TcpTransportServer {
    bind_addr: String,
    port: u16,
}

impl TcpTransportServer {
    pub fn new(bind_addr: String, port: u16) -> Self {
        Self { bind_addr, port }
    }

    pub async fn bind(&self) -> Result<TcpListener> {
        let addr = format!("{}:{}", self.bind_addr, self.port);
        let listener = TcpListener::bind(&addr).await.context(format!("Failed to bind TCP server on {}", addr))?;
        info!("ORD TCP Server listening on {}", addr);
        Ok(listener)
    }
}
