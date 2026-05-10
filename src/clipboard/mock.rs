use crate::message::ClipboardMessage;
use async_trait::async_trait;
use std::sync::{Arc, Mutex};
use super::ClipboardProvider;

/// In-memory clipboard for use in tests. Thread-safe so multiple handles
/// can share the same backing store.
#[derive(Clone, Default)]
pub struct MockClipboard {
    contents: Arc<Mutex<Option<ClipboardMessage>>>,
}

impl MockClipboard {
    pub fn new() -> Self {
        Self::default()
    }

    /// Directly set contents without going through the trait (useful in tests).
    pub fn set(&self, msg: ClipboardMessage) {
        *self.contents.lock().unwrap() = Some(msg);
    }

    /// Directly read contents without going through the trait (useful in tests).
    pub fn get(&self) -> Option<ClipboardMessage> {
        self.contents.lock().unwrap().clone()
    }
}

#[async_trait]
impl ClipboardProvider for MockClipboard {
    async fn read(&mut self) -> anyhow::Result<Option<ClipboardMessage>> {
        Ok(self.contents.lock().unwrap().clone())
    }

    async fn write(&mut self, msg: &ClipboardMessage) -> anyhow::Result<()> {
        *self.contents.lock().unwrap() = Some(msg.clone());
        Ok(())
    }
}
