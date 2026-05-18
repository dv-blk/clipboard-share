use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// A clipboard payload transmitted between peers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Payload {
    Text(String),
    Image {
        width: u32,
        height: u32,
        /// Raw RGBA pixels, row-major.
        rgba: Vec<u8>,
    },
    Heartbeat,
}

/// Maximum permitted payload size (32 MiB). Protects against a malicious or
/// buggy peer causing a huge allocation via a crafted length prefix.
const MAX_CONTENT_BYTES: usize = 32 * 1024 * 1024;

impl Payload {
    /// SHA-256 fingerprint used to detect changes and prevent echo loops.
    pub fn fingerprint(&self) -> Vec<u8> {
        let mut hasher = Sha256::new();
        match self {
            Payload::Text(t) => {
                hasher.update(b"text:");
                hasher.update(t.as_bytes());
            }
            Payload::Image {
                width,
                height,
                rgba,
            } => {
                hasher.update(b"image:");
                hasher.update(width.to_le_bytes());
                hasher.update(height.to_le_bytes());
                hasher.update(rgba);
            }
            Payload::Heartbeat => {
                hasher.update(b"heartbeat");
            }
        }
        hasher.finalize().to_vec()
    }

    /// Read a length-prefixed bincode frame from an async reader.
    pub async fn read_from<R: AsyncRead + Unpin>(reader: &mut R) -> anyhow::Result<Self> {
        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf).await?;
        let len = u32::from_be_bytes(len_buf) as usize;

        if len > MAX_CONTENT_BYTES {
            anyhow::bail!(
                "incoming content too large: {} bytes (max {})",
                len,
                MAX_CONTENT_BYTES
            );
        }

        let mut buf = vec![0u8; len];
        reader.read_exact(&mut buf).await?;
        Ok(bincode::deserialize(&buf)?)
    }

    /// Write a length-prefixed bincode frame to an async writer.
    pub async fn write_to<W: AsyncWrite + Unpin>(&self, writer: &mut W) -> anyhow::Result<()> {
        let encoded = bincode::serialize(self)?;
        let len = encoded.len();
        if len > MAX_CONTENT_BYTES {
            anyhow::bail!(
                "content too large to send: {} bytes (max {})",
                len,
                MAX_CONTENT_BYTES
            );
        }
        writer.write_all(&(len as u32).to_be_bytes()).await?;
        writer.write_all(&encoded).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::duplex;

    use super::*;

    #[tokio::test]
    async fn roundtrip_text() {
        let (mut a, mut b) = duplex(4096);
        let original = Payload::Text("hello world".to_string());
        original.write_to(&mut a).await.unwrap();
        let received = Payload::read_from(&mut b).await.unwrap();
        assert_eq!(original, received);
    }

    #[tokio::test]
    async fn roundtrip_image() {
        let (mut a, mut b) = duplex(65536);
        let original = Payload::Image {
            width: 2,
            height: 2,
            rgba: vec![
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255,
            ],
        };
        original.write_to(&mut a).await.unwrap();
        let received = Payload::read_from(&mut b).await.unwrap();
        assert_eq!(original, received);
    }

    #[tokio::test]
    async fn roundtrip_multiple_messages() {
        let (mut a, mut b) = duplex(65536);
        let msgs = vec![
            Payload::Text("first".to_string()),
            Payload::Heartbeat,
            Payload::Text("second".to_string()),
        ];
        for msg in &msgs {
            msg.write_to(&mut a).await.unwrap();
        }
        for expected in &msgs {
            let received = Payload::read_from(&mut b).await.unwrap();
            assert_eq!(expected, &received);
        }
    }

    #[test]
    fn fingerprint_differs_by_content() {
        let a = Payload::Text("foo".to_string());
        let b = Payload::Text("bar".to_string());
        assert_ne!(a.fingerprint(), b.fingerprint());
    }

    #[test]
    fn fingerprint_stable() {
        let msg = Payload::Text("stable".to_string());
        assert_eq!(msg.fingerprint(), msg.fingerprint());
    }
}
