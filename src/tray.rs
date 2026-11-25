// Rust (windows-sys 0.59), Windows Shell_NotifyIconW & context menu management.
use crate::devices::DeviceBattery;
use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_NO_MORE_ITEMS, HWND, POINT};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_DWORD, REG_SZ, RegCloseKey, RegEnumKeyExW,
    RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
};
use windows_sys::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
    Shell_NotifyIconW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyIcon, DestroyMenu, GetCursorPos, HICON, HMENU,
    MF_CHECKED, MF_DISABLED, MF_GRAYED, MF_SEPARATOR, MF_STRING, MF_UNCHECKED, PostMessageW,
    SetForegroundWindow, TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_NONOTIFY, TPM_RETURNCMD,
    TPM_RIGHTBUTTON, TrackPopupMenu, WM_APP, WM_NULL,
};

pub const WM_TRAYICON: u32 = WM_APP + 1;
const TRAY_UID: u32 = 1;

const IDM_AUTO: u32 = 1000;
const IDM_REFRESH: u32 = 1001;
const IDM_AUTOSTART: u32 = 1002;
const IDM_EXIT: u32 = 1003;
const IDM_DEVICE_BASE: u32 = 2000;

pub enum MenuAction {
    None,
    SelectAuto,
    SelectDevice(String),
    Refresh,
    ToggleAutostart,
    Exit,
}

pub struct TrayIcon {
    hwnd: HWND,
    current_icon: HICON,
    added: bool,
}

impl TrayIcon {
    pub fn new(hwnd: HWND) -> Self {
        Self {
            hwnd,
            current_icon: std::ptr::null_mut(),
            added: false,
        }
    }

    pub fn update(&mut self, new_icon: HICON, tooltip: &str) {
        let mut nid = self.build_nid(new_icon, tooltip);
        let msg = if self.added { NIM_MODIFY } else { NIM_ADD };
        let ok = unsafe { Shell_NotifyIconW(msg, &mut nid) };
        if ok != 0 {
            self.added = true;
        }
        let old = std::mem::replace(&mut self.current_icon, new_icon);
        if !old.is_null() {
            unsafe { DestroyIcon(old) };
        }
    }

    pub fn readd(&mut self, tooltip: &str) {
        if self.current_icon.is_null() {
            return;
        }
        let mut nid = self.build_nid(self.current_icon, tooltip);
        let ok = unsafe { Shell_NotifyIconW(NIM_ADD, &mut nid) };
        if ok != 0 {
            self.added = true;
        }
    }

    pub fn remove(&mut self) {
        if self.added {
            let mut nid = self.build_nid(std::ptr::null_mut(), "");
            unsafe { Shell_NotifyIconW(NIM_DELETE, &mut nid) };
            self.added = false;
        }
    }

    fn build_nid(&self, icon: HICON, tooltip: &str) -> NOTIFYICONDATAW {
        let mut nid: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = self.hwnd;
        nid.uID = TRAY_UID;
        nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        nid.uCallbackMessage = WM_TRAYICON;
        nid.hIcon = icon;
        write_utf16_tip(tooltip, &mut nid.szTip);
        nid
    }
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        self.remove();
        if !self.current_icon.is_null() {
            unsafe { DestroyIcon(self.current_icon) };
            self.current_icon = std::ptr::null_mut();
        }
    }
}

fn write_utf16_tip(text: &str, dest: &mut [u16; 128]) {
    dest.fill(0);
    let mut i = 0usize;
    for unit in text.encode_utf16() {
        if i >= dest.len() - 1 {
            break;
        }
        dest[i] = unit;
        i += 1;
    }
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
    pinned_id: Option<&str>,
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
            let checked = pinned_id == Some(dev.container_id.as_str());
            let flag = MF_STRING | if checked { MF_CHECKED } else { MF_UNCHECKED };
            let label = format!("[{}] {} — {}%", dev.category.label(), dev.name, dev.battery);
            append_item(hmenu, flag, IDM_DEVICE_BASE as usize + i, &label);
        }
        append_item(hmenu, MF_SEPARATOR, 0, "");
        let auto_flag = MF_STRING
            | if pinned_id.is_none() {
                MF_CHECKED
            } else {
                MF_UNCHECKED
            };
        append_item(
            hmenu,
            auto_flag,
            IDM_AUTO as usize,
            "Tự động (ưu tiên pin thấp nhất)",
        );
    }

    append_item(hmenu, MF_SEPARATOR, 0, "");
    append_item(hmenu, MF_STRING, IDM_REFRESH as usize, "Cập nhật ngay");
    let auto_start_flag = MF_STRING
        | if autostart_enabled {
            MF_CHECKED
        } else {
            MF_UNCHECKED
        };
    append_item(
        hmenu,
        auto_start_flag,
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
    if cmd == IDM_AUTO {
        MenuAction::SelectAuto
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

struct RegKeyGuard(HKEY);

impl Drop for RegKeyGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { RegCloseKey(self.0) };
        }
    }
}

/// Promotes this executable's Windows 11 tray icon onto the visible taskbar if the
/// OS created its NotifyIconSettings entry without an explicit `IsPromoted` value.
pub fn promote_to_taskbar_if_unset() -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let exe_str = exe.to_string_lossy();

    let root_path: Vec<u16> = "Control Panel\\NotifyIconSettings\0".encode_utf16().collect();
    let mut root_key: HKEY = std::ptr::null_mut();
    let status = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            root_path.as_ptr(),
            0,
            KEY_READ,
            &mut root_key,
        )
    };
    if status != 0 {
        return false;
    }
    let _root_guard = RegKeyGuard(root_key);

    let exe_val_name: Vec<u16> = "ExecutablePath\0".encode_utf16().collect();
    let promoted_val_name: Vec<u16> = "IsPromoted\0".encode_utf16().collect();

    let mut index: u32 = 0;
    loop {
        let mut name_buf = [0u16; 256];
        let mut name_len = name_buf.len() as u32;
        let st = unsafe {
            RegEnumKeyExW(
                root_key,
                index,
                name_buf.as_mut_ptr(),
                &mut name_len,
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        if st == ERROR_NO_MORE_ITEMS {
            break;
        }
        index += 1;
        if st != 0 || name_len == 0 {
            continue;
        }

        let mut sub_name = name_buf[..name_len as usize].to_vec();
        sub_name.push(0);

        let mut sub_key: HKEY = std::ptr::null_mut();
        let st = unsafe {
            RegOpenKeyExW(
                root_key,
                sub_name.as_ptr(),
                0,
                KEY_READ | KEY_WRITE,
                &mut sub_key,
            )
        };
        if st != 0 {
            continue;
        }
        let _sub_guard = RegKeyGuard(sub_key);

        let Some(reg_exe) = read_reg_sz(sub_key, &exe_val_name) else {
            continue;
        };
        if !reg_exe.trim().eq_ignore_ascii_case(exe_str.as_ref()) {
            continue;
        }

        let mut val_type: u32 = 0;
        let mut val_size: u32 = 0;
        let q = unsafe {
            RegQueryValueExW(
                sub_key,
                promoted_val_name.as_ptr(),
                std::ptr::null_mut(),
                &mut val_type,
                std::ptr::null_mut(),
                &mut val_size,
            )
        };
        if q == ERROR_FILE_NOT_FOUND {
            let promoted: u32 = 1;
            let _ = unsafe {
                RegSetValueExW(
                    sub_key,
                    promoted_val_name.as_ptr(),
                    0,
                    REG_DWORD,
                    (&promoted as *const u32).cast::<u8>(),
                    std::mem::size_of::<u32>() as u32,
                )
            };
        }
        return true;
    }

    false
}

fn read_reg_sz(hkey: HKEY, value_name: &[u16]) -> Option<String> {
    let mut val_type: u32 = 0;
    let mut size: u32 = 0;
    let st = unsafe {
        RegQueryValueExW(
            hkey,
            value_name.as_ptr(),
            std::ptr::null_mut(),
            &mut val_type,
            std::ptr::null_mut(),
            &mut size,
        )
    };
    if st != 0 || val_type != REG_SZ || size < 2 {
        return None;
    }
    let mut buf = vec![0u16; (size as usize).div_ceil(2)];
    let st = unsafe {
        RegQueryValueExW(
            hkey,
            value_name.as_ptr(),
            std::ptr::null_mut(),
            &mut val_type,
            buf.as_mut_ptr().cast::<u8>(),
            &mut size,
        )
    };
    if st != 0 {
        return None;
    }
    let trimmed: Vec<u16> = buf.into_iter().take_while(|&w| w != 0).collect();
    Some(String::from_utf16_lossy(&trimmed))
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
