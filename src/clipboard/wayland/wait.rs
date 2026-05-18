use anyhow::Context;
use wl_clipboard_rs::paste::{ClipboardType, Error as PasteError, Seat, get_mime_types};

use super::read;
use crate::connection::payload::Payload;

/// Block until the clipboard changes, then return the new contents.
pub async fn wait() -> anyhow::Result<Option<Payload>> {
    let has_content = tokio::task::spawn_blocking(blocking_wait_for_event)
        .await
        .context("clipboard clipboard_wait task panicked")?;
    if has_content? {
        read::read().await
    } else {
        Ok(None)
    }
}

fn blocking_wait_for_event() -> anyhow::Result<bool> {
    let mime_types = match get_mime_types(ClipboardType::Regular, Seat::Unspecified) {
        Ok(types) => types,
        Err(PasteError::ClipboardEmpty) | Err(PasteError::NoMimeType) => return Ok(false),
        Err(e) => return Err(e.into()),
    };

    let has_text = mime_types
        .iter()
        .any(|m| m.starts_with("text/") || m == "TEXT" || m == "STRING" || m == "UTF8_STRING");
    let has_image = mime_types.iter().any(|m| m == "image/png");

    Ok(has_text || has_image)
}
