// Rust (windows-sys 0.59), Apple/iOS minimal taskbar pill window & message loop.
#![windows_subsystem = "windows"]

mod autostart;
mod devices;
mod render;
mod settings;
mod tray;

use devices::{DeviceBattery, DeviceCategory, scan_devices, select_active_device};
use render::RenderedPill;
use settings::Settings;
use tray::{MenuAction, format_tooltip, show_context_menu};
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, HINSTANCE, HWND, LPARAM, LRESULT,
    RECT, WPARAM,
};
use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::Controls::{
    TOOLTIPS_CLASSW, TTF_IDISHWND, TTF_SUBCLASS, TTM_ADDTOOLW, TTM_SETMAXTIPWIDTH,
    TTM_UPDATETIPTEXTW, TTS_ALWAYSTIP, TTS_NOPREFIX, TTTOOLINFOW, WM_MOUSELEAVE,
};
use windows_sys::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForSystem, GetDpiForWindow,
    SetProcessDpiAwarenessContext,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
};
use windows_sys::Win32::UI::Shell::{
    QUNS_BUSY, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN, SHQueryUserNotificationState,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DBT_DEVNODES_CHANGED, DefWindowProcW, DestroyWindow, DispatchMessageW,
    FindWindowExW, FindWindowW, GW_HWNDPREV, GWLP_USERDATA, GetMessageW, GetWindow,
    GetWindowLongPtrW, GetWindowRect, HWND_TOPMOST, IDC_ARROW, IsWindowVisible, KillTimer,
    LoadCursorW, MSG, PostQuitMessage, RegisterClassExW, RegisterWindowMessageW, SW_HIDE,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SendMessageW, SetTimer,
    SetWindowLongPtrW, SetWindowPos, ShowWindow, TranslateMessage, WM_CLOSE, WM_CONTEXTMENU,
    WM_DESTROY, WM_DEVICECHANGE, WM_LBUTTONUP, WM_MOUSEMOVE, WM_RBUTTONUP, WM_SETTINGCHANGE,
    WM_TIMER, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_POPUP,
};

const TIMER_POLL_ID: usize = 1;
const TIMER_DEBOUNCE_ID: usize = 2;
const TIMER_SYNC_ID: usize = 3;

const POLL_INTERVAL_MS: u32 = 15_000;
const DEBOUNCE_DELAY_MS: u32 = 1_000;
const SYNC_INTERVAL_MS: u32 = 750;

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
    category: DeviceCategory,
    device_name: Option<String>,
    show_name: bool,
    show_bar: bool,
    light_taskbar: bool,
    hovered: bool,
    dpi: u32,
}

struct AppState {
    hwnd: HWND,
    tooltip_hwnd: HWND,
    hinstance: HINSTANCE,
    taskbar_created_msg: u32,
    devices: Vec<DeviceBattery>,
    settings: Settings,
    hovered: bool,
    dpi: u32,
    pos_x: i32,
    pos_y: i32,
    visible: bool,
    last_render: Option<RenderKey>,
    last_tooltip: String,
    pill: Option<RenderedPill>,
}

impl AppState {
    fn new(hwnd: HWND, hinstance: HINSTANCE, taskbar_created_msg: u32) -> Self {
        let tooltip_hwnd = create_tooltip_window(hwnd, hinstance);
        let dpi = unsafe { GetDpiForSystem() }.max(96);
        Self {
            hwnd,
            tooltip_hwnd,
            hinstance,
            taskbar_created_msg,
            devices: Vec::new(),
            settings: Settings::load(),
            hovered: false,
            dpi,
            pos_x: 0,
            pos_y: 0,
            visible: false,
            last_render: None,
            last_tooltip: String::new(),
            pill: None,
        }
    }

    fn active_device(&self) -> Option<&DeviceBattery> {
        select_active_device(&self.devices, self.settings.selected_device.as_deref())
    }

    fn refresh(&mut self, force_redraw: bool) {
        self.devices = scan_devices();
        self.update_visuals(force_redraw);
        self.sync_position(true);
    }

    fn update_visuals(&mut self, force_redraw: bool) {
        let active = self.active_device();
        let active_id = active.map(|d| d.container_id.as_str());
        let battery = active.map(|d| d.battery);
        let category = active.map(|d| d.category).unwrap_or(DeviceCategory::Headset);
        let device_name = active.map(|d| d.name.clone());
        let light = render::is_light_taskbar();
        let tip = format_tooltip(&self.devices, active_id);

        if tip != self.last_tooltip {
            update_tooltip_text(self.tooltip_hwnd, self.hwnd, self.hinstance, &tip);
            self.last_tooltip = tip;
        }

        let key = RenderKey {
            battery,
            category,
            device_name: if self.settings.show_device_name {
                device_name.clone()
            } else {
                None
            },
            show_name: self.settings.show_device_name,
            show_bar: self.settings.show_battery_bar,
            light_taskbar: light,
            hovered: self.hovered,
            dpi: self.dpi,
        };
        if !force_redraw && self.last_render.as_ref() == Some(&key) {
            return;
        }

        let pill = render::render_pill(
            battery,
            category,
            device_name.as_deref(),
            self.settings.show_device_name,
            self.settings.show_battery_bar,
            light,
            self.hovered,
            self.dpi,
        );
        self.pill = Some(pill);
        self.last_render = Some(key);
        if self.visible
            && let Some(p) = &self.pill
        {
            render::apply_to_layered_window(self.hwnd, self.pos_x, self.pos_y, p);
        }
    }

    fn sync_position(&mut self, force_apply: bool) {
        let Some((tray_hwnd, tray_rc, tray_dpi, anchor_right)) = find_taskbar_geometry() else {
            self.hide();
            return;
        };
        if is_fullscreen_active() {
            self.hide();
            return;
        }

        if tray_dpi != self.dpi {
            self.dpi = tray_dpi;
            self.update_visuals(true);
        }
        let Some(pill) = &self.pill else {
            return;
        };

        let x = anchor_right - pill.width;
        let y = tray_rc.top + ((tray_rc.bottom - tray_rc.top) - pill.height) / 2;

        let changed = !self.visible || self.pos_x != x || self.pos_y != y;
        self.pos_x = x;
        self.pos_y = y;

        if changed || force_apply {
            render::apply_to_layered_window(self.hwnd, x, y, pill);
            unsafe {
                SetWindowPos(
                    self.hwnd,
                    HWND_TOPMOST,
                    x,
                    y,
                    pill.width,
                    pill.height,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
            }
            self.visible = true;
        } else if !is_window_above(self.hwnd, tray_hwnd) {
            unsafe {
                SetWindowPos(
                    self.hwnd,
                    HWND_TOPMOST,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
            }
        }
    }

    fn hide(&mut self) {
        if self.visible {
            unsafe { ShowWindow(self.hwnd, SW_HIDE) };
            self.visible = false;
        }
    }

    fn cycle_device(&mut self) {
        if self.devices.len() <= 1 {
            self.refresh(true);
            return;
        }
        let current_id = self.active_device().map(|d| d.container_id.as_str());
        let next_idx = current_id
            .and_then(|id| self.devices.iter().position(|d| d.container_id == id))
            .map(|i| (i + 1) % self.devices.len())
            .unwrap_or(0);
        self.settings.selected_device = Some(self.devices[next_idx].container_id.clone());
        self.settings.save();
        self.update_visuals(true);
        self.sync_position(true);
    }
}

fn is_window_above(target: HWND, reference: HWND) -> bool {
    let mut cur = unsafe { GetWindow(reference, GW_HWNDPREV) };
    let mut steps = 0usize;
    while !cur.is_null() && steps < 64 {
        if cur == target {
            return true;
        }
        cur = unsafe { GetWindow(cur, GW_HWNDPREV) };
        steps += 1;
    }
    false
}

fn find_taskbar_geometry() -> Option<(HWND, RECT, u32, i32)> {
    let tray_cls: Vec<u16> = "Shell_TrayWnd\0".encode_utf16().collect();
    let tray_hwnd = unsafe { FindWindowW(tray_cls.as_ptr(), std::ptr::null()) };
    if tray_hwnd.is_null() || unsafe { IsWindowVisible(tray_hwnd) } == 0 {
        return None;
    }

    let mut tray_rc: RECT = unsafe { std::mem::zeroed() };
    if unsafe { GetWindowRect(tray_hwnd, &mut tray_rc) } == 0 {
        return None;
    }
    if tray_rc.bottom - tray_rc.top < 20 || tray_rc.right - tray_rc.left < 120 {
        return None;
    }

    let dpi = unsafe { GetDpiForWindow(tray_hwnd) }.max(96);
    let scale = dpi as f32 / 96.0;
    let gap = (2.0 * scale).round() as i32;

    let notify_cls: Vec<u16> = "TrayNotifyWnd\0".encode_utf16().collect();
    let notify_hwnd = unsafe {
        FindWindowExW(
            tray_hwnd,
            std::ptr::null_mut(),
            notify_cls.as_ptr(),
            std::ptr::null(),
        )
    };

    let mut anchor_right = tray_rc.right - (220.0 * scale).round() as i32;
    if !notify_hwnd.is_null() {
        let mut notify_rc: RECT = unsafe { std::mem::zeroed() };
        if unsafe { GetWindowRect(notify_hwnd, &mut notify_rc) } != 0
            && notify_rc.left > tray_rc.left + 80
            && notify_rc.right <= tray_rc.right + 4
        {
            anchor_right = notify_rc.left - gap;
        }
    }

    Some((tray_hwnd, tray_rc, dpi, anchor_right))
}

fn is_fullscreen_active() -> bool {
    let mut state = 0i32;
    let hr = unsafe { SHQueryUserNotificationState(&mut state) };
    hr >= 0
        && (state == QUNS_BUSY
            || state == QUNS_RUNNING_D3D_FULL_SCREEN
            || state == QUNS_PRESENTATION_MODE)
}

fn create_tooltip_window(owner_hwnd: HWND, hinstance: HINSTANCE) -> HWND {
    let tip_hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST,
            TOOLTIPS_CLASSW,
            std::ptr::null(),
            WS_POPUP | TTS_ALWAYSTIP | TTS_NOPREFIX,
            0,
            0,
            0,
            0,
            owner_hwnd,
            std::ptr::null_mut(),
            hinstance,
            std::ptr::null(),
        )
    };
    if tip_hwnd.is_null() {
        return std::ptr::null_mut();
    }

    let mut empty = [0u16; 1];
    let mut ti: TTTOOLINFOW = unsafe { std::mem::zeroed() };
    ti.cbSize = std::mem::size_of::<TTTOOLINFOW>() as u32;
    ti.uFlags = TTF_IDISHWND | TTF_SUBCLASS;
    ti.hwnd = owner_hwnd;
    ti.uId = owner_hwnd as usize;
    ti.hinst = hinstance;
    ti.lpszText = empty.as_mut_ptr();

    unsafe {
        SendMessageW(tip_hwnd, TTM_ADDTOOLW, 0, (&ti as *const TTTOOLINFOW) as isize);
        SendMessageW(tip_hwnd, TTM_SETMAXTIPWIDTH, 0, 360);
    }
    tip_hwnd
}

fn update_tooltip_text(tip_hwnd: HWND, owner_hwnd: HWND, hinstance: HINSTANCE, text: &str) {
    if tip_hwnd.is_null() {
        return;
    }
    let mut wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let mut ti: TTTOOLINFOW = unsafe { std::mem::zeroed() };
    ti.cbSize = std::mem::size_of::<TTTOOLINFOW>() as u32;
    ti.hwnd = owner_hwnd;
    ti.uId = owner_hwnd as usize;
    ti.hinst = hinstance;
    ti.lpszText = wide.as_mut_ptr();

    unsafe {
        SendMessageW(
            tip_hwnd,
            TTM_UPDATETIPTEXTW,
            0,
            (&ti as *const TTTOOLINFOW) as isize,
        );
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

    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST,
            class_name.as_ptr(),
            class_name.as_ptr(),
            WS_POPUP,
            0,
            0,
            1,
            1,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            hinstance,
            std::ptr::null(),
        )
    };
    if hwnd.is_null() {
        return;
    }

    let state_ptr = Box::into_raw(Box::new(AppState::new(hwnd, hinstance, taskbar_created_msg)));
    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, state_ptr as isize);
        (*state_ptr).refresh(true);
        SetTimer(hwnd, TIMER_POLL_ID, POLL_INTERVAL_MS, None);
        SetTimer(hwnd, TIMER_SYNC_ID, SYNC_INTERVAL_MS, None);
    }

    let mut msg: MSG = unsafe { std::mem::zeroed() };
    while unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) } > 0 {
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

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
        state.update_visuals(true);
        state.sync_position(true);
        return 0;
    }

    match msg {
        WM_MOUSEMOVE => {
            if !state.hovered {
                state.hovered = true;
                let mut tme = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                unsafe { TrackMouseEvent(&mut tme) };
                state.update_visuals(false);
            }
            0
        }
        WM_MOUSELEAVE => {
            if state.hovered {
                state.hovered = false;
                state.update_visuals(false);
            }
            0
        }
        WM_LBUTTONUP => {
            state.cycle_device();
            0
        }
        WM_RBUTTONUP | WM_CONTEXTMENU => {
            let active_id = state.active_device().map(|d| d.container_id.clone());
            let action = show_context_menu(
                hwnd,
                &state.devices,
                active_id.as_deref(),
                state.settings.show_battery_bar,
                state.settings.show_device_name,
                autostart::is_enabled(),
            );
            match action {
                MenuAction::SelectDevice(id) => {
                    state.settings.selected_device = Some(id);
                    state.settings.save();
                    state.update_visuals(true);
                    state.sync_position(true);
                }
                MenuAction::ToggleBatteryBar => {
                    state.settings.show_battery_bar = !state.settings.show_battery_bar;
                    state.settings.save();
                    state.update_visuals(true);
                    state.sync_position(true);
                }
                MenuAction::ToggleDeviceName => {
                    state.settings.show_device_name = !state.settings.show_device_name;
                    state.settings.save();
                    state.update_visuals(true);
                    state.sync_position(true);
                }
                MenuAction::Refresh => {
                    state.refresh(true);
                }
                MenuAction::ToggleAutostart => {
                    let _ = autostart::set_enabled(!autostart::is_enabled());
                }
                MenuAction::Exit => {
                    unsafe { DestroyWindow(hwnd) };
                }
                MenuAction::None => {}
            }
            0
        }
        WM_TIMER => {
            if wparam == TIMER_POLL_ID {
                state.refresh(false);
            } else if wparam == TIMER_DEBOUNCE_ID {
                unsafe { KillTimer(hwnd, TIMER_DEBOUNCE_ID) };
                state.refresh(false);
            } else if wparam == TIMER_SYNC_ID {
                state.sync_position(false);
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
            state.update_visuals(false);
            state.sync_position(false);
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
                KillTimer(hwnd, TIMER_SYNC_ID);
                if !state.tooltip_hwnd.is_null() {
                    DestroyWindow(state.tooltip_hwnd);
                    state.tooltip_hwnd = std::ptr::null_mut();
                }
            }
            unsafe { PostQuitMessage(0) };
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}
