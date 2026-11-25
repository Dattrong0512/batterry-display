// Rust (windows-sys 0.59), Windows Registry HKCU Run autostart toggle.
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey,
    RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
};

const RUN_SUBKEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run\0";
const VALUE_NAME: &str = "BatteryDevice\0";

struct RegKeyGuard(HKEY);

impl Drop for RegKeyGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { RegCloseKey(self.0) };
        }
    }
}

fn expected_command() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    Some(format!("\"{}\"", exe.to_string_lossy()))
}

pub fn is_enabled() -> bool {
    let Some(expected) = expected_command() else {
        return false;
    };
    let subkey: Vec<u16> = RUN_SUBKEY.encode_utf16().collect();
    let value_name: Vec<u16> = VALUE_NAME.encode_utf16().collect();

    let mut hkey: HKEY = std::ptr::null_mut();
    let status =
        unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, subkey.as_ptr(), 0, KEY_READ, &mut hkey) };
    if status != 0 {
        return false;
    }
    let _guard = RegKeyGuard(hkey);

    let mut val_type: u32 = 0;
    let mut size: u32 = 0;
    let status = unsafe {
        RegQueryValueExW(
            hkey,
            value_name.as_ptr(),
            std::ptr::null_mut(),
            &mut val_type,
            std::ptr::null_mut(),
            &mut size,
        )
    };
    if status != 0 || val_type != REG_SZ || size < 2 {
        return false;
    }

    let mut buf = vec![0u16; (size as usize).div_ceil(2)];
    let status = unsafe {
        RegQueryValueExW(
            hkey,
            value_name.as_ptr(),
            std::ptr::null_mut(),
            &mut val_type,
            buf.as_mut_ptr().cast::<u8>(),
            &mut size,
        )
    };
    if status != 0 {
        return false;
    }

    let current = String::from_utf16_lossy(
        &buf.into_iter().take_while(|&w| w != 0).collect::<Vec<u16>>(),
    );
    current.trim().eq_ignore_ascii_case(&expected)
}

pub fn set_enabled(enable: bool) -> Result<(), u32> {
    let subkey: Vec<u16> = RUN_SUBKEY.encode_utf16().collect();
    let value_name: Vec<u16> = VALUE_NAME.encode_utf16().collect();

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
        return Err(status as u32);
    }
    let _guard = RegKeyGuard(hkey);

    if enable {
        let Some(cmd) = expected_command() else {
            return Err(1);
        };
        let wide: Vec<u16> = cmd.encode_utf16().chain(std::iter::once(0)).collect();
        let byte_len = (wide.len() * std::mem::size_of::<u16>()) as u32;
        let status = unsafe {
            RegSetValueExW(
                hkey,
                value_name.as_ptr(),
                0,
                REG_SZ,
                wide.as_ptr().cast::<u8>(),
                byte_len,
            )
        };
        if status != 0 {
            return Err(status as u32);
        }
    } else {
        let status = unsafe { RegDeleteValueW(hkey, value_name.as_ptr()) };
        // ERROR_FILE_NOT_FOUND (2) is fine when already disabled.
        if status != 0 && status != 2 {
            return Err(status as u32);
        }
    }
    Ok(())
}
