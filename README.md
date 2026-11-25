# BatteryDevice

Ứng dụng Windows siêu nhẹ viết bằng **Rust** (thuần Win32 API, không framework nặng), hiển thị tối giản **icon thiết bị + % pin** (`🎧 90%`, `🖱 85%`, `⌨ 60%`) trực tiếp ngay trên **thanh Taskbar** cạnh khay hệ thống.

---

## Tải về & Sử dụng ngay (Không cần cài đặt)

1. Vào trang [**Releases**](https://github.com/Dattrong0512/batterry-display/releases/latest) và tải file **`BatteryDevice.exe`**.
2. Chạy trực tiếp `BatteryDevice.exe` (tự động link tĩnh CRT — **không cần cài Visual C++ Redistributable hay .NET**).
3. Widget trong suốt tự động neo ngay bên cạnh mũi tên khay hệ thống trên Taskbar Windows 10/11 và tự ẩn khi có ứng dụng toàn màn hình.

---

## Tính năng chính

- **Phong cách tối giản kiểu iPhone**: Vẽ vector icon từng loại thiết bị (tai nghe, chuột, bàn phím, tay cầm, Bluetooth) và font `Segoe UI Variable Text` Medium bằng **4x supersampling**, đơn sắc tinh gọn (tự chuyển cam/đỏ khi pin `<= 20%`).
- **Nhận diện đa thiết bị tự động**: Hỗ trợ Bluetooth Classic (tai nghe HFP), Bluetooth LE / GATT Battery Service (chuột, bàn phím, tai nghe BLE, bút cảm ứng) và thiết bị HID không dây có báo pin qua Windows PnP.
- **Siêu nhẹ & không tốn tài nguyên**: Dung lượng file `.exe` chỉ ~270 KB, tiêu thụ ~1.5–3 MB private RAM, ~0% CPU khi chạy nền (event-driven Win32 loop).
- **Cập nhật tức thì & thích ứng nền**: Lắng nghe `WM_DEVICECHANGE` (phản hồi ~1s khi bật/tắt thiết bị), tự đổi màu theo Dark/Light Taskbar và DPI màn hình.
- **Tương tác trực tiếp trên Taskbar**:
  - **Di chuột (hover)**: Hiện nền pill mờ + tooltip % pin của tất cả thiết bị đang kết nối.
  - **Click chuột trái**: Chuyển nhanh qua thiết bị tiếp theo để hiển thị trên Taskbar.
  - **Click chuột phải**: Chọn trực tiếp thiết bị muốn hiển thị (tự ghi nhớ vào cấu hình), bật/tắt **Hiện thanh pin**, bật/tắt **Hiện tên thiết bị**, bật/tắt **Khởi động cùng Windows**, làm mới thủ công, hoặc thoát ứng dụng.

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
