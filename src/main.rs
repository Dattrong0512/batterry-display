// Rust (windows-sys 0.59), Windows 10/11 headless tray app entry & message loop.
#![windows_subsystem = "windows"]

mod autostart;
mod devices;
mod icon;
mod tray;

use devices::{DeviceBattery, scan_devices, select_active_device};
use tray::{MenuAction, TrayIcon, WM_TRAYICON, format_tooltip, show_context_menu};
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, HWND, LPARAM, LRESULT, WPARAM,
};
use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DBT_DEVNODES_CHANGED, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GWLP_USERDATA, GetMessageW, GetWindowLongPtrW, IDC_ARROW, KillTimer, LoadCursorW, MSG,
    PostMessageW, PostQuitMessage, RegisterClassExW, RegisterWindowMessageW, SetTimer,
    SetWindowLongPtrW, TranslateMessage, WM_CLOSE, WM_CONTEXTMENU, WM_DESTROY, WM_DEVICECHANGE, WM_LBUTTONUP,
    WM_RBUTTONUP, WM_SETTINGCHANGE, WM_TIMER, WNDCLASSEXW, WS_OVERLAPPEDWINDOW,
};

const TIMER_POLL_ID: usize = 1;
const TIMER_DEBOUNCE_ID: usize = 2;
const TIMER_PROMOTE_ID: usize = 3;
const POLL_INTERVAL_MS: u32 = 15_000;
const DEBOUNCE_DELAY_MS: u32 = 1_000;
const PROMOTE_RETRY_MS: u32 = 600;

struct MutexGuard(HANDLE);

impl Drop for MutexGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CloseHandle(self.0) };
        }
    }
}

#[derive(PartialEq, Eq)]
struct RenderKey {
    battery: Option<u8>,
    tooltip: String,
    light_taskbar: bool,
}

struct AppState {
    tray: TrayIcon,
    taskbar_created_msg: u32,
    devices: Vec<DeviceBattery>,
    pinned_id: Option<String>,
    last_render: Option<RenderKey>,
}

impl AppState {
    fn new(hwnd: HWND, taskbar_created_msg: u32) -> Self {
        Self {
            tray: TrayIcon::new(hwnd),
            taskbar_created_msg,
            devices: Vec::new(),
            pinned_id: None,
            last_render: None,
        }
    }

    fn refresh(&mut self, force_redraw: bool) {
        self.devices = scan_devices();
        if let Some(pinned) = &self.pinned_id
            && !self.devices.iter().any(|d| d.container_id == *pinned)
        {
            self.pinned_id = None;
        }
        self.render(force_redraw);
    }

    fn render(&mut self, force_redraw: bool) {
        let active = select_active_device(&self.devices, self.pinned_id.as_deref());
        let active_id = active.map(|d| d.container_id.as_str());
        let active_battery = active.map(|d| d.battery);
        let tooltip = format_tooltip(&self.devices, active_id);
        let light = icon::is_light_taskbar();

        let key = RenderKey {
            battery: active_battery,
            tooltip: tooltip.clone(),
            light_taskbar: light,
        };
        if !force_redraw && self.last_render.as_ref() == Some(&key) {
            return;
        }

        let hicon = icon::create_battery_icon(active_battery, light);
        self.tray.update(hicon, &tooltip);
        self.last_render = Some(key);
    }

    fn cycle_device(&mut self) {
        if self.devices.len() <= 1 {
            self.refresh(true);
            return;
        }
        let current_id = select_active_device(&self.devices, self.pinned_id.as_deref())
            .map(|d| d.container_id.as_str());
        let next_idx = current_id
            .and_then(|id| self.devices.iter().position(|d| d.container_id == id))
            .map(|i| (i + 1) % self.devices.len())
            .unwrap_or(0);
        self.pinned_id = Some(self.devices[next_idx].container_id.clone());
        self.render(true);
    }
}

fn main() {
    if std::env::args().any(|a| a == "--probe") {
        unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
        let devs = scan_devices();
        if devs.is_empty() {
            println!("No connected battery devices found.");
        } else {
            for d in devs {
                println!(
                    "[{}] {}: {}% (id={})",
                    d.category.label(),
                    d.name,
                    d.battery,
                    d.container_id
                );
            }
        }
        return;
    }

    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };

    let mutex_name: Vec<u16> = "Local\\BatteryDevice_SingleInstance_v1\0"
        .encode_utf16()
        .collect();
    let mutex = unsafe { CreateMutexW(std::ptr::null(), 1, mutex_name.as_ptr()) };
    if mutex.is_null() {
        return;
    }
    let _mutex_guard = MutexGuard(mutex);
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        return;
    }

    let hinstance = unsafe { GetModuleHandleW(std::ptr::null()) };
    let class_name: Vec<u16> = "BatteryDeviceTrayWindow\0".encode_utf16().collect();
    let taskbar_msg_name: Vec<u16> = "TaskbarCreated\0".encode_utf16().collect();
    let taskbar_created_msg = unsafe { RegisterWindowMessageW(taskbar_msg_name.as_ptr()) };

    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: 0,
        lpfnWndProc: Some(wnd_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: hinstance,
        hIcon: std::ptr::null_mut(),
        hCursor: unsafe { LoadCursorW(std::ptr::null_mut(), IDC_ARROW) },
        hbrBackground: std::ptr::null_mut(),
        lpszMenuName: std::ptr::null(),
        lpszClassName: class_name.as_ptr(),
        hIconSm: std::ptr::null_mut(),
    };
    if unsafe { RegisterClassExW(&wc) } == 0 {
        return;
    }

    // Must be a hidden top-level window (not HWND_MESSAGE) to receive WM_SETTINGCHANGE,
    // WM_DEVICECHANGE, and TaskbarCreated broadcasts.
    let hwnd = unsafe {
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            class_name.as_ptr(),
            WS_OVERLAPPEDWINDOW,
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            hinstance,
            std::ptr::null(),
        )
    };
    if hwnd.is_null() {
        return;
    }

    let state_ptr = Box::into_raw(Box::new(AppState::new(hwnd, taskbar_created_msg)));
    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, state_ptr as isize);
        (*state_ptr).refresh(true);
        if !tray::promote_to_taskbar_if_unset() {
            SetTimer(hwnd, TIMER_PROMOTE_ID, PROMOTE_RETRY_MS, None);
        }
        SetTimer(hwnd, TIMER_POLL_ID, POLL_INTERVAL_MS, None);
    }

    let mut msg: MSG = unsafe { std::mem::zeroed() };
    while unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) } > 0 {
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    // Reclaim Box<AppState> only after message loop terminates.
    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        drop(Box::from_raw(state_ptr));
    }
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let state_ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut AppState };
    if state_ptr.is_null() {
        return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
    }
    let state = unsafe { &mut *state_ptr };

    if state.taskbar_created_msg != 0 && msg == state.taskbar_created_msg {
        let active_id = select_active_device(&state.devices, state.pinned_id.as_deref())
            .map(|d| d.container_id.as_str());
        let tip = format_tooltip(&state.devices, active_id);
        state.tray.readd(&tip);
        return 0;
    }

    match msg {
        WM_TRAYICON => {
            let ev = (lparam & 0xFFFF) as u32;
            if ev == WM_RBUTTONUP || ev == WM_CONTEXTMENU {
                let action = show_context_menu(
                    hwnd,
                    &state.devices,
                    state.pinned_id.as_deref(),
                    autostart::is_enabled(),
                );
                match action {
                    MenuAction::SelectAuto => {
                        state.pinned_id = None;
                        state.render(true);
                    }
                    MenuAction::SelectDevice(id) => {
                        state.pinned_id = Some(id);
                        state.render(true);
                    }
                    MenuAction::Refresh => {
                        state.refresh(true);
                    }
                    MenuAction::ToggleAutostart => {
                        let _ = autostart::set_enabled(!autostart::is_enabled());
                    }
                    MenuAction::Exit => {
                        unsafe { PostMessageW(hwnd, WM_CLOSE, 0, 0) };
                    }
                    MenuAction::None => {}
                }
            } else if ev == WM_LBUTTONUP {
                state.cycle_device();
            }
            0
        }
        WM_TIMER => {
            if wparam == TIMER_POLL_ID {
                state.refresh(false);
            } else if wparam == TIMER_DEBOUNCE_ID {
                unsafe { KillTimer(hwnd, TIMER_DEBOUNCE_ID) };
                state.refresh(false);
            } else if wparam == TIMER_PROMOTE_ID {
                unsafe { KillTimer(hwnd, TIMER_PROMOTE_ID) };
                let _ = tray::promote_to_taskbar_if_unset();
            }
            0
        }
        WM_DEVICECHANGE => {
            if wparam == DBT_DEVNODES_CHANGED as usize {
                unsafe { SetTimer(hwnd, TIMER_DEBOUNCE_ID, DEBOUNCE_DELAY_MS, None) };
            }
            0
        }
        WM_SETTINGCHANGE => {
            state.render(false);
            0
        }
        WM_CLOSE => {
            unsafe { DestroyWindow(hwnd) };
            0
        }
        WM_DESTROY => {
            unsafe {
                KillTimer(hwnd, TIMER_POLL_ID);
                KillTimer(hwnd, TIMER_DEBOUNCE_ID);
                KillTimer(hwnd, TIMER_PROMOTE_ID);
            }
            state.tray.remove();
            unsafe { PostQuitMessage(0) };
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}
