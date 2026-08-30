//! 系统托盘：隐藏窗口 + 消息循环 + 右键菜单 + 看门狗
//! 左键单击：呼出/隐藏控制台；右键菜单：打开终端 / 退出

use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
use std::thread;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, CS_HREDRAW, DefWindowProcW, DestroyMenu,
    DispatchMessageW, GetCursorPos, GetMessageW, HCURSOR, IDI_APPLICATION, IDI_WINLOGO, LoadIconW,
    MF_SEPARATOR, MF_STRING, MSG, PostMessageW, RegisterClassExW, RegisterWindowMessageW,
    SetForegroundWindow, TrackPopupMenu, TranslateMessage, TPM_RIGHTBUTTON, WM_APP, WM_COMMAND,
    WM_NULL, WINDOW_EX_STYLE, WINDOW_STYLE, WNDCLASSEXW,
};

// ── 托盘状态 ──

pub static TRAY_WINDOW: AtomicIsize = AtomicIsize::new(0); // 托盘隐藏窗口句柄
static TASKBAR_CREATED_MSG: AtomicU32 = AtomicU32::new(0); // Explorer 重启广播消息

// ── 托盘消息/菜单 ID ──

pub const WM_TRAYICON: u32 = WM_APP + 1;
/// 新实例/看门狗请求重建托盘图标
pub const WM_TRAY_RESTORE: u32 = WM_APP + 2;

const ID_TRAY_OPEN: u16 = 1000;
const ID_TRAY_EXIT: u16 = 1001;

/// 启动托盘图标线程（隐藏窗口 + 消息循环）
pub fn spawn_tray() {
    thread::spawn(move || {
        unsafe {
            // COM 初始化（Shell_NotifyIconW 需要）
            use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

            let hinstance = match GetModuleHandleW(None) {
                Ok(h) => h,
                Err(_) => return,
            };
            let class_name = w!("psman_tray_window");

            // 载入嵌入的自定义图标（winres 默认资源 ID 为 1）
            let app_icon = LoadIconW(hinstance, PCWSTR(1 as *const u16)).unwrap_or_default();

            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: CS_HREDRAW,
                lpfnWndProc: Some(tray_wnd_proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: hinstance.into(),
                hIcon: app_icon,
                hCursor: HCURSOR::default(),
                lpszMenuName: PCWSTR::null(),
                lpszClassName: class_name,
                hIconSm: app_icon,
                ..Default::default()
            };
            if RegisterClassExW(&wc) == 0 {
                // 类已注册（此前托盘线程注册过）→ 复用，允许托盘线程重启
                if windows::Win32::Foundation::GetLastError()
                    != windows::Win32::Foundation::ERROR_CLASS_ALREADY_EXISTS
                {
                    eprintln!("注册托盘窗口类失败");
                    return;
                }
            }

            // 提前注册 Explorer 重启消息（任务栏重建后托盘图标被清空，据此自动重建）
            // 必须在创建窗口之前注册，避免 Explorer 恰好此时重启而漏掉广播
            TASKBAR_CREATED_MSG.store(RegisterWindowMessageW(w!("TaskbarCreated")), Ordering::Relaxed);

            let hwnd = match CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                class_name,
                w!("psman_tray"),
                WINDOW_STYLE::default(),
                0, 0, 0, 0, None, None, hinstance, None,
            ) {
                Ok(h) => h,
                Err(_) => {
                    eprintln!("创建托盘窗口失败");
                    return;
                }
            };
            // 保存窗口句柄，供看门狗/新实例通知
            TRAY_WINDOW.store(hwnd.0 as isize, Ordering::Relaxed);

            if add_tray_icon() {
                // 消息循环
                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                // 退出时删除托盘图标
                let mut del = NOTIFYICONDATAW::default();
                del.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
                del.hWnd = hwnd;
                del.uID = 1;
                let _ = Shell_NotifyIconW(NIM_DELETE, &del);
            } else {
                eprintln!("创建托盘图标失败");
            }
        }
    });
}

/// 托盘看门狗线程：每 5 秒检查托盘图标是否还在，
/// 丢失（Explorer 重启/异常清除）则通知托盘线程重建；
/// 托盘窗口已失效则自动重启托盘线程
pub fn spawn_tray_watchdog() {
    thread::spawn(move || {
        unsafe {
            use windows::Win32::UI::Shell::{Shell_NotifyIconGetRect, NOTIFYICONIDENTIFIER};
            use windows::Win32::UI::WindowsAndMessaging::IsWindow;

            loop {
                std::thread::sleep(std::time::Duration::from_secs(5));
                let hwnd = HWND(TRAY_WINDOW.load(Ordering::Relaxed) as *mut std::ffi::c_void);
                if hwnd.0.is_null() {
                    continue; // 托盘窗口尚未创建
                }
                if !IsWindow(hwnd).as_bool() {
                    // 托盘窗口已失效 → 托盘线程已退出，重启托盘线程
                    eprintln!("托盘窗口失效，自动重启托盘线程");
                    spawn_tray();
                    continue;
                }
                // 图标不在通知区域（Explorer 重启/异常清除）→ 请求托盘线程重建
                let nid = NOTIFYICONIDENTIFIER {
                    cbSize: std::mem::size_of::<NOTIFYICONIDENTIFIER>() as u32,
                    hWnd: hwnd,
                    uID: 1,
                    ..Default::default()
                };
                if Shell_NotifyIconGetRect(&nid).is_err() {
                    let _ = PostMessageW(hwnd, WM_TRAY_RESTORE, WPARAM(0), LPARAM(0));
                }
            }
        }
    });
}

/// 托盘隐藏窗口的窗口过程
unsafe extern "system" fn tray_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match msg {
            WM_TRAYICON => {
                match lparam.0 as u32 {
                    0x0202 => { // WM_LBUTTONUP — 切换控制台显隐
                        crate::console::open_or_toggle();
                        LRESULT(0)
                    }
                    0x0205 => { // WM_RBUTTONUP — 显示右键菜单
                        show_menu(hwnd);
                        LRESULT(0)
                    }
                    _ => DefWindowProcW(hwnd, msg, wparam, lparam),
                }
            }
            WM_COMMAND => {
                let id = wparam.0 as u32 & 0xFFFF;
                match id as u16 {
                    ID_TRAY_OPEN => {
                        crate::console::open_or_toggle();
                        LRESULT(0)
                    }
                    ID_TRAY_EXIT => {
                        // 退出：进程结束，控制台随之关闭
                        std::process::exit(0);
                    }
                    _ => DefWindowProcW(hwnd, msg, wparam, lparam),
                }
            }
            m if m == TASKBAR_CREATED_MSG.load(Ordering::Relaxed) => {
                // Explorer 重启 → 任务栏重建，托盘图标被清空，重新添加
                add_tray_icon();
                LRESULT(0)
            }
            WM_TRAY_RESTORE => {
                // 新实例/看门狗请求重建托盘图标
                add_tray_icon();
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

/// 弹出右键菜单（打开终端 / 退出）
unsafe fn show_menu(hwnd: HWND) {
    unsafe {
        let menu = match CreatePopupMenu() {
            Ok(m) => m,
            Err(_) => return,
        };
        let _ = AppendMenuW(menu, MF_STRING, ID_TRAY_OPEN as usize, w!("打开终端"));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
        let _ = AppendMenuW(menu, MF_STRING, ID_TRAY_EXIT as usize, w!("退出"));

        let _ = SetForegroundWindow(hwnd);
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let _ = TrackPopupMenu(menu, TPM_RIGHTBUTTON, pt.x, pt.y, 0, hwnd, None);
        // TrackPopupMenu 返回后菜单已关闭，句柄失效
        let _ = PostMessageW(hwnd, WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
    }
}

/// 添加/重建托盘图标（初始创建、TaskbarCreated、WM_TRAY_RESTORE 均复用此函数）
fn add_tray_icon() -> bool {
    unsafe {
        let hwnd = HWND(TRAY_WINDOW.load(Ordering::Relaxed) as *mut std::ffi::c_void);
        if hwnd.0.is_null() {
            return false;
        }
        let hinstance = GetModuleHandleW(None).unwrap_or_default();
        let app_icon = LoadIconW(hinstance, PCWSTR(1 as *const u16)).unwrap_or_default();
        let icon = if !app_icon.0.is_null() {
            app_icon
        } else {
            LoadIconW(None, IDI_WINLOGO)
                .or_else(|_| LoadIconW(None, IDI_APPLICATION))
                .unwrap_or_default()
        };

        let mut nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: 1,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
            uCallbackMessage: WM_TRAYICON,
            hIcon: icon,
            ..NOTIFYICONDATAW::default()
        };
        // 设置提示文字
        let tip = format!("psman v{} - 进程/端口/线程管理\0", env!("CARGO_PKG_VERSION"));
        for (i, c) in tip.encode_utf16().enumerate().take(127) {
            nid.szTip[i] = c;
        }

        Shell_NotifyIconW(NIM_ADD, &nid).as_bool()
    }
}
