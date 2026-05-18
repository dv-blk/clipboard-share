use anyhow::Context;
use wl_clipboard_rs::paste::{ClipboardType, Error as PasteError, Seat, get_mime_types};

use super::read;
use crate::payload::Payload;

pub async fn changed() -> anyhow::Result<Option<Payload>> {
    let has_content = tokio::task::spawn_blocking(|| {
        let mime_types = match get_mime_types(ClipboardType::Regular, Seat::Unspecified) {
            Ok(types) => types,
            Err(PasteError::ClipboardEmpty) | Err(PasteError::NoMimeType) => return Ok(false),
            Err(e) => return Err(anyhow::Error::from(e)),
        };

        let has_text = mime_types
            .iter()
            .any(|m| m.starts_with("text/") || m == "TEXT" || m == "STRING" || m == "UTF8_STRING");
        let has_image = mime_types.iter().any(|m| m == "image/png");

        Ok(has_text || has_image)
    })
    .await
    .context("clipboard wait task panicked")?;

    if has_content? {
        read::read().await
    } else {
        Ok(None)
    }
}
