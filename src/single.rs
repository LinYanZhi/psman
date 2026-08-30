//! 单例控制 — 互斥体 + 通知旧实例重建托盘图标

use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, LPARAM, WPARAM};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW};
use windows::core::w;

use crate::tray::WM_TRAY_RESTORE;

/// 确保只有一个 psman 实例。首次运行返回 true；
/// 已有实例则请其重建托盘图标后本实例退出，返回 false。
pub fn ensure_single_instance() -> bool {
    unsafe {
        let handle = match CreateMutexW(None, false, w!("Local\\psman_single_instance")) {
            Ok(h) => h,
            Err(_) => return true, // 创建失败仍允许运行
        };

        if GetLastError() == ERROR_ALREADY_EXISTS {
            // 已有实例：请其重建托盘图标（新实例可能因 Explorer 重启丢图标），然后退出
            if let Ok(hwnd) = FindWindowW(w!("psman_tray_window"), None) {
                if !hwnd.0.is_null() {
                    let _ = PostMessageW(hwnd, WM_TRAY_RESTORE, WPARAM(0), LPARAM(0));
                }
            }
            let _ = CloseHandle(handle);
            return false;
        }

        // 首次运行：互斥体句柄保持开放（不关闭），使互斥体随进程存续
        // （HANDLE 为 Copy 类型且无 Drop，局部变量即持有句柄，进程退出后由系统回收）
        let _ = handle;
        true
    }
}
