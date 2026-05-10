use crate::message::{read_message, write_message, ClipboardMessage};
use crate::transport::{MessageReceiver, MessageSender};
use async_trait::async_trait;
use tokio::io::{ReadHalf, WriteHalf};
use tokio::net::TcpStream;

pub struct TcpSender {
    writer: WriteHalf<TcpStream>,
}

pub struct TcpReceiver {
    reader: ReadHalf<TcpStream>,
}

/// Split a connected `TcpStream` into typed sender/receiver halves.
pub fn split_tcp(stream: TcpStream) -> (TcpSender, TcpReceiver) {
    let (reader, writer) = tokio::io::split(stream);
    (TcpSender { writer }, TcpReceiver { reader })
}

#[async_trait]
impl MessageSender for TcpSender {
    async fn send(&mut self, msg: ClipboardMessage) -> anyhow::Result<()> {
        write_message(&mut self.writer, &msg).await
    }
}

#[async_trait]
impl MessageReceiver for TcpReceiver {
    async fn recv(&mut self) -> anyhow::Result<Option<ClipboardMessage>> {
        match read_message(&mut self.reader).await {
            Ok(msg) => Ok(Some(msg)),
            Err(e) => {
                // EOF / connection closed — treat as clean disconnect.
                if let Some(io_err) = e.downcast_ref::<std::io::Error>() {
                    if io_err.kind() == std::io::ErrorKind::UnexpectedEof
                        || io_err.kind() == std::io::ErrorKind::ConnectionReset
                    {
                        return Ok(None);
                    }
                }
                Err(e)
            }
        }
    }
}
