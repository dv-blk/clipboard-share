use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// A clipboard payload transmitted between peers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ClipboardMessage {
    Text(String),
    Image {
        width: u32,
        height: u32,
        /// Raw RGBA pixels, row-major.
        rgba: Vec<u8>,
    },
    Heartbeat,
}

impl ClipboardMessage {
    /// SHA-256 fingerprint used to detect changes and prevent echo loops.
    pub fn fingerprint(&self) -> Vec<u8> {
        let mut hasher = Sha256::new();
        match self {
            ClipboardMessage::Text(t) => {
                hasher.update(b"text:");
                hasher.update(t.as_bytes());
            }
            ClipboardMessage::Image { width, height, rgba } => {
                hasher.update(b"image:");
                hasher.update(width.to_le_bytes());
                hasher.update(height.to_le_bytes());
                hasher.update(rgba);
            }
            ClipboardMessage::Heartbeat => {
                hasher.update(b"heartbeat");
            }
        }
        hasher.finalize().to_vec()
    }
}

/// Write a length-prefixed bincode frame to an async writer.
pub async fn write_message<W: AsyncWrite + Unpin>(
    writer: &mut W,
    msg: &ClipboardMessage,
) -> anyhow::Result<()> {
    let encoded = bincode::serialize(msg)?;
    let len = encoded.len() as u32;
    writer.write_all(&len.to_be_bytes()).await?;
    writer.write_all(&encoded).await?;
    Ok(())
}

/// Read a length-prefixed bincode frame from an async reader.
pub async fn read_message<R: AsyncRead + Unpin>(
    reader: &mut R,
) -> anyhow::Result<ClipboardMessage> {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;

    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf).await?;
    let msg = bincode::deserialize(&buf)?;
    Ok(msg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::duplex;

    #[tokio::test]
    async fn roundtrip_text() {
        let (mut a, mut b) = duplex(4096);
        let original = ClipboardMessage::Text("hello world".to_string());
        write_message(&mut a, &original).await.unwrap();
        let received = read_message(&mut b).await.unwrap();
        assert_eq!(original, received);
    }

    #[tokio::test]
    async fn roundtrip_image() {
        let (mut a, mut b) = duplex(65536);
        let original = ClipboardMessage::Image {
            width: 2,
            height: 2,
            rgba: vec![255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255],
        };
        write_message(&mut a, &original).await.unwrap();
        let received = read_message(&mut b).await.unwrap();
        assert_eq!(original, received);
    }

    #[tokio::test]
    async fn roundtrip_multiple_messages() {
        let (mut a, mut b) = duplex(65536);
        let msgs = vec![
            ClipboardMessage::Text("first".to_string()),
            ClipboardMessage::Heartbeat,
            ClipboardMessage::Text("second".to_string()),
        ];
        for msg in &msgs {
            write_message(&mut a, msg).await.unwrap();
        }
        for expected in &msgs {
            let received = read_message(&mut b).await.unwrap();
            assert_eq!(expected, &received);
        }
    }

    #[test]
    fn fingerprint_differs_by_content() {
        let a = ClipboardMessage::Text("foo".to_string());
        let b = ClipboardMessage::Text("bar".to_string());
        assert_ne!(a.fingerprint(), b.fingerprint());
    }

    #[test]
    fn fingerprint_stable() {
        let msg = ClipboardMessage::Text("stable".to_string());
        assert_eq!(msg.fingerprint(), msg.fingerprint());
    }
}
