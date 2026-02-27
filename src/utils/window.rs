// src/utils/window.rs

#[cfg(target_os = "windows")]
use windows_sys::Win32::Foundation::{HWND, RECT};
#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::WindowsAndMessaging::{
    SetWindowPos, SystemParametersInfoW, SPI_GETWORKAREA, SWP_NOACTIVATE, SWP_NOZORDER,
};

#[cfg(target_os = "windows")]
pub fn get_work_area_rect() -> (i32, i32, i32, i32) {
    unsafe {
        let mut rect = std::mem::zeroed::<RECT>();
        if SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut rect as *mut _ as *mut _, 0) != 0 {
            let width = rect.right - rect.left;
            let height = rect.bottom - rect.top;
            return (width, height, rect.left, rect.top);
        }
    }
    (1920, 1080, 0, 0)
}

#[cfg(not(target_os = "windows"))]
pub fn get_work_area_rect() -> (i32, i32, i32, i32) {
    // Mac/Linux 兜底
    (1920, 1080, 0, 0)
}

pub fn atomic_update_window(
    window: &dioxus::desktop::DesktopContext,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    always_on_top: bool,
) {
    #[cfg(target_os = "windows")]
    {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let hwnd = if let Ok(handle) = window.window_handle() {
            if let RawWindowHandle::Win32(win32_handle) = handle.as_raw() {
                Some(win32_handle.hwnd.get() as HWND)
            } else {
                None
            }
        } else {
            None
        };

        if let Some(hwnd) = hwnd {
            unsafe {
                SetWindowPos(
                    hwnd,
                    std::ptr::null_mut(),
                    x,
                    y,
                    w,
                    h,
                    SWP_NOACTIVATE | SWP_NOZORDER,
                );
            }
        } else {
            fallback_update(window, x, y, w, h);
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        fallback_update(window, x, y, w, h);
    }

    window.set_always_on_top(always_on_top);
}

fn fallback_update(window: &dioxus::desktop::DesktopContext, x: i32, y: i32, w: i32, h: i32) {
    use dioxus::desktop::wry::dpi::{PhysicalPosition, PhysicalSize};
    window.set_outer_position(PhysicalPosition::new(x, y));
    window.set_inner_size(PhysicalSize::new(w as u32, h as u32));
}
