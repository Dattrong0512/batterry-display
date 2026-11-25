// Rust (windows-sys 0.59), Windows context menu & multi-device tooltip formatting.
use crate::devices::DeviceBattery;
use windows_sys::Win32::Foundation::{HWND, POINT};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, HMENU, MF_CHECKED, MF_DISABLED,
    MF_GRAYED, MF_SEPARATOR, MF_STRING, MF_UNCHECKED, PostMessageW, SetForegroundWindow,
    TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu,
    WM_NULL,
};

const IDM_REFRESH: u32 = 1001;
const IDM_AUTOSTART: u32 = 1002;
const IDM_EXIT: u32 = 1003;
const IDM_SHOW_BAR: u32 = 1004;
const IDM_SHOW_NAME: u32 = 1005;
const IDM_DEVICE_BASE: u32 = 2000;

pub enum MenuAction {
    None,
    SelectDevice(String),
    ToggleBatteryBar,
    ToggleDeviceName,
    Refresh,
    ToggleAutostart,
    Exit,
}

pub fn format_tooltip(devices: &[DeviceBattery], active_id: Option<&str>) -> String {
    if devices.is_empty() {
        return "Battery Device\nKhông có thiết bị kết nối".to_string();
    }
    if devices.len() == 1 {
        return format!("{}: {}%", devices[0].name, devices[0].battery);
    }

    let mut out = String::new();
    for (idx, dev) in devices.iter().enumerate() {
        let marker = if active_id == Some(dev.container_id.as_str()) {
            "• "
        } else {
            ""
        };
        let line = format!("{marker}{}: {}%", dev.name, dev.battery);
        let candidate_len = if idx == 0 {
            line.encode_utf16().count()
        } else {
            out.encode_utf16().count() + 1 + line.encode_utf16().count()
        };
        if candidate_len > 124 {
            let rem = devices.len() - idx;
            let suffix = format!("\n+{rem} thiết bị khác");
            if out.encode_utf16().count() + suffix.encode_utf16().count() <= 127 {
                out.push_str(&suffix);
            }
            break;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&line);
    }
    out
}

pub fn show_context_menu(
    hwnd: HWND,
    devices: &[DeviceBattery],
    active_id: Option<&str>,
    show_bar: bool,
    show_name: bool,
    autostart_enabled: bool,
) -> MenuAction {
    let hmenu: HMENU = unsafe { CreatePopupMenu() };
    if hmenu.is_null() {
        return MenuAction::None;
    }

    if devices.is_empty() {
        append_item(
            hmenu,
            MF_STRING | MF_DISABLED | MF_GRAYED,
            0,
            "Chưa có thiết bị kết nối",
        );
    } else {
        for (i, dev) in devices.iter().enumerate() {
            let checked = active_id == Some(dev.container_id.as_str());
            let flag = MF_STRING | if checked { MF_CHECKED } else { MF_UNCHECKED };
            let label = format!("[{}] {} — {}%", dev.category.label(), dev.name, dev.battery);
            append_item(hmenu, flag, IDM_DEVICE_BASE as usize + i, &label);
        }
    }

    append_item(hmenu, MF_SEPARATOR, 0, "");
    append_item(
        hmenu,
        MF_STRING | if show_bar { MF_CHECKED } else { MF_UNCHECKED },
        IDM_SHOW_BAR as usize,
        "Hiện thanh pin",
    );
    append_item(
        hmenu,
        MF_STRING | if show_name { MF_CHECKED } else { MF_UNCHECKED },
        IDM_SHOW_NAME as usize,
        "Hiện tên thiết bị",
    );

    append_item(hmenu, MF_SEPARATOR, 0, "");
    append_item(hmenu, MF_STRING, IDM_REFRESH as usize, "Cập nhật ngay");
    append_item(
        hmenu,
        MF_STRING
            | if autostart_enabled {
                MF_CHECKED
            } else {
                MF_UNCHECKED
            },
        IDM_AUTOSTART as usize,
        "Khởi động cùng Windows",
    );
    append_item(hmenu, MF_SEPARATOR, 0, "");
    append_item(hmenu, MF_STRING, IDM_EXIT as usize, "Thoát");

    let mut pt = POINT { x: 0, y: 0 };
    unsafe {
        GetCursorPos(&mut pt);
        SetForegroundWindow(hwnd);
    }
    let cmd = unsafe {
        TrackPopupMenu(
            hmenu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY | TPM_BOTTOMALIGN | TPM_LEFTALIGN,
            pt.x,
            pt.y,
            0,
            hwnd,
            std::ptr::null(),
        )
    };
    unsafe {
        PostMessageW(hwnd, WM_NULL, 0, 0);
        DestroyMenu(hmenu);
    }

    let cmd = cmd as u32;
    if cmd == IDM_SHOW_BAR {
        MenuAction::ToggleBatteryBar
    } else if cmd == IDM_SHOW_NAME {
        MenuAction::ToggleDeviceName
    } else if cmd == IDM_REFRESH {
        MenuAction::Refresh
    } else if cmd == IDM_AUTOSTART {
        MenuAction::ToggleAutostart
    } else if cmd == IDM_EXIT {
        MenuAction::Exit
    } else if cmd >= IDM_DEVICE_BASE {
        let idx = (cmd - IDM_DEVICE_BASE) as usize;
        match devices.get(idx) {
            Some(dev) => MenuAction::SelectDevice(dev.container_id.clone()),
            None => MenuAction::None,
        }
    } else {
        MenuAction::None
    }
}

fn append_item(hmenu: HMENU, flags: u32, id: usize, text: &str) {
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe { AppendMenuW(hmenu, flags, id, wide.as_ptr()) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devices::DeviceCategory;

    fn dev(id: &str, name: &str, battery: u8) -> DeviceBattery {
        DeviceBattery {
            container_id: id.to_string(),
            name: name.to_string(),
            category: DeviceCategory::Headset,
            battery,
        }
    }

    #[test]
    fn formats_empty_single_and_multi_tooltips_within_win32_limit() {
        assert_eq!(
            format_tooltip(&[], None),
            "Battery Device\nKhông có thiết bị kết nối"
        );
        assert_eq!(
            format_tooltip(&[dev("1", "SOUNDPEATS T3 Pro", 100)], Some("1")),
            "SOUNDPEATS T3 Pro: 100%"
        );

        let many: Vec<DeviceBattery> = (0..8)
            .map(|i| dev(&format!("{i}"), &format!("Wireless Device Long Name {i}"), 80))
            .collect();
        let tip = format_tooltip(&many, Some("0"));
        assert!(tip.encode_utf16().count() <= 127);
        assert!(tip.contains("thiết bị khác"));
    }
}
