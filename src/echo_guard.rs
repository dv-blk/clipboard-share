use std::sync::{Arc, Mutex};

/// Tracks the fingerprint of the most recently peer-written clipboard content.
///
/// Shared between `ClipboardMonitor` (checks before publishing) and
/// `SyncEngine` receive task (records after writing). Prevents echo loops
/// where content received from a peer is immediately sent back.
#[derive(Clone, Default)]
pub struct EchoGuard {
    last_written_fp: Arc<Mutex<Option<Vec<u8>>>>,
}

impl EchoGuard {
    /// Record the fingerprint of content just written from the peer.
    pub fn record(&self, fp: Vec<u8>) {
        *self.last_written_fp.lock().unwrap() = Some(fp);
    }

    /// Returns `true` if `fp` matches the last peer-written fingerprint.
    /// The monitor should suppress publishing when this returns `true`.
    pub fn is_echo(&self, fp: &[u8]) -> bool {
        self.last_written_fp
            .lock()
            .unwrap()
            .as_deref()
            .map(|last| last == fp)
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_echo_when_empty() {
        let guard = EchoGuard::default();
        assert!(!guard.is_echo(b"anything"));
    }

    #[test]
    fn echo_detected_after_record() {
        let guard = EchoGuard::default();
        guard.record(b"hello".to_vec());
        assert!(guard.is_echo(b"hello"));
    }

    #[test]
    fn different_fp_not_echo() {
        let guard = EchoGuard::default();
        guard.record(b"hello".to_vec());
        assert!(!guard.is_echo(b"world"));
    }

    #[test]
    fn record_overwrites_previous() {
        let guard = EchoGuard::default();
        guard.record(b"first".to_vec());
        guard.record(b"second".to_vec());
        assert!(!guard.is_echo(b"first"));
        assert!(guard.is_echo(b"second"));
    }

    #[test]
    fn clone_shares_state() {
        let guard = EchoGuard::default();
        let clone = guard.clone();
        guard.record(b"shared".to_vec());
        assert!(clone.is_echo(b"shared"));
    }
}
