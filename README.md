# BatteryDevice

Ứng dụng Windows siêu nhẹ viết bằng **Rust** (thuần Win32 API, không framework nặng), hiển thị trực tiếp **% pin của tai nghe, chuột, bàn phím, tay cầm và các thiết bị Bluetooth/HID đang kết nối** ngay tại **System Tray** góc phải thanh Taskbar.

---

## Tải về & Sử dụng ngay (Không cần cài đặt)

1. Vào trang [**Releases**](https://github.com/Dattrong0512/batterry-display/releases/latest) và tải file **`BatteryDevice.exe`**.
2. Chạy trực tiếp `BatteryDevice.exe` (tự động link tĩnh CRT — **không cần cài Visual C++ Redistributable hay .NET**).
3. **Hiển thị trực tiếp trên Taskbar (Windows 11)**: Khi chạy lần đầu, ứng dụng tự động ghim icon ra ngoài thanh Taskbar (nếu cần bật/tắt lại thủ công, vào *Taskbar settings* → *Other system tray icons* → **BatteryDevice**).

---

## Tính năng chính

- **Nhận diện đa thiết bị tự động**: Hỗ trợ Bluetooth Classic (tai nghe HFP), Bluetooth LE / GATT Battery Service (chuột, bàn phím, tai nghe BLE, bút cảm ứng) và thiết bị HID không dây có báo pin qua Windows PnP.
- **Siêu nhẹ & không tốn tài nguyên**: Dung lượng file `.exe` chỉ khoảng vài trăm KB, tiêu thụ ~1.5–3 MB RAM, ~0% CPU khi chạy nền (event-driven Win32 loop).
- **Cập nhật tức thì**: Tự lắng nghe `WM_DEVICECHANGE` (phản hồi ~1s khi bật/tắt thiết bị) kết hợp làm mới định kỳ mỗi 15 giây.
- **Tự thích ứng giao diện & DPI**: Đọc `SystemUsesLightTheme` và `SM_CXSMICON` để vẽ chữ sắc nét trên cả Taskbar sáng/tối và màn hình độ phân giải cao.
- **Quản lý nhiều thiết bị tiện lợi**:
  - Di chuột (hover) lên icon để xem % pin của tất cả thiết bị đang kết nối.
  - **Click chuột trái**: Chuyển nhanh qua thiết bị tiếp theo để hiển thị số % lên icon.
  - **Click chuột phải**: Ghim cố định 1 thiết bị hoặc để chế độ **Tự động (ưu tiên pin thấp nhất)**, bật/tắt **Khởi động cùng Windows**, làm mới thủ công, hoặc thoát ứng dụng.

---

## Kiểm tra thiết bị từ CLI

Chạy lệnh sau để in danh sách thiết bị đang kết nối và % pin Windows nhận diện được:

```powershell
.\BatteryDevice.exe --probe
```

---

## Tự build từ mã nguồn

Yêu cầu: Rust toolchain (`x86_64-pc-windows-msvc`) và Windows SDK / MSVC Build Tools.

```powershell
# Chạy unit tests
cargo test

# Build bản release standalone
cargo build --release
```

File đầu ra nằm tại `target\release\BatteryDevice.exe`.

---

## Giấy phép

Phát hành theo giấy phép [MIT](LICENSE).
