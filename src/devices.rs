// Rust (windows-sys 0.59), Windows 10/11 CfgMgr32 PnP multi-device battery scanner.
use std::collections::HashMap;
use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    CM_GETIDLIST_FILTER_ENUMERATOR, CM_GETIDLIST_FILTER_PRESENT, CM_Get_DevNode_PropertyW,
    CM_Get_DevNode_Status, CM_Get_Device_ID_ListW, CM_Get_Device_ID_List_SizeW,
    CM_LOCATE_DEVNODE_NORMAL, CM_Locate_DevNodeW, CR_BUFFER_SMALL, CR_SUCCESS, DN_STARTED,
};
use windows_sys::Win32::Devices::Properties::{
    DEVPROP_TYPE_BOOLEAN, DEVPROP_TYPE_BYTE, DEVPROP_TYPE_GUID, DEVPROP_TYPE_STRING,
    DEVPROP_TYPE_UINT32, DEVPROPKEY,
};
use windows_sys::core::GUID;

const DEVPKEY_DEVICE_DEVICEDESC: DEVPROPKEY = DEVPROPKEY {
    fmtid: GUID::from_u128(0xa45c254e_df1c_4efd_8020_67d146a850e0),
    pid: 2,
};

const DEVPKEY_DEVICE_CLASS: DEVPROPKEY = DEVPROPKEY {
    fmtid: GUID::from_u128(0xa45c254e_df1c_4efd_8020_67d146a850e0),
    pid: 9,
};

const DEVPKEY_DEVICE_FRIENDLYNAME: DEVPROPKEY = DEVPROPKEY {
    fmtid: GUID::from_u128(0xa45c254e_df1c_4efd_8020_67d146a850e0),
    pid: 14,
};

const DEVPKEY_DEVICE_CONTAINERID: DEVPROPKEY = DEVPROPKEY {
    fmtid: GUID::from_u128(0x8c7ed206_3f8a_4827_b3ab_ae9e1faefc6c),
    pid: 2,
};

const DEVPKEY_DEVICE_ISCONNECTED: DEVPROPKEY = DEVPROPKEY {
    fmtid: GUID::from_u128(0x83da6326_97a6_4088_9453_a1923f573b29),
    pid: 15,
};

const DEVPKEY_BLUETOOTH_BATTERY: DEVPROPKEY = DEVPROPKEY {
    fmtid: GUID::from_u128(0x104ea319_6ee2_4701_bd47_8ddbf425bbe5),
    pid: 2,
};

const DEVPKEY_BLUETOOTH_CLASSOFDEVICE: DEVPROPKEY = DEVPROPKEY {
    fmtid: GUID::from_u128(0xa92f26ca_eda7_4b1d_9db2_27b68aa5a2eb),
    pid: 10,
};

const DEVPKEY_DEVICE_BATTERY_ALT: DEVPROPKEY = DEVPROPKEY {
    fmtid: GUID::from_u128(0x2bd67d8b_8beb_48d5_87e0_6cda3428040a),
    pid: 2,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceCategory {
    Headset,
    Mouse,
    Keyboard,
    Gamepad,
    Bluetooth,
}

impl DeviceCategory {
    pub fn label(self) -> &'static str {
        match self {
            Self::Headset => "Tai nghe",
            Self::Mouse => "Chuột",
            Self::Keyboard => "Bàn phím",
            Self::Gamepad => "Tay cầm",
            Self::Bluetooth => "Bluetooth",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceBattery {
    pub container_id: String,
    pub name: String,
    pub category: DeviceCategory,
    pub battery: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Enumerator {
    BthEnum,
    BthLe,
    BthLeDevice,
    Hid,
}

impl Enumerator {
    fn as_str(self) -> &'static str {
        match self {
            Self::BthEnum => "BTHENUM",
            Self::BthLe => "BTHLE",
            Self::BthLeDevice => "BTHLEDEVICE",
            Self::Hid => "HID",
        }
    }

    fn is_bluetooth(self) -> bool {
        matches!(self, Self::BthEnum | Self::BthLe | Self::BthLeDevice)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RawDeviceNode {
    pub instance_id: String,
    pub enumerator: Enumerator,
    pub container_id: Option<[u8; 16]>,
    pub friendly_name: Option<String>,
    pub device_desc: Option<String>,
    pub pnp_class: Option<String>,
    pub is_connected: Option<bool>,
    pub bt_battery: Option<u8>,
    pub alt_battery: Option<u8>,
    pub class_of_device: Option<u32>,
    pub hid_started: bool,
}

pub fn scan_devices() -> Vec<DeviceBattery> {
    let mut nodes = Vec::new();
    for enumerator in [
        Enumerator::BthEnum,
        Enumerator::BthLe,
        Enumerator::BthLeDevice,
        Enumerator::Hid,
    ] {
        enumerate_by_prefix(enumerator, &mut nodes);
    }
    aggregate_nodes(nodes)
}

pub fn select_active_device<'a>(
    devices: &'a [DeviceBattery],
    pinned_id: Option<&str>,
) -> Option<&'a DeviceBattery> {
    if let Some(id) = pinned_id
        && let Some(found) = devices.iter().find(|d| d.container_id == id)
    {
        return Some(found);
    }
    devices.iter().min_by_key(|d| d.battery)
}

fn enumerate_by_prefix(enumerator: Enumerator, out: &mut Vec<RawDeviceNode>) {
    let filter_wide: Vec<u16> = enumerator
        .as_str()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let flags = CM_GETIDLIST_FILTER_ENUMERATOR | CM_GETIDLIST_FILTER_PRESENT;

    let mut len: u32 = 0;
    let cr = unsafe { CM_Get_Device_ID_List_SizeW(&mut len, filter_wide.as_ptr(), flags) };
    if cr != CR_SUCCESS || len <= 1 {
        return;
    }

    let mut buf = vec![0u16; len as usize];
    let cr = unsafe {
        CM_Get_Device_ID_ListW(filter_wide.as_ptr(), buf.as_mut_ptr(), len, flags)
    };
    if cr != CR_SUCCESS {
        return;
    }

    for slice in buf.split(|&c| c == 0) {
        if slice.is_empty() {
            break;
        }
        let mut id_z = slice.to_vec();
        id_z.push(0);

        let mut devinst: u32 = 0;
        let cr = unsafe {
            CM_Locate_DevNodeW(&mut devinst, id_z.as_ptr(), CM_LOCATE_DEVNODE_NORMAL)
        };
        if cr != CR_SUCCESS {
            continue;
        }

        let instance_id = String::from_utf16_lossy(slice);
        let container_id = read_guid_prop(devinst, &DEVPKEY_DEVICE_CONTAINERID)
            .filter(|g| *g != [0u8; 16]);
        let friendly_name = read_string_prop(devinst, &DEVPKEY_DEVICE_FRIENDLYNAME);
        let device_desc = read_string_prop(devinst, &DEVPKEY_DEVICE_DEVICEDESC);
        let pnp_class = read_string_prop(devinst, &DEVPKEY_DEVICE_CLASS);
        let is_connected = read_bool_prop(devinst, &DEVPKEY_DEVICE_ISCONNECTED);
        let bt_battery = read_battery_prop(devinst, &DEVPKEY_BLUETOOTH_BATTERY);
        let alt_battery = read_battery_prop(devinst, &DEVPKEY_DEVICE_BATTERY_ALT);
        let class_of_device = read_u32_prop(devinst, &DEVPKEY_BLUETOOTH_CLASSOFDEVICE);

        let hid_started = if enumerator == Enumerator::Hid {
            let mut status = 0u32;
            let mut problem = 0u32;
            let st = unsafe { CM_Get_DevNode_Status(&mut status, &mut problem, devinst, 0) };
            st == CR_SUCCESS && (status & DN_STARTED) != 0 && problem == 0
        } else {
            false
        };

        out.push(RawDeviceNode {
            instance_id,
            enumerator,
            container_id,
            friendly_name,
            device_desc,
            pnp_class,
            is_connected,
            bt_battery,
            alt_battery,
            class_of_device,
            hid_started,
        });
    }
}

fn read_prop_raw(devinst: u32, key: &DEVPROPKEY) -> Option<(u32, Vec<u8>)> {
    let mut prop_type = 0u32;
    let mut size = 0u32;
    let cr = unsafe {
        CM_Get_DevNode_PropertyW(
            devinst,
            key,
            &mut prop_type,
            std::ptr::null_mut(),
            &mut size,
            0,
        )
    };
    if cr != CR_BUFFER_SMALL && cr != CR_SUCCESS {
        return None;
    }
    if size == 0 {
        return None;
    }

    let mut buf = vec![0u8; size as usize];
    let cr = unsafe {
        CM_Get_DevNode_PropertyW(
            devinst,
            key,
            &mut prop_type,
            buf.as_mut_ptr(),
            &mut size,
            0,
        )
    };
    if cr != CR_SUCCESS {
        return None;
    }
    buf.truncate(size as usize);
    Some((prop_type, buf))
}

fn read_string_prop(devinst: u32, key: &DEVPROPKEY) -> Option<String> {
    let (prop_type, buf) = read_prop_raw(devinst, key)?;
    if prop_type != DEVPROP_TYPE_STRING || buf.len() < 2 {
        return None;
    }
    let words: Vec<u16> = buf
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .take_while(|&w| w != 0)
        .collect();
    let s = String::from_utf16_lossy(&words).trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

fn read_guid_prop(devinst: u32, key: &DEVPROPKEY) -> Option<[u8; 16]> {
    let (prop_type, buf) = read_prop_raw(devinst, key)?;
    if prop_type != DEVPROP_TYPE_GUID || buf.len() != 16 {
        return None;
    }
    let mut out = [0u8; 16];
    out.copy_from_slice(&buf);
    Some(out)
}

fn read_bool_prop(devinst: u32, key: &DEVPROPKEY) -> Option<bool> {
    let (prop_type, buf) = read_prop_raw(devinst, key)?;
    if prop_type != DEVPROP_TYPE_BOOLEAN || buf.is_empty() {
        return None;
    }
    // DEVPROP_TRUE in Windows PnP is 0xFF (-1), never compare against 1.
    Some(buf[0] != 0)
}

fn read_u32_prop(devinst: u32, key: &DEVPROPKEY) -> Option<u32> {
    let (prop_type, buf) = read_prop_raw(devinst, key)?;
    if prop_type != DEVPROP_TYPE_UINT32 || buf.len() < 4 {
        return None;
    }
    Some(u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]))
}

fn read_battery_prop(devinst: u32, key: &DEVPROPKEY) -> Option<u8> {
    let (prop_type, buf) = read_prop_raw(devinst, key)?;
    let val = if prop_type == DEVPROP_TYPE_BYTE && !buf.is_empty() {
        buf[0] as u32
    } else if prop_type == DEVPROP_TYPE_UINT32 && buf.len() >= 4 {
        u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]])
    } else {
        return None;
    };
    if val <= 100 { Some(val as u8) } else { None }
}

pub(crate) fn aggregate_nodes(nodes: Vec<RawDeviceNode>) -> Vec<DeviceBattery> {
    let mut groups: HashMap<String, Vec<RawDeviceNode>> = HashMap::new();
    for node in nodes {
        let key = match node.container_id {
            Some(guid) => format_guid_bytes(&guid),
            None => format!("ID:{}", node.instance_id.to_ascii_uppercase()),
        };
        groups.entry(key).or_default().push(node);
    }

    let mut result = Vec::new();
    for (container_id, group) in groups {
        let has_bt = group.iter().any(|n| n.enumerator.is_bluetooth());
        let connected = if has_bt {
            group.iter().any(|n| n.is_connected == Some(true))
        } else {
            group.iter().any(|n| n.hid_started && n.is_connected != Some(false))
        };
        if !connected {
            continue;
        }

        let battery = group
            .iter()
            .filter_map(|n| n.bt_battery)
            .next()
            .or_else(|| group.iter().filter_map(|n| n.alt_battery).next());
        let Some(battery) = battery else {
            continue;
        };

        let name = choose_best_name(&group);
        let category = infer_category(&group, &name);

        result.push(DeviceBattery {
            container_id,
            name,
            category,
            battery,
        });
    }

    result.sort_by(|a, b| {
        a.name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase())
            .then_with(|| a.container_id.cmp(&b.container_id))
    });
    result
}

fn format_guid_bytes(b: &[u8; 16]) -> String {
    let d1 = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
    let d2 = u16::from_le_bytes([b[4], b[5]]);
    let d3 = u16::from_le_bytes([b[6], b[7]]);
    format!(
        "{d1:08X}-{d2:04X}-{d3:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
        b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]
    )
}

fn is_generic_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.starts_with("hid-compliant")
        || lower.starts_with("usb input device")
        || lower.starts_with("generic attribute")
        || lower == "bluetooth"
        || lower == "bluetooth device"
        || lower.contains("service discovery")
}

fn clean_device_name(raw: &str) -> String {
    let mut name = raw.trim().to_string();
    for suffix in [
        " Hands-Free AG",
        " Hands-Free",
        " Avrcp Transport",
        " Stereo",
        " BLE Device",
    ] {
        if let Some(stripped) = name.strip_suffix(suffix)
            && !stripped.trim().is_empty()
        {
            name = stripped.trim().to_string();
        }
    }
    name
}

fn choose_best_name(group: &[RawDeviceNode]) -> String {
    // Prefer the root Bluetooth device node (BTHENUM\DEV_* or BTHLE\Dev_*).
    for node in group {
        let upper = node.instance_id.to_ascii_uppercase();
        if (upper.starts_with("BTHENUM\\DEV_") || upper.starts_with("BTHLE\\DEV_"))
            && let Some(raw) = &node.friendly_name
        {
            let cleaned = clean_device_name(raw);
            if !is_generic_name(&cleaned) {
                return cleaned;
            }
        }
    }

    let mut candidates: Vec<String> = group
        .iter()
        .filter_map(|n| n.friendly_name.as_deref().or(n.device_desc.as_deref()))
        .map(clean_device_name)
        .filter(|s| !s.is_empty() && !is_generic_name(s))
        .collect();

    candidates.sort_by_key(|s| s.len());
    candidates
        .into_iter()
        .next()
        .or_else(|| {
            group
                .iter()
                .filter_map(|n| n.friendly_name.clone().or_else(|| n.device_desc.clone()))
                .next()
        })
        .unwrap_or_else(|| "Bluetooth Device".to_string())
}

fn infer_category(group: &[RawDeviceNode], clean_name: &str) -> DeviceCategory {
    for node in group {
        if let Some(cod) = node.class_of_device {
            let major = (cod >> 8) & 0x1F;
            let minor = (cod >> 2) & 0x3F;
            if major == 0x04 {
                return DeviceCategory::Headset;
            }
            if major == 0x05 {
                if (minor & 0x20) != 0 {
                    return DeviceCategory::Mouse;
                }
                if (minor & 0x10) != 0 {
                    return DeviceCategory::Keyboard;
                }
                if minor == 0x01 || minor == 0x02 {
                    return DeviceCategory::Gamepad;
                }
            }
        }
    }

    for node in group {
        let id_upper = node.instance_id.to_ascii_uppercase();
        if id_upper.contains("0000111E") || id_upper.contains("0000110B") {
            return DeviceCategory::Headset;
        }
        if let Some(cls) = &node.pnp_class {
            let c = cls.to_ascii_lowercase();
            if c == "mouse" {
                return DeviceCategory::Mouse;
            }
            if c == "keyboard" {
                return DeviceCategory::Keyboard;
            }
            if c == "audioendpoint" || c == "media" {
                return DeviceCategory::Headset;
            }
        }
    }

    let lower = clean_name.to_ascii_lowercase();
    if [
        "mouse", "mx master", "mx anywhere", "deathadder", "viper", "g pro", "basilisk",
        "orochi", "aerox", "trackball",
    ]
    .iter()
    .any(|k| lower.contains(k))
    {
        return DeviceCategory::Mouse;
    }

    if ["keyboard", "keychron", "k380", "k780", "nuphy", "anne pro", "mx keys"]
        .iter()
        .any(|k| lower.contains(k))
    {
        return DeviceCategory::Keyboard;
    }

    if ["controller", "gamepad", "xbox", "dualsense", "dualshock", "8bitdo", "joy-con"]
        .iter()
        .any(|k| lower.contains(k))
    {
        return DeviceCategory::Gamepad;
    }

    if [
        "headset", "headphone", "earbud", "airpod", "pods", "buds", "soundpeats", "sony wh",
        "sony wf", "bose", "jbl", "marshall", "edifier", "sennheiser", "havit", "qcy", "anker",
    ]
    .iter()
    .any(|k| lower.contains(k))
    {
        return DeviceCategory::Headset;
    }

    DeviceCategory::Bluetooth
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(
        instance_id: &str,
        enumerator: Enumerator,
        container_id: [u8; 16],
        friendly_name: Option<&str>,
        is_connected: Option<bool>,
        bt_battery: Option<u8>,
    ) -> RawDeviceNode {
        RawDeviceNode {
            instance_id: instance_id.to_string(),
            enumerator,
            container_id: Some(container_id),
            friendly_name: friendly_name.map(str::to_string),
            device_desc: None,
            pnp_class: None,
            is_connected,
            bt_battery,
            alt_battery: None,
            class_of_device: None,
            hid_started: false,
        }
    }

    #[test]
    fn correlates_split_win11_bluetooth_nodes_by_container_id() {
        let sp_cid = [0x40; 16];
        let havit_cid = [0x8E; 16];

        let nodes = vec![
            // Root device holds IsConnected = true, no battery
            node(
                "BTHENUM\\DEV_41423652CBD9\\8&2EEE4740&1",
                Enumerator::BthEnum,
                sp_cid,
                Some("SOUNDPEATS T3 Pro"),
                Some(true),
                None,
            ),
            // Hands-Free AG holds battery = 100, IsConnected = false
            node(
                "BTHENUM\\{0000111E-0000-1000-8000-00805F9B34FB}_LOCALMFG",
                Enumerator::BthEnum,
                sp_cid,
                Some("SOUNDPEATS T3 Pro Hands-Free AG"),
                Some(false),
                Some(100),
            ),
            // Disconnected headset with stale cached battery must be filtered out
            node(
                "BTHENUM\\DEV_EE7FAA324B12\\8&2EEE4740&1",
                Enumerator::BthEnum,
                havit_cid,
                Some("HAVIT H619BT"),
                Some(false),
                None,
            ),
            node(
                "BTHENUM\\{0000111E-0000-1000-8000-00805F9B34FB}_VID",
                Enumerator::BthEnum,
                havit_cid,
                Some("HAVIT H619BT Hands-Free AG"),
                Some(false),
                Some(80),
            ),
        ];

        let devs = aggregate_nodes(nodes);
        assert_eq!(devs.len(), 1);
        assert_eq!(devs[0].name, "SOUNDPEATS T3 Pro");
        assert_eq!(devs[0].battery, 100);
        assert_eq!(devs[0].category, DeviceCategory::Headset);
    }

    #[test]
    fn aggregates_multiple_device_types_and_selects_lowest_battery() {
        let headset_cid = [1; 16];
        let mut mouse_node = node(
            "BTHLE\\Dev_AABBCCDDEEFF\\7&1",
            Enumerator::BthLe,
            [2; 16],
            Some("MX Master 3S"),
            Some(true),
            None,
        );
        mouse_node.pnp_class = Some("Mouse".to_string());

        let mouse_batt = node(
            "BTHLEDEVICE\\{0000180F-0000-1000-8000-00805F9B34FB}\\8&1",
            Enumerator::BthLeDevice,
            [2; 16],
            Some("Battery Service"),
            Some(true),
            Some(28),
        );

        let mut kb_node = node(
            "BTHLE\\Dev_112233445566\\7&1",
            Enumerator::BthLe,
            [3; 16],
            Some("Keychron K2"),
            Some(true),
            Some(64),
        );
        kb_node.pnp_class = Some("Keyboard".to_string());

        let hs_node = node(
            "BTHENUM\\DEV_998877665544\\8&1",
            Enumerator::BthEnum,
            headset_cid,
            Some("Sony WH-1000XM5"),
            Some(true),
            Some(92),
        );

        let devs = aggregate_nodes(vec![hs_node, mouse_node, mouse_batt, kb_node]);
        assert_eq!(devs.len(), 3);
        // Sorted alphabetically
        assert_eq!(devs[0].name, "Keychron K2");
        assert_eq!(devs[0].category, DeviceCategory::Keyboard);
        assert_eq!(devs[1].name, "MX Master 3S");
        assert_eq!(devs[1].category, DeviceCategory::Mouse);
        assert_eq!(devs[2].name, "Sony WH-1000XM5");
        assert_eq!(devs[2].category, DeviceCategory::Headset);

        // Auto mode picks lowest battery (MX Master 3S at 28%)
        let auto_pick = select_active_device(&devs, None).unwrap();
        assert_eq!(auto_pick.name, "MX Master 3S");

        // Pinned mode keeps pinned device
        let pinned_pick = select_active_device(&devs, Some(&devs[2].container_id)).unwrap();
        assert_eq!(pinned_pick.name, "Sony WH-1000XM5");
    }
}
