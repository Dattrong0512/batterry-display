// Rust (windows-sys 0.59), Windows 10/11 DPI-aware 32-bit ARGB GDI tray icon renderer.
use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    ANTIALIASED_QUALITY, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CLIP_DEFAULT_PRECIS, CreateBitmap,
    CreateCompatibleDC, CreateDIBSection, CreateFontW, DEFAULT_CHARSET, DEFAULT_PITCH,
    DIB_RGB_COLORS, DT_CENTER, DT_NOCLIP, DT_SINGLELINE, DT_VCENTER, DeleteDC, DeleteObject,
    DrawTextW, FF_SWISS, FW_BOLD, GdiFlush, GetDC, OUT_TT_PRECIS, ReleaseDC, SelectObject,
    SetBkMode, SetTextColor, TRANSPARENT,
};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_READ, REG_DWORD, RegCloseKey, RegOpenKeyExW, RegQueryValueExW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateIconIndirect, GetSystemMetrics, HICON, ICONINFO, SM_CXSMICON,
};

struct RegKeyGuard(HKEY);

impl Drop for RegKeyGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { RegCloseKey(self.0) };
        }
    }
}

pub fn is_light_taskbar() -> bool {
    let subkey: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize\0"
        .encode_utf16()
        .collect();
    let value_name: Vec<u16> = "SystemUsesLightTheme\0".encode_utf16().collect();

    let mut hkey: HKEY = std::ptr::null_mut();
    let status = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, subkey.as_ptr(), 0, KEY_READ, &mut hkey) };
    if status != 0 {
        return false;
    }
    let _guard = RegKeyGuard(hkey);

    let mut val_type: u32 = 0;
    let mut data: u32 = 0;
    let mut data_size: u32 = std::mem::size_of::<u32>() as u32;
    let status = unsafe {
        RegQueryValueExW(
            hkey,
            value_name.as_ptr(),
            std::ptr::null_mut(),
            &mut val_type,
            (&mut data as *mut u32).cast::<u8>(),
            &mut data_size,
        )
    };
    status == 0 && val_type == REG_DWORD && data != 0
}

pub fn create_battery_icon(battery: Option<u8>, light_taskbar: bool) -> HICON {
    let size = unsafe { GetSystemMetrics(SM_CXSMICON) }.max(16);
    let (text, fg_rgb, bar_rgb) = match battery {
        Some(pct) => {
            let fg = if pct <= 15 {
                (239u8, 68u8, 68u8)
            } else if pct <= 30 {
                (245u8, 158u8, 11u8)
            } else if light_taskbar {
                (26u8, 26u8, 26u8)
            } else {
                (255u8, 255u8, 255u8)
            };

            let bar = if pct <= 15 {
                (239u8, 68u8, 68u8)
            } else if pct <= 30 {
                (245u8, 158u8, 11u8)
            } else if light_taskbar {
                (22u8, 163u8, 74u8)
            } else {
                (34u8, 197u8, 94u8)
            };
            (format!("{pct}"), fg, Some((pct, bar)))
        }
        None => {
            let fg = if light_taskbar {
                (105u8, 105u8, 105u8)
            } else {
                (160u8, 160u8, 160u8)
            };
            ("--".to_string(), fg, None)
        }
    };

    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: size,
            biHeight: -size, // top-down DIB
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            biSizeImage: 0,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        },
        bmiColors: [unsafe { std::mem::zeroed() }; 1],
    };

    let screen_dc = unsafe { GetDC(std::ptr::null_mut() as HWND) };
    let mem_dc = unsafe { CreateCompatibleDC(screen_dc) };
    let mut bits_ptr: *mut std::ffi::c_void = std::ptr::null_mut();
    let color_bmp = unsafe {
        CreateDIBSection(
            screen_dc,
            &bmi,
            DIB_RGB_COLORS,
            &mut bits_ptr,
            std::ptr::null_mut(),
            0,
        )
    };
    unsafe { ReleaseDC(std::ptr::null_mut() as HWND, screen_dc) };

    if mem_dc.is_null() || color_bmp.is_null() || bits_ptr.is_null() {
        if !color_bmp.is_null() {
            unsafe { DeleteObject(color_bmp) };
        }
        if !mem_dc.is_null() {
            unsafe { DeleteDC(mem_dc) };
        }
        return std::ptr::null_mut();
    }

    let pixel_count = (size as usize) * (size as usize);
    let pixels = unsafe { std::slice::from_raw_parts_mut(bits_ptr.cast::<u8>(), pixel_count * 4) };
    pixels.fill(0);

    let old_bmp = unsafe { SelectObject(mem_dc, color_bmp) };

    let font_face: Vec<u16> = "Segoe UI\0".encode_utf16().collect();
    let (font_h, font_w) = if text.len() >= 3 {
        (
            -((size as f32 * 0.68).round() as i32),
            (size as f32 * 0.24).round() as i32,
        )
    } else {
        (-((size as f32 * 0.80).round() as i32), 0)
    };

    let hfont = unsafe {
        CreateFontW(
            font_h,
            font_w,
            0,
            0,
            FW_BOLD as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            OUT_TT_PRECIS as u32,
            CLIP_DEFAULT_PRECIS as u32,
            ANTIALIASED_QUALITY as u32,
            (DEFAULT_PITCH | FF_SWISS) as u32,
            font_face.as_ptr(),
        )
    };
    let old_font = if !hfont.is_null() {
        unsafe { SelectObject(mem_dc, hfont) }
    } else {
        std::ptr::null_mut()
    };

    let bar_h = if bar_rgb.is_some() { (size / 7).max(2) } else { 0 };
    let text_bottom = size - bar_h - if bar_h > 0 { 1 } else { 0 };
    let mut rc = RECT {
        left: 0,
        top: 0,
        right: size,
        bottom: text_bottom,
    };

    let text_w: Vec<u16> = text.encode_utf16().collect();
    unsafe {
        SetBkMode(mem_dc, TRANSPARENT as i32);
        SetTextColor(mem_dc, 0x00FF_FFFF);
        DrawTextW(
            mem_dc,
            text_w.as_ptr(),
            text_w.len() as i32,
            &mut rc,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOCLIP,
        );
        GdiFlush();
    }

    // GDI zeroes the alpha byte on 32-bit DIB text; convert luminance into premultiplied ARGB.
    let (fr, fg, fb) = fg_rgb;
    for px in pixels.chunks_exact_mut(4) {
        let luma = px[2];
        if luma > 0 {
            let alpha = ((luma as f32 / 255.0).powf(0.82) * 255.0)
                .round()
                .clamp(0.0, 255.0) as u8;
            let a = alpha as u16;
            px[0] = ((fb as u16 * a) / 255) as u8;
            px[1] = ((fg as u16 * a) / 255) as u8;
            px[2] = ((fr as u16 * a) / 255) as u8;
            px[3] = alpha;
        } else {
            px.fill(0);
        }
    }

    // Draw crisp battery progress bar at the bottom edge.
    if let Some((pct, (br, bg, bb))) = bar_rgb {
        let fill_w = if pct == 0 {
            0
        } else {
            ((size as u32 * pct as u32 + 50) / 100).clamp(1, size as u32) as i32
        };
        let track_val: u8 = if light_taskbar { 45 } else { 70 };
        let track_alpha: u8 = 88;
        let row_start = size - bar_h;
        for y in row_start..size {
            for x in 0..size {
                let idx = ((y as usize) * (size as usize) + (x as usize)) * 4;
                if x < fill_w {
                    pixels[idx] = bb;
                    pixels[idx + 1] = bg;
                    pixels[idx + 2] = br;
                    pixels[idx + 3] = 255;
                } else {
                    let premul = ((track_val as u16 * track_alpha as u16) / 255) as u8;
                    pixels[idx] = premul;
                    pixels[idx + 1] = premul;
                    pixels[idx + 2] = premul;
                    pixels[idx + 3] = track_alpha;
                }
            }
        }
    }

    if !old_font.is_null() {
        unsafe { SelectObject(mem_dc, old_font) };
    }
    if !hfont.is_null() {
        unsafe { DeleteObject(hfont) };
    }
    unsafe {
        SelectObject(mem_dc, old_bmp);
        DeleteDC(mem_dc);
    }

    let mask_bmp = unsafe { CreateBitmap(size, size, 1, 1, std::ptr::null()) };
    let icon_info = ICONINFO {
        fIcon: 1,
        xHotspot: 0,
        yHotspot: 0,
        hbmMask: mask_bmp,
        hbmColor: color_bmp,
    };
    let hicon = unsafe { CreateIconIndirect(&icon_info) };

    if !mask_bmp.is_null() {
        unsafe { DeleteObject(mask_bmp) };
    }
    unsafe { DeleteObject(color_bmp) };

    hicon
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo};

    #[test]
    fn creates_non_empty_hicon_for_all_states() {
        for (bat, light) in [
            (Some(100), false),
            (Some(25), true),
            (Some(10), false),
            (None, false),
            (None, true),
        ] {
            let hicon = create_battery_icon(bat, light);
            assert!(!hicon.is_null(), "HICON null for ({bat:?}, {light})");

            let mut info: ICONINFO = unsafe { std::mem::zeroed() };
            let ok = unsafe { GetIconInfo(hicon, &mut info) };
            assert_ne!(ok, 0);
            assert!(!info.hbmColor.is_null());
            assert!(!info.hbmMask.is_null());

            unsafe {
                DeleteObject(info.hbmColor);
                DeleteObject(info.hbmMask);
                DestroyIcon(hicon);
            }
        }
    }
}
