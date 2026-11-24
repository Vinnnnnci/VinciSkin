//! A native dialog for a failure that happens before the window exists.

/// Show `message` in a native error dialog.
pub fn show_build_error(message: &str) {
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};

        let text = wide(message);
        let title = wide("VinciSkin did not start");

        // SAFETY: both strings are null-terminated and outlive the call.
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                text.as_ptr(),
                title.as_ptr(),
                MB_ICONERROR | MB_OK,
            );
        }
    }

    #[cfg(not(target_os = "windows"))]
    eprintln!("VinciSkin did not start: {message}");
}

/// A null-terminated UTF-16 string.
#[cfg(target_os = "windows")]
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}
