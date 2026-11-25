// Rust (windows-sys 0.59), Windows Registry persistence under HKCU\Software\BatteryDevice.
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_DWORD, REG_OPTION_NON_VOLATILE, REG_SZ,
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
};

const SETTINGS_SUBKEY: &str = "Software\\BatteryDevice\0";
const VAL_SHOW_BAR: &str = "ShowBatteryBar\0";
const VAL_SHOW_NAME: &str = "ShowDeviceName\0";
const VAL_SELECTED_DEVICE: &str = "SelectedDevice\0";

struct RegKeyGuard(HKEY);

impl Drop for RegKeyGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { RegCloseKey(self.0) };
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Settings {
    pub show_battery_bar: bool,
    pub show_device_name: bool,
    pub selected_device: Option<String>,
}

impl Settings {
    pub fn load() -> Self {
        let subkey: Vec<u16> = SETTINGS_SUBKEY.encode_utf16().collect();
        let mut hkey: HKEY = std::ptr::null_mut();
        let status =
            unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, subkey.as_ptr(), 0, KEY_READ, &mut hkey) };
        if status != 0 {
            return Self::default();
        }
        let _guard = RegKeyGuard(hkey);

        Self {
            show_battery_bar: read_dword(hkey, VAL_SHOW_BAR).unwrap_or(0) != 0,
            show_device_name: read_dword(hkey, VAL_SHOW_NAME).unwrap_or(0) != 0,
            selected_device: read_sz(hkey, VAL_SELECTED_DEVICE).filter(|s| !s.is_empty()),
        }
    }

    pub fn save(&self) {
        let subkey: Vec<u16> = SETTINGS_SUBKEY.encode_utf16().collect();
        let mut hkey: HKEY = std::ptr::null_mut();
        let mut disp: u32 = 0;
        let status = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                subkey.as_ptr(),
                0,
                std::ptr::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_WRITE,
                std::ptr::null(),
                &mut hkey,
                &mut disp,
            )
        };
        if status != 0 {
            return;
        }
        let _guard = RegKeyGuard(hkey);

        write_dword(hkey, VAL_SHOW_BAR, u32::from(self.show_battery_bar));
        write_dword(hkey, VAL_SHOW_NAME, u32::from(self.show_device_name));
        match &self.selected_device {
            Some(id) if !id.is_empty() => write_sz(hkey, VAL_SELECTED_DEVICE, id),
            _ => delete_val(hkey, VAL_SELECTED_DEVICE),
        }
    }
}

fn read_dword(hkey: HKEY, name: &str) -> Option<u32> {
    let wide_name: Vec<u16> = name.encode_utf16().collect();
    let mut val_type: u32 = 0;
    let mut data: u32 = 0;
    let mut size: u32 = std::mem::size_of::<u32>() as u32;
    let st = unsafe {
        RegQueryValueExW(
            hkey,
            wide_name.as_ptr(),
            std::ptr::null_mut(),
            &mut val_type,
            (&mut data as *mut u32).cast::<u8>(),
            &mut size,
        )
    };
    (st == 0 && val_type == REG_DWORD).then_some(data)
}

fn read_sz(hkey: HKEY, name: &str) -> Option<String> {
    let wide_name: Vec<u16> = name.encode_utf16().collect();
    let mut val_type: u32 = 0;
    let mut size: u32 = 0;
    let st = unsafe {
        RegQueryValueExW(
            hkey,
            wide_name.as_ptr(),
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
            wide_name.as_ptr(),
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

fn write_dword(hkey: HKEY, name: &str, value: u32) {
    let wide_name: Vec<u16> = name.encode_utf16().collect();
    let _ = unsafe {
        RegSetValueExW(
            hkey,
            wide_name.as_ptr(),
            0,
            REG_DWORD,
            (&value as *const u32).cast::<u8>(),
            std::mem::size_of::<u32>() as u32,
        )
    };
}

fn write_sz(hkey: HKEY, name: &str, value: &str) {
    let wide_name: Vec<u16> = name.encode_utf16().collect();
    let wide_val: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
    let bytes = (wide_val.len() * std::mem::size_of::<u16>()) as u32;
    let _ = unsafe {
        RegSetValueExW(
            hkey,
            wide_name.as_ptr(),
            0,
            REG_SZ,
            wide_val.as_ptr().cast::<u8>(),
            bytes,
        )
    };
}

fn delete_val(hkey: HKEY, name: &str) {
    let wide_name: Vec<u16> = name.encode_utf16().collect();
    let _ = unsafe { RegDeleteValueW(hkey, wide_name.as_ptr()) };
}
