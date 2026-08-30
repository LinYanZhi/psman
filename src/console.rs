//! 控制台生命周期 — 继承父控制台 / 自行分配 / 显隐切换 / 关闭处理（隐藏不退出）

use std::os::windows::io::AsRawHandle;

use windows::Win32::Foundation::{BOOL, HANDLE};
use windows::Win32::System::Console::{
    AllocConsole, AttachConsole, FreeConsole, GetConsoleScreenBufferInfo, GetConsoleWindow,
    GetStdHandle, SetConsoleCtrlHandler, SetConsoleTitleW, SetStdHandle, CONSOLE_SCREEN_BUFFER_INFO,
    STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, ATTACH_PARENT_PROCESS,
    CTRL_CLOSE_EVENT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    IsWindowVisible, SetForegroundWindow, ShowWindow, SW_HIDE, SW_SHOW,
};
use windows::core::w;

/// 继承父进程控制台（从 cmd/powershell 启动时）。成功（或已有控制台）则重定向标准句柄。
pub fn attach_parent() -> bool {
    unsafe {
        let attached = AttachConsole(ATTACH_PARENT_PROCESS).is_ok() || !GetConsoleWindow().0.is_null();
        if attached {
            redirect_std_handles();
        }
        attached
    }
}

/// 自行分配控制台（双击启动 / 单次执行无父控制台时）
pub fn alloc() {
    unsafe {
        if GetConsoleWindow().0.is_null() {
            let _ = AllocConsole();
        }
        redirect_std_handles();
        let _ = SetConsoleTitleW(w!("psman - 进程/端口/线程管理"));
    }
}

/// 打开/切换控制台显隐（托盘左键）
pub fn open_or_toggle() {
    unsafe {
        let con = GetConsoleWindow();
        if con.0.is_null() {
            alloc();
            let con = GetConsoleWindow();
            if !con.0.is_null() {
                let _ = ShowWindow(con, SW_SHOW);
                let _ = SetForegroundWindow(con);
            }
        } else if IsWindowVisible(con).as_bool() {
            let _ = ShowWindow(con, SW_HIDE);
        } else {
            let _ = ShowWindow(con, SW_SHOW);
            let _ = SetForegroundWindow(con);
        }
    }
}

/// 当前是否有可见的控制台窗口（REPL 重启循环轮询用）
pub fn console_visible() -> bool {
    unsafe {
        let con = GetConsoleWindow();
        if con.0.is_null() {
            return false;
        }
        IsWindowVisible(con).as_bool()
    }
}

/// 控制台窗口宽度（列数）；无法获取时回退 100
pub fn console_width() -> usize {
    unsafe {
        let Ok(h) = GetStdHandle(STD_OUTPUT_HANDLE) else {
            return 100;
        };
        let mut info = CONSOLE_SCREEN_BUFFER_INFO::default();
        if GetConsoleScreenBufferInfo(h, &mut info).is_ok() {
            let w = (info.srWindow.Right - info.srWindow.Left + 1) as usize;
            if w > 10 {
                return w;
            }
        }
    }
    100
}

/// 隐藏并断开当前控制台（REPL 结束时 / 关闭事件时调用）
pub fn hide_and_free() {
    unsafe {
        let con = GetConsoleWindow();
        if !con.0.is_null() {
            let _ = ShowWindow(con, SW_HIDE);
            let _ = FreeConsole();
        }
    }
}

/// 注册控制台关闭处理：点击 X 只隐藏并断开控制台，进程保持运行（托盘仍在）
pub fn set_close_handler() {
    unsafe {
        let _ = SetConsoleCtrlHandler(Some(ctrl_handler), BOOL(1));
    }
}

unsafe extern "system" fn ctrl_handler(ctrl_type: u32) -> BOOL {
    if ctrl_type == CTRL_CLOSE_EVENT {
        // 先隐藏，再异步断开，避免在 Ctrl 处理器线程里直接 FreeConsole 导致
        // 后续 AllocConsole 在部分 Windows 版本上失败
        std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_millis(100));
            unsafe {
                let _ = FreeConsole();
            }
            crate::repl::REPL_ACTIVE.store(false, std::sync::atomic::Ordering::Relaxed);
        });
        BOOL(1)
    } else {
        BOOL(0)
    }
}

/// 重定向标准句柄到控制台（GUI 子系统进程默认不继承控制台句柄，必须显式设置；
/// 必须在任何 println 之前调用，Rust std 首次使用时会缓存句柄）
fn redirect_std_handles() {
    unsafe {
        if let Ok(f) = std::fs::OpenOptions::new().read(true).write(true).open("CONOUT$") {
            let h = HANDLE(f.as_raw_handle() as *mut core::ffi::c_void);
            let _ = SetStdHandle(STD_OUTPUT_HANDLE, h);
            let _ = SetStdHandle(STD_ERROR_HANDLE, h);
            std::mem::forget(f); // 句柄交由系统保持，进程退出时自动关闭
        }
        if let Ok(f) = std::fs::OpenOptions::new().read(true).write(true).open("CONIN$") {
            let h = HANDLE(f.as_raw_handle() as *mut core::ffi::c_void);
            let _ = SetStdHandle(STD_INPUT_HANDLE, h);
            std::mem::forget(f);
        }
    }
}
