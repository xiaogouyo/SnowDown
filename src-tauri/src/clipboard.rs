#[cfg(windows)]
pub fn read_text() -> Result<Option<String>, String> {
    use windows_sys::Win32::{
        System::DataExchange::{
            CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
        },
        System::Memory::{GlobalLock, GlobalSize, GlobalUnlock},
    };
    unsafe {
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return Err("剪贴板正被其他程序使用，请稍后重试。".into());
        }
        struct ClipboardGuard;
        impl Drop for ClipboardGuard {
            fn drop(&mut self) {
                unsafe {
                    CloseClipboard();
                }
            }
        }
        let _guard = ClipboardGuard;
        if IsClipboardFormatAvailable(13) == 0 {
            return Ok(None);
        }
        let handle = GetClipboardData(13);
        if handle.is_null() {
            return Err("剪贴板文本暂未就绪，请稍后重试。".into());
        }
        let size = GlobalSize(handle).min(128 * 1024) / 2;
        if size == 0 {
            return Err("剪贴板文本暂不可读，请稍后重试。".into());
        }
        let ptr = GlobalLock(handle) as *const u16;
        if ptr.is_null() {
            return Err("剪贴板文本暂不可读，请稍后重试。".into());
        }
        let slice = std::slice::from_raw_parts(ptr, size);
        let length = slice.iter().position(|v| *v == 0).unwrap_or(size);
        let text = String::from_utf16_lossy(&slice[..length]);
        GlobalUnlock(handle);
        Ok(Some(text))
    }
}
#[cfg(not(windows))]
pub fn read_text() -> Result<Option<String>, String> {
    Ok(None)
}
#[cfg(windows)]
pub fn sequence() -> u32 {
    unsafe { windows_sys::Win32::System::DataExchange::GetClipboardSequenceNumber() }
}
#[cfg(not(windows))]
pub fn sequence() -> u32 {
    0
}
