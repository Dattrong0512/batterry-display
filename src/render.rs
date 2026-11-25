// Rust (windows-sys 0.59), Apple/iOS minimal taskbar pill renderer with 4x SDF supersampling.
use crate::devices::DeviceCategory;
use windows_sys::Win32::Foundation::{HWND, POINT, RECT, SIZE};
use windows_sys::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, ANTIALIASED_QUALITY, BI_RGB, BITMAPINFO, BITMAPINFOHEADER,
    BLENDFUNCTION, CLIP_DEFAULT_PRECIS, CreateCompatibleDC, CreateDIBSection, CreateFontW,
    DIB_RGB_COLORS, DT_LEFT, DT_NOCLIP, DT_SINGLELINE, DT_VCENTER, DeleteDC, DeleteObject,
    DrawTextW, FW_MEDIUM, GdiFlush, GetDC, GetTextExtentPoint32W, OUT_TT_PRECIS, ReleaseDC,
    SelectObject, SetBkMode, SetTextColor, TRANSPARENT,
};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_READ, REG_DWORD, RegCloseKey, RegOpenKeyExW, RegQueryValueExW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{ULW_ALPHA, UpdateLayeredWindow};

const SS: i32 = 4;

struct RegKeyGuard(HKEY);

impl Drop for RegKeyGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { RegCloseKey(self.0) };
        }
    }
}

pub struct RenderedPill {
    pub width: i32,
    pub height: i32,
    pub pixels: Vec<u8>,
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

pub fn format_pill_text(battery: Option<u8>, device_name: Option<&str>, show_name: bool) -> String {
    match battery {
        Some(pct) => {
            if show_name
                && let Some(name) = device_name
                && !name.trim().is_empty()
            {
                let trimmed = name.trim();
                let short = if trimmed.chars().count() > 22 {
                    let prefix: String = trimmed.chars().take(21).collect();
                    format!("{}…", prefix.trim_end())
                } else {
                    trimmed.to_string()
                };
                format!("{short} · {pct}%")
            } else {
                format!("{pct}%")
            }
        }
        None => "--".to_string(),
    }
}

pub fn render_pill(
    battery: Option<u8>,
    category: DeviceCategory,
    device_name: Option<&str>,
    show_name: bool,
    show_bar: bool,
    light_taskbar: bool,
    hovered: bool,
    dpi: u32,
) -> RenderedPill {
    let scale = (dpi.max(96) as f32) / 96.0;
    let h = (32.0 * scale).round() as i32;
    let pad_x = (8.0 * scale).round() as i32;
    let icon_sz = (15.0 * scale).round() as i32;
    let gap = (5.0 * scale).round() as i32;

    let text = format_pill_text(battery, device_name, show_name);
    let has_bar = show_bar && battery.is_some();

    let fg = foreground_rgb(battery, light_taskbar);
    let (bg_rgb, bg_alpha) = if hovered {
        if light_taskbar {
            ((0.0f32, 0.0f32, 0.0f32), 16.0 / 255.0)
        } else {
            ((1.0f32, 1.0f32, 1.0f32), 26.0 / 255.0)
        }
    } else {
        // Alpha = 1/255 keeps the entire pill surface hit-testable without visible tint.
        ((0.0f32, 0.0f32, 0.0f32), 1.0 / 255.0)
    };

    let font_h_hi = -((13.0 * scale * SS as f32).round() as i32);
    let text_w_hi = measure_text_hi(&text, font_h_hi);
    let text_w = (text_w_hi + SS - 1) / SS;
    let w = (pad_x + icon_sz + gap + text_w + pad_x).max(h);

    let hi_w = w * SS;
    let hi_h = h * SS;

    let y_shift_hi = if has_bar {
        (2.0 * scale * SS as f32).round() as i32
    } else {
        0
    };
    let text_x_hi = (pad_x + icon_sz + gap) * SS;
    let text_luma = rasterize_text_hi(&text, font_h_hi, hi_w, hi_h, text_x_hi, -2 - y_shift_hi);

    let icon_cx = (pad_x as f32 + icon_sz as f32 * 0.5) * SS as f32;
    let icon_cy = (h as f32 * 0.5) * SS as f32 - y_shift_hi as f32;
    let icon_s = icon_sz as f32 * SS as f32;

    let pill_cx = hi_w as f32 * 0.5;
    let pill_cy = hi_h as f32 * 0.5;
    let pill_hx = pill_cx - 1.0 * SS as f32 * 0.25;
    let pill_hy = pill_cy - 1.0 * SS as f32;
    let pill_r = 5.5 * scale * SS as f32;

    // Optional thin 2px iOS-style rounded-capsule battery bar at the bottom.
    let bar_cy = (h as f32 - 4.0 * scale) * SS as f32;
    let bar_hy = 1.0 * scale * SS as f32;
    let bar_x0 = (pad_x as f32) * SS as f32;
    let bar_x1 = ((w - pad_x) as f32) * SS as f32;
    let bar_full_w = (bar_x1 - bar_x0).max(2.0 * bar_hy);
    let bar_cx = (bar_x0 + bar_x1) * 0.5;
    let bar_hx = bar_full_w * 0.5;
    let pct = battery.unwrap_or(0);
    let bar_fill_w = if pct == 0 {
        0.0
    } else {
        (bar_full_w * (pct as f32 / 100.0))
            .max(2.0 * bar_hy)
            .min(bar_full_w)
    };
    let bar_fill_cx = bar_x0 + bar_fill_w * 0.5;
    let bar_fill_hx = bar_fill_w * 0.5;
    let bar_rgb = bar_accent_rgb(pct, light_taskbar);
    let (bar_fr, bar_fg, bar_fb) = (
        bar_rgb.0 as f32 / 255.0,
        bar_rgb.1 as f32 / 255.0,
        bar_rgb.2 as f32 / 255.0,
    );
    let (track_val, track_alpha) = if light_taskbar {
        (0.0f32, 28.0 / 255.0)
    } else {
        (1.0f32, 38.0 / 255.0)
    };

    let fg_r = fg.0 as f32 / 255.0;
    let fg_g = fg.1 as f32 / 255.0;
    let fg_b = fg.2 as f32 / 255.0;

    let mut out = vec![0u8; (w * h * 4) as usize];
    let inv_16 = 1.0f32 / 16.0;

    for y in 0..h {
        for x in 0..w {
            let mut sum_r = 0.0f32;
            let mut sum_g = 0.0f32;
            let mut sum_b = 0.0f32;
            let mut sum_a = 0.0f32;

            for sy in 0..SS {
                let hy = y * SS + sy;
                let fy = hy as f32 + 0.5;
                let row_off = (hy * hi_w) as usize;
                for sx in 0..SS {
                    let hx = x * SS + sx;
                    let fx = hx as f32 + 0.5;

                    let pill_cov = sdf_coverage(sd_round_box(
                        fx - pill_cx,
                        fy - pill_cy,
                        pill_hx,
                        pill_hy,
                        pill_r,
                    ));
                    let mut dst_a = bg_alpha * pill_cov;
                    let mut dst_r = bg_rgb.0 * dst_a;
                    let mut dst_g = bg_rgb.1 * dst_a;
                    let mut dst_b = bg_rgb.2 * dst_a;

                    if has_bar {
                        let track_cov = sdf_coverage(sd_round_box(
                            fx - bar_cx,
                            fy - bar_cy,
                            bar_hx,
                            bar_hy,
                            bar_hy,
                        ));
                        if track_cov > 0.0 {
                            let ta = track_alpha * track_cov;
                            let inv_ta = 1.0 - ta;
                            dst_r = track_val * ta + dst_r * inv_ta;
                            dst_g = track_val * ta + dst_g * inv_ta;
                            dst_b = track_val * ta + dst_b * inv_ta;
                            dst_a = ta + dst_a * inv_ta;
                        }
                        if bar_fill_w > 0.0 {
                            let fill_cov = sdf_coverage(sd_round_box(
                                fx - bar_fill_cx,
                                fy - bar_cy,
                                bar_fill_hx,
                                bar_hy,
                                bar_hy,
                            ));
                            if fill_cov > 0.0 {
                                let inv_fill = 1.0 - fill_cov;
                                dst_r = bar_fr * fill_cov + dst_r * inv_fill;
                                dst_g = bar_fg * fill_cov + dst_g * inv_fill;
                                dst_b = bar_fb * fill_cov + dst_b * inv_fill;
                                dst_a = fill_cov + dst_a * inv_fill;
                            }
                        }
                    }

                    let icon_d = sdf_device_icon(category, fx - icon_cx, fy - icon_cy, icon_s);
                    let icon_cov = sdf_coverage(icon_d);
                    let txt_cov = text_luma[row_off + hx as usize] as f32 / 255.0;
                    let fa = icon_cov.max(txt_cov).clamp(0.0, 1.0);

                    let inv_fa = 1.0 - fa;
                    sum_r += fg_r * fa + dst_r * inv_fa;
                    sum_g += fg_g * fa + dst_g * inv_fa;
                    sum_b += fg_b * fa + dst_b * inv_fa;
                    sum_a += fa + dst_a * inv_fa;
                }
            }

            let a = (sum_a * inv_16 * 255.0).round().clamp(0.0, 255.0) as u8;
            let r = (sum_r * inv_16 * 255.0).round().clamp(0.0, a as f32) as u8;
            let g = (sum_g * inv_16 * 255.0).round().clamp(0.0, a as f32) as u8;
            let b = (sum_b * inv_16 * 255.0).round().clamp(0.0, a as f32) as u8;

            let idx = ((y * w + x) * 4) as usize;
            out[idx] = b;
            out[idx + 1] = g;
            out[idx + 2] = r;
            out[idx + 3] = a;
        }
    }

    RenderedPill {
        width: w,
        height: h,
        pixels: out,
    }
}

pub fn apply_to_layered_window(hwnd: HWND, x: i32, y: i32, pill: &RenderedPill) -> bool {
    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: pill.width,
            biHeight: -pill.height,
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
    let dib = unsafe {
        CreateDIBSection(
            screen_dc,
            &bmi,
            DIB_RGB_COLORS,
            &mut bits_ptr,
            std::ptr::null_mut(),
            0,
        )
    };

    if mem_dc.is_null() || dib.is_null() || bits_ptr.is_null() {
        if !dib.is_null() {
            unsafe { DeleteObject(dib) };
        }
        if !mem_dc.is_null() {
            unsafe { DeleteDC(mem_dc) };
        }
        unsafe { ReleaseDC(std::ptr::null_mut() as HWND, screen_dc) };
        return false;
    }

    unsafe {
        std::ptr::copy_nonoverlapping(
            pill.pixels.as_ptr(),
            bits_ptr.cast::<u8>(),
            pill.pixels.len(),
        );
    }

    let old_bmp = unsafe { SelectObject(mem_dc, dib) };
    let dst_pt = POINT { x, y };
    let dst_size = SIZE {
        cx: pill.width,
        cy: pill.height,
    };
    let src_pt = POINT { x: 0, y: 0 };
    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: 255,
        AlphaFormat: AC_SRC_ALPHA as u8,
    };

    let ok = unsafe {
        UpdateLayeredWindow(
            hwnd,
            screen_dc,
            &dst_pt,
            &dst_size,
            mem_dc,
            &src_pt,
            0,
            &blend,
            ULW_ALPHA,
        )
    };

    unsafe {
        SelectObject(mem_dc, old_bmp);
        DeleteObject(dib);
        DeleteDC(mem_dc);
        ReleaseDC(std::ptr::null_mut() as HWND, screen_dc);
    }

    ok != 0
}

fn foreground_rgb(battery: Option<u8>, light_taskbar: bool) -> (u8, u8, u8) {
    match battery {
        Some(pct) if pct <= 10 => {
            if light_taskbar {
                (255, 59, 48)
            } else {
                (255, 69, 58)
            }
        }
        Some(pct) if pct <= 20 => {
            if light_taskbar {
                (202, 102, 0)
            } else {
                (255, 159, 10)
            }
        }
        Some(_) => {
            if light_taskbar {
                (29, 29, 31)
            } else {
                (245, 245, 247)
            }
        }
        None => {
            if light_taskbar {
                (128, 128, 134)
            } else {
                (142, 142, 147)
            }
        }
    }
}

fn bar_accent_rgb(pct: u8, light_taskbar: bool) -> (u8, u8, u8) {
    if pct <= 10 {
        if light_taskbar {
            (255, 59, 48)
        } else {
            (255, 69, 58)
        }
    } else if pct <= 20 {
        if light_taskbar {
            (217, 119, 6)
        } else {
            (255, 159, 10)
        }
    } else if light_taskbar {
        (36, 138, 61)
    } else {
        (52, 199, 89)
    }
}

fn create_minimal_font(font_h: i32) -> *mut std::ffi::c_void {
    let face: Vec<u16> = "Segoe UI Variable Text\0".encode_utf16().collect();
    unsafe {
        CreateFontW(
            font_h,
            0,
            0,
            0,
            FW_MEDIUM as i32,
            0,
            0,
            0,
            0,
            OUT_TT_PRECIS as u32,
            CLIP_DEFAULT_PRECIS as u32,
            ANTIALIASED_QUALITY as u32,
            0,
            face.as_ptr(),
        )
    }
}

fn measure_text_hi(text: &str, font_h_hi: i32) -> i32 {
    let screen_dc = unsafe { GetDC(std::ptr::null_mut() as HWND) };
    let mem_dc = unsafe { CreateCompatibleDC(screen_dc) };
    let hfont = create_minimal_font(font_h_hi);
    let old_font = if !hfont.is_null() {
        unsafe { SelectObject(mem_dc, hfont) }
    } else {
        std::ptr::null_mut()
    };

    let wide: Vec<u16> = text.encode_utf16().collect();
    let mut sz = SIZE { cx: 0, cy: 0 };
    unsafe { GetTextExtentPoint32W(mem_dc, wide.as_ptr(), wide.len() as i32, &mut sz) };

    if !old_font.is_null() {
        unsafe { SelectObject(mem_dc, old_font) };
    }
    if !hfont.is_null() {
        unsafe { DeleteObject(hfont) };
    }
    unsafe {
        DeleteDC(mem_dc);
        ReleaseDC(std::ptr::null_mut() as HWND, screen_dc);
    }
    sz.cx.max(16)
}

fn rasterize_text_hi(
    text: &str,
    font_h_hi: i32,
    hi_w: i32,
    hi_h: i32,
    text_x_hi: i32,
    y_offset_hi: i32,
) -> Vec<u8> {
    let count = (hi_w * hi_h) as usize;
    let mut luma = vec![0u8; count];

    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: hi_w,
            biHeight: -hi_h,
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
    let dib = unsafe {
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

    if mem_dc.is_null() || dib.is_null() || bits_ptr.is_null() {
        if !dib.is_null() {
            unsafe { DeleteObject(dib) };
        }
        if !mem_dc.is_null() {
            unsafe { DeleteDC(mem_dc) };
        }
        return luma;
    }

    let pixels = unsafe { std::slice::from_raw_parts_mut(bits_ptr.cast::<u8>(), count * 4) };
    pixels.fill(0);

    let old_bmp = unsafe { SelectObject(mem_dc, dib) };
    let hfont = create_minimal_font(font_h_hi);
    let old_font = if !hfont.is_null() {
        unsafe { SelectObject(mem_dc, hfont) }
    } else {
        std::ptr::null_mut()
    };

    let mut rc = RECT {
        left: text_x_hi,
        top: y_offset_hi,
        right: hi_w,
        bottom: hi_h + y_offset_hi,
    };
    let wide: Vec<u16> = text.encode_utf16().collect();
    unsafe {
        SetBkMode(mem_dc, TRANSPARENT as i32);
        SetTextColor(mem_dc, 0x00FF_FFFF);
        DrawTextW(
            mem_dc,
            wide.as_ptr(),
            wide.len() as i32,
            &mut rc,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOCLIP,
        );
        GdiFlush();
    }

    for (i, px) in pixels.chunks_exact(4).enumerate() {
        luma[i] = px[2];
    }

    if !old_font.is_null() {
        unsafe { SelectObject(mem_dc, old_font) };
    }
    if !hfont.is_null() {
        unsafe { DeleteObject(hfont) };
    }
    unsafe {
        SelectObject(mem_dc, old_bmp);
        DeleteObject(dib);
        DeleteDC(mem_dc);
    }
    luma
}

#[inline]
fn sdf_coverage(d: f32) -> f32 {
    (0.5 - d).clamp(0.0, 1.0)
}

#[inline]
fn sd_round_box(px: f32, py: f32, bx: f32, by: f32, r: f32) -> f32 {
    let qx = px.abs() - (bx - r);
    let qy = py.abs() - (by - r);
    qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - r
}

#[inline]
fn sd_segment(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let pax = px - ax;
    let pay = py - ay;
    let bax = bx - ax;
    let bay = by - ay;
    let denom = bax * bax + bay * bay;
    let h = if denom > 0.0 {
        ((pax * bax + pay * bay) / denom).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (pax - bax * h).hypot(pay - bay * h)
}

#[inline]
fn sd_circle(px: f32, py: f32, cx: f32, cy: f32, r: f32) -> f32 {
    (px - cx).hypot(py - cy) - r
}

fn sdf_device_icon(category: DeviceCategory, px: f32, py: f32, s: f32) -> f32 {
    match category {
        DeviceCategory::Headset => {
            let r = 0.34 * s;
            let ay = 0.03 * s;
            let stroke = 0.052 * s;
            let d_arc = if py <= ay {
                (px.hypot(py - ay) - r).abs() - stroke
            } else {
                let d_l = sd_segment(px, py, -r, ay, -r, ay + 0.14 * s) - stroke;
                let d_r = sd_segment(px, py, r, ay, r, ay + 0.14 * s) - stroke;
                d_l.min(d_r)
            };
            let cup_x = 0.31 * s;
            let cup_y = 0.17 * s;
            let d_cup_l = sd_round_box(px + cup_x, py - cup_y, 0.095 * s, 0.165 * s, 0.065 * s);
            let d_cup_r = sd_round_box(px - cup_x, py - cup_y, 0.095 * s, 0.165 * s, 0.065 * s);
            d_arc.min(d_cup_l).min(d_cup_r)
        }
        DeviceCategory::Mouse => {
            let stroke = 0.050 * s;
            let d_body = sd_round_box(px, py, 0.24 * s, 0.36 * s, 0.22 * s).abs() - stroke;
            let d_wheel = sd_segment(px, py, 0.0, -0.21 * s, 0.0, -0.07 * s) - 0.040 * s;
            d_body.min(d_wheel)
        }
        DeviceCategory::Keyboard => {
            let stroke = 0.048 * s;
            let d_body = sd_round_box(px, py, 0.41 * s, 0.25 * s, 0.075 * s).abs() - stroke;
            let d_k1 = sd_circle(px, py, -0.21 * s, -0.07 * s, 0.036 * s);
            let d_k2 = sd_circle(px, py, -0.07 * s, -0.07 * s, 0.036 * s);
            let d_k3 = sd_circle(px, py, 0.07 * s, -0.07 * s, 0.036 * s);
            let d_k4 = sd_circle(px, py, 0.21 * s, -0.07 * s, 0.036 * s);
            let d_space = sd_segment(px, py, -0.15 * s, 0.09 * s, 0.15 * s, 0.09 * s) - 0.034 * s;
            d_body
                .min(d_k1)
                .min(d_k2)
                .min(d_k3)
                .min(d_k4)
                .min(d_space)
        }
        DeviceCategory::Gamepad => {
            let stroke = 0.048 * s;
            let d_body = sd_round_box(px, py, 0.40 * s, 0.24 * s, 0.18 * s).abs() - stroke;
            let d_dpad = sd_circle(px, py, -0.18 * s, 0.0, 0.045 * s);
            let d_b1 = sd_circle(px, py, 0.14 * s, 0.04 * s, 0.035 * s);
            let d_b2 = sd_circle(px, py, 0.23 * s, -0.04 * s, 0.035 * s);
            d_body.min(d_dpad).min(d_b1).min(d_b2)
        }
        DeviceCategory::Bluetooth => {
            let stroke = 0.046 * s;
            let d_stem = sd_segment(px, py, 0.0, -0.36 * s, 0.0, 0.36 * s) - stroke;
            let d_1 = sd_segment(px, py, 0.0, -0.36 * s, 0.22 * s, -0.16 * s) - stroke;
            let d_2 = sd_segment(px, py, 0.22 * s, -0.16 * s, -0.20 * s, 0.16 * s) - stroke;
            let d_3 = sd_segment(px, py, 0.0, 0.36 * s, 0.22 * s, 0.16 * s) - stroke;
            let d_4 = sd_segment(px, py, 0.22 * s, 0.16 * s, -0.20 * s, -0.16 * s) - stroke;
            d_stem.min(d_1).min(d_2).min(d_3).min(d_4)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_valid_pill(pill: &RenderedPill) {
        assert!(pill.width > 20 && pill.height >= 32);
        assert_eq!(pill.pixels.len(), (pill.width * pill.height * 4) as usize);

        let mut visible_pixels = 0usize;
        let mut icon_visible = 0usize;
        let mut text_visible = 0usize;

        for y in 0..pill.height {
            for x in 0..pill.width {
                let idx = ((y * pill.width + x) * 4) as usize;
                let (b, g, r, a) = (
                    pill.pixels[idx],
                    pill.pixels[idx + 1],
                    pill.pixels[idx + 2],
                    pill.pixels[idx + 3],
                );
                assert!(b <= a && g <= a && r <= a);
                if a > 30 {
                    visible_pixels += 1;
                    if x < 24 {
                        icon_visible += 1;
                    } else {
                        text_visible += 1;
                    }
                }
            }
        }

        assert!(visible_pixels > 10);
        assert!(icon_visible > 5, "SDF icon region had no visible pixels");
        assert!(text_visible > 5, "Typography region had no visible pixels");
    }

    #[test]
    fn formats_pill_text_with_optional_name_and_truncation() {
        assert_eq!(format_pill_text(None, Some("Test"), true), "--");
        assert_eq!(format_pill_text(Some(90), Some("Test"), false), "90%");
        assert_eq!(
            format_pill_text(Some(90), Some("SOUNDPEATS T3 Pro"), true),
            "SOUNDPEATS T3 Pro · 90%"
        );
        let long = format_pill_text(Some(85), Some("Very Long Bluetooth Headset Name"), true);
        assert!(long.contains("… · 85%"));
    }

    #[test]
    fn renders_premultiplied_pills_with_optional_bar_and_name() {
        let compact = render_pill(
            Some(90),
            DeviceCategory::Headset,
            Some("SOUNDPEATS T3 Pro"),
            false,
            false,
            true,
            false,
            96,
        );
        assert_valid_pill(&compact);

        let named_with_bar = render_pill(
            Some(90),
            DeviceCategory::Headset,
            Some("SOUNDPEATS T3 Pro"),
            true,
            true,
            true,
            false,
            96,
        );
        assert_valid_pill(&named_with_bar);
        assert!(named_with_bar.width > compact.width);

        // Verify bottom battery bar pixels exist along y = height - 4.
        let bar_y = named_with_bar.height - 4;
        let bar_pixels = (8..named_with_bar.width - 8)
            .filter(|&x| {
                let idx = ((bar_y * named_with_bar.width + x) * 4) as usize;
                named_with_bar.pixels[idx + 3] > 200
            })
            .count();
        assert!(bar_pixels > 10, "Expected visible battery bar pixels");

        for (bat, cat, light, hovered, dpi) in [
            (Some(100), DeviceCategory::Mouse, false, true, 144),
            (Some(15), DeviceCategory::Keyboard, true, true, 96),
            (Some(8), DeviceCategory::Gamepad, false, false, 96),
            (None, DeviceCategory::Bluetooth, true, false, 96),
        ] {
            let pill = render_pill(bat, cat, Some("Dev"), false, true, light, hovered, dpi);
            assert_valid_pill(&pill);
        }
    }
}
