use std::sync::OnceLock;

use tokio::sync::{Mutex, mpsc};

use super::read;
use crate::payload::Payload;

struct Listener {
    event_rx: Mutex<mpsc::UnboundedReceiver<()>>,
}

static LISTENER: OnceLock<Listener> = OnceLock::new();

fn listener() -> &'static Listener {
    LISTENER.get_or_init(|| {
        let (tx, rx) = mpsc::unbounded_channel();
        std::thread::spawn(move || win32_clipboard_listener(tx));
        Listener {
            event_rx: Mutex::new(rx),
        }
    })
}

/// Block until the clipboard changes, then return the new contents.
pub async fn clipboard_wait() -> anyhow::Result<Option<Payload>> {
    match listener().event_rx.lock().await.recv().await {
        Some(()) => {
            // Brief delay to allow the source application to fulfil delayed
            // rendering before we attempt to read the clipboard data.
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            read::read().await
        }
        None => Err(anyhow::anyhow!(
            "Win32 clipboard listener thread exited unexpectedly"
        )),
    }
}

fn win32_clipboard_listener(event_tx: mpsc::UnboundedSender<()>) {
    use windows::{
        Win32::{
            System::DataExchange::AddClipboardFormatListener,
            UI::WindowsAndMessaging::{
                CreateWindowExW, DestroyWindow, DispatchMessageW, GetMessageW, HWND_MESSAGE, MSG,
                RegisterClassW, TranslateMessage, WNDCLASSW, WS_EX_NOACTIVATE, WS_OVERLAPPED,
            },
        },
        core::PCWSTR,
    };

    unsafe {
        let class_name: Vec<u16> = "clipboard-share-listener\0".encode_utf16().collect();

        CLIPBOARD_TX.with(|cell| {
            *cell.borrow_mut() = Some(event_tx);
        });

        let wc = WNDCLASSW {
            lpfnWndProc: Some(wnd_proc),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };

        if RegisterClassW(&wc) == 0 {
            tracing::error!("RegisterClassW failed — clipboard listener cannot start");
            return;
        }

        let hwnd = CreateWindowExW(
            WS_EX_NOACTIVATE,
            PCWSTR(class_name.as_ptr()),
            PCWSTR::null(),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            None,
            None,
        );

        let Ok(hwnd) = hwnd else {
            tracing::error!("failed to create clipboard listener window");
            return;
        };

        if AddClipboardFormatListener(hwnd).is_err() {
            tracing::error!("AddClipboardFormatListener failed");
            DestroyWindow(hwnd).ok();
            return;
        }

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

std::thread_local! {
    static CLIPBOARD_TX: std::cell::RefCell<Option<mpsc::UnboundedSender<()>>> =
        std::cell::RefCell::new(None);
}

unsafe extern "system" fn wnd_proc(
    hwnd: windows::Win32::Foundation::HWND,
    msg: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::UI::WindowsAndMessaging::{
        DefWindowProcW, PostQuitMessage, WM_CLIPBOARDUPDATE, WM_DESTROY,
    };
    match msg {
        WM_CLIPBOARDUPDATE => {
            CLIPBOARD_TX.with(|cell| {
                if let Some(tx) = cell.borrow().as_ref() {
                    let _ = tx.send(());
                }
            });
            windows::Win32::Foundation::LRESULT(0)
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            windows::Win32::Foundation::LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}
