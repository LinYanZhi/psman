//! psman GUI 后端 — Tauri 命令层，复用 psman 数据层（proc/net/file）。
//! 列表刷新只取轻量字段（避免全量读路径/命令行拖慢），详情按需取。

use std::collections::HashMap;
use std::sync::Mutex;

use serde::Serialize;
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, WindowEvent, Wry,
};

use psman::{file, handles, net, proc, services, startup};

mod icon;

/// 托盘"开机自启"菜单项引用（tauri 2 TrayIcon 无 menu getter，用 state 保存后动态切勾选态）
struct AutostartItem(Mutex<Option<CheckMenuItem<Wry>>>);

// ── DTO ─────────────────────────────────────────────

/// 进程列表行（轻量，不含命令行，保证千级进程刷新速度）
#[derive(Serialize)]
struct ProcInfo {
    pid: u32,
    name: String,
    mem: u64,
    threads: usize,
    status: String,
    start: u64,
    ports: Vec<u16>,
    parent: u32,
    /// exe 路径：图标懒加载 + 右键菜单"打开文件位置"用
    exe: String,
    /// CPU 使用率（%）——复用全局 System 连续采样，首次为 0，之后为相邻采样均值
    cpu: f32,
}

/// 进程详情（点击行时按需查询）
#[derive(Serialize)]
struct ProcDetail {
    pid: u32,
    name: String,
    path: String,
    cmd: String,
    mem: u64,
    threads: usize,
    status: String,
    start: u64,
    ports: Vec<String>,
    parent: u32,
    children: Vec<u32>,
    /// 该进程全部套接字连接（结构化，前端网络段展示）
    conns: Vec<ConnInfo>,
    /// 该进程全部线程（TID/优先级）
    thread_list: Vec<proc::ThreadInfo>,
    /// 句柄总数（句柄快照计数，一次几十毫秒）
    handles: usize,
    /// 优先级类常量（GetPriorityClass）
    priority: u32,
}

/// 套接字连接行（进程详情内展示）
#[derive(Serialize)]
struct ConnInfo {
    port: u16,
    proto: String,
    state: String,
    /// 远程地址 "ip:port"（监听为空串）
    remote: String,
}

/// 端口行（含进程名联表）
#[derive(Serialize)]
struct PortInfo {
    port: u16,
    proto: String,
    state: String,
    pid: u32,
    name: String,
    /// 远程地址 "ip:port"（监听/UDP 为空串）
    remote: String,
}

/// 杀进程结果
#[derive(Serialize)]
struct KillResult {
    pid: u32,
    ok: bool,
    name: String,
    error: Option<String>,
}

// ── 命令 ─────────────────────────────────────────────

#[tauri::command]
async fn list_processes() -> Vec<ProcInfo> {
    // 重查询放 spawn_blocking，避免阻塞主线程（同步 command 在主线程跑会卡 UI）
    tauri::async_runtime::spawn_blocking(|| {
        // CPU 采样需复用同一 System 连续 refresh；同时避免每次新建快照的进程枚举开销
        let mut sys_guard = proc::global_system();
        if sys_guard.is_none() {
            *sys_guard = Some(sysinfo::System::new());
        }
        let sys = sys_guard.as_mut().unwrap();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

        let thread_counts = proc::thread_count_map();

        let mut port_map: HashMap<u32, Vec<u16>> = HashMap::new();
        for r in net::all_rows() {
            port_map.entry(r.pid).or_default().push(r.port);
        }

        let mut out: Vec<ProcInfo> = sys
            .processes()
            .iter()
            .map(|(p, pr)| {
                let pid = p.as_u32();
                let mut ports = port_map.get(&pid).cloned().unwrap_or_default();
                ports.sort_unstable();
                ports.dedup();
                ProcInfo {
                    pid,
                    name: proc::display_name(pr),
                    mem: pr.memory(),
                    threads: thread_counts.get(&pid).copied().unwrap_or(0),
                    status: status_text(&pr.status()).into(),
                    start: pr.start_time(),
                    ports,
                    parent: pr.parent().map(|pp| pp.as_u32()).unwrap_or(0),
                    exe: proc::exe_path(pid).unwrap_or_default(),
                    cpu: pr.cpu_usage(),
                }
            })
            .collect();
        out.sort_by_key(|p| p.pid);
        out
    })
    .await
    .unwrap_or_default()
}

#[tauri::command]
async fn get_process_detail(pid: u32) -> Option<ProcDetail> {
    tauri::async_runtime::spawn_blocking(move || {
        let snap = proc::Snapshot::new();
        let pr = snap.get(pid)?;
        let mut ports = net::rows_by_pid(pid);
        ports.sort_by_key(|r| r.port);
        let port_text = ports
            .iter()
            .map(|r| format!("{}/{}", r.port, if r.proto.starts_with("TCP") { "tcp" } else { "udp" }))
            .collect();
        let conns = ports
            .iter()
            .map(|r| ConnInfo {
                port: r.port,
                proto: r.proto.to_string(),
                state: tcp_state_text(r.state).into(),
                remote: r.remote.clone(),
            })
            .collect();
        let thread_list = proc::threads_of(pid);
        let children = snap.children_map().get(&pid).cloned().unwrap_or_default();
        let priority = proc::get_priority(pid).unwrap_or(0);
        Some(ProcDetail {
            pid,
            name: proc::display_name(pr),
            path: proc::exe_path(pid).unwrap_or_default(),
            cmd: proc::cmdline_of(pid).unwrap_or_default(),
            mem: pr.memory(),
            threads: thread_list.len(),
            status: status_text(&pr.status()).into(),
            start: pr.start_time(),
            ports: port_text,
            parent: pr.parent().map(|pp| pp.as_u32()).unwrap_or(0),
            children,
            conns,
            thread_list,
            handles: handles::handle_count_of(pid),
            priority,
        })
    })
    .await
    .unwrap_or(None)
}

#[tauri::command]
async fn query_ports() -> Vec<PortInfo> {
    tauri::async_runtime::spawn_blocking(|| {
        let snap = proc::Snapshot::new();
        let names = snap.pid_to_name();
        let mut rows = net::all_rows();
        rows.sort_by_key(|r| r.port);
        rows.into_iter()
            .map(|r| PortInfo {
                port: r.port,
                proto: r.proto.to_string(),
                state: tcp_state_text(r.state).into(),
                pid: r.pid,
                name: names.get(&r.pid).cloned().unwrap_or_default(),
                remote: r.remote,
            })
            .collect()
    })
    .await
    .unwrap_or_default()
}

#[tauri::command]
async fn file_holders(path: String) -> Result<Vec<file::FileHolder>, String> {
    tauri::async_runtime::spawn_blocking(move || file::holders_of(&path))
        .await
        .unwrap_or_else(|e| Err(format!("查询线程异常: {e}")))
}

/// 进程打开的文件列表（句柄枚举，按需调用；非提权时系统进程可能返回空）
#[tauri::command]
async fn process_open_files(pid: u32) -> Vec<handles::OpenFile> {
    tauri::async_runtime::spawn_blocking(move || handles::open_files_of(pid))
        .await
        .unwrap_or_default()
}

#[tauri::command]
fn kill_processes(pids: Vec<u32>) -> Vec<KillResult> {
    let self_pid = std::process::id();
    let snap = proc::Snapshot::new();
    let mut out = Vec::new();
    for pid in pids {
        if pid == self_pid {
            out.push(KillResult {
                pid,
                ok: false,
                name: "psman 自身".into(),
                error: Some("不能结束自己".into()),
            });
            continue;
        }
        let name = snap
            .get(pid)
            .map(proc::display_name)
            .unwrap_or_else(|| "未知进程".into());
        let ok = proc::kill_pid(pid);
        out.push(KillResult {
            pid,
            ok,
            name,
            error: if ok { None } else { Some("无权限或已退出".into()) },
        });
    }
    out
}

/// 结束进程及其全部子孙进程，返回按终止顺序排列的 PID 列表
#[tauri::command]
fn kill_tree_process(pid: u32) -> Result<Vec<u32>, String> {
    if pid == std::process::id() {
        return Err("不能结束自己".into());
    }
    Ok(proc::kill_tree(pid))
}

// ── 服务 / 启动项 / 进程操作 ─────────────────────────

#[tauri::command]
async fn list_services() -> Vec<services::ServiceInfo> {
    tauri::async_runtime::spawn_blocking(services::all_services)
        .await
        .unwrap_or_default()
}

/// 服务启停：action ∈ {"start","stop","restart"}
#[tauri::command]
async fn service_action(name: String, action: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || match action.as_str() {
        "start" => services::start_service(&name),
        "stop" => services::stop_service(&name),
        "restart" => services::restart_service(&name),
        _ => Err("未知操作".into()),
    })
    .await
    .unwrap_or_else(|e| Err(format!("操作线程异常: {e}")))
}

#[tauri::command]
async fn list_startup_items() -> Vec<startup::StartupItem> {
    tauri::async_runtime::spawn_blocking(startup::all_items)
        .await
        .unwrap_or_default()
}

/// 启动项启用/禁用（写 StartupApproved 标记）
#[tauri::command]
async fn startup_set_enabled(source: String, name: String, enabled: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || startup::set_enabled(&source, &name, enabled))
        .await
        .unwrap_or_else(|e| Err(format!("操作线程异常: {e}")))
}

/// 设置进程优先级类（class 为 IDLE/BELOW_NORMAL/NORMAL/ABOVE_NORMAL/HIGH 常量）
#[tauri::command]
async fn process_priority(pid: u32, class: u32) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || proc::set_priority(pid, class))
        .await
        .unwrap_or_else(|e| Err(format!("操作线程异常: {e}")))
}

#[tauri::command]
async fn process_suspend(pid: u32) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || proc::suspend_pid(pid))
        .await
        .unwrap_or_else(|e| Err(format!("操作线程异常: {e}")))
}

#[tauri::command]
async fn process_resume(pid: u32) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || proc::resume_pid(pid))
        .await
        .unwrap_or_else(|e| Err(format!("操作线程异常: {e}")))
}

/// 以管理员权限重启自身（ShellExecute runas），成功后当前实例退出
#[tauri::command]
fn relaunch_as_admin(app: tauri::AppHandle) -> Result<(), String> {
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let exe = std::env::current_exe().map_err(|e| format!("获取自身路径失败: {e}"))?;
    let exe_w: Vec<u16> = exe.to_string_lossy().encode_utf16().chain(std::iter::once(0)).collect();
    let res = unsafe {
        ShellExecuteW(
            None,
            windows::core::w!("runas"),
            PCWSTR(exe_w.as_ptr()),
            None,
            None,
            SW_SHOWNORMAL,
        )
    };
    // 返回值 > 32 表示成功启动
    if res.0 as isize <= 32 {
        return Err("提权启动失败（用户可能取消了 UAC 确认）".into());
    }
    // 提权实例已启动，退出当前普通权限实例
    app.exit(0);
    Ok(())
}

/// 系统总览：CPU 使用率 / 内存 / 磁盘（CPU 需两次采样间隔）
#[derive(Serialize, Default)]
struct SystemOverview {
    cpu: f32,
    mem_total: u64,
    mem_used: u64,
    disk_total: u64,
    disk_free: u64,
}

#[tauri::command]
async fn system_overview() -> SystemOverview {
    tauri::async_runtime::spawn_blocking(|| {
        use sysinfo::System;
        let mut sys = System::new();
        sys.refresh_cpu_usage();
        std::thread::sleep(std::time::Duration::from_millis(300));
        sys.refresh_cpu_usage();
        let cpu = sys.global_cpu_usage();
        sys.refresh_memory();
        let (disk_total, disk_free) = disk_usage();
        SystemOverview {
            cpu,
            mem_total: sys.total_memory(),
            mem_used: sys.used_memory(),
            disk_total,
            disk_free,
        }
    })
    .await
    .unwrap_or_default()
}

/// 各逻辑盘容量汇总（GetDiskFreeSpaceExW）
fn disk_usage() -> (u64, u64) {
    use windows::Win32::Storage::FileSystem::{GetDiskFreeSpaceExW, GetLogicalDrives};
    let mut total = 0u64;
    let mut free = 0u64;
    let drives = unsafe { GetLogicalDrives() };
    for i in 0..26u32 {
        if drives & (1 << i) == 0 {
            continue;
        }
        let letter = (b'A' + i as u8) as char;
        let root: Vec<u16> = format!("{letter}:\\")
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let mut t = 0u64;
        let mut f = 0u64;
        let ok = unsafe {
            GetDiskFreeSpaceExW(
                windows::core::PCWSTR(root.as_ptr()),
                None,
                Some(&mut t),
                Some(&mut f),
            )
            .is_ok()
        };
        if ok {
            total += t;
            free += f;
        }
    }
    (total, free)
}

/// 进程图标：提取 exe 图标编码为 base64 ICO data URL（前端懒加载 + 缓存）
#[tauri::command]
async fn process_icon(path: String) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || {
        icon::icon_ico_bytes(&path)
            .map(|b| format!("data:image/x-icon;base64,{}", base64_encode(&b)))
    })
    .await
    .unwrap_or(None)
}

/// 在资源管理器中选中该文件
#[tauri::command]
fn reveal_in_folder(path: String) -> Result<(), String> {
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let arg: Vec<u16> = format!("/select,\"{}\"", path)
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let res = unsafe {
        ShellExecuteW(
            None,
            windows::core::w!("open"),
            windows::core::w!("explorer.exe"),
            PCWSTR(arg.as_ptr()),
            None,
            SW_SHOWNORMAL,
        )
    };
    if res.0 as isize <= 32 {
        Err("打开资源管理器失败".into())
    } else {
        Ok(())
    }
}

/// 开机自启：HKCU\Software\Microsoft\Windows\CurrentVersion\Run 的 psman 值
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "psman";

fn open_run_key(write: bool) -> Option<windows::Win32::System::Registry::HKEY> {
    use windows::Win32::System::Registry::{
        RegOpenKeyExW, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE,
    };
    unsafe {
        let key: Vec<u16> = RUN_KEY.encode_utf16().chain(std::iter::once(0)).collect();
        let access = if write { KEY_SET_VALUE } else { KEY_QUERY_VALUE };
        let mut h = windows::Win32::System::Registry::HKEY::default();
        let res = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            windows::core::PCWSTR(key.as_ptr()),
            0,
            access,
            &mut h,
        );
        (res == windows::Win32::Foundation::ERROR_SUCCESS).then_some(h)
    }
}

#[tauri::command]
fn autostart_enabled() -> bool {
    let Some(h) = open_run_key(false) else {
        return false;
    };
    unsafe {
        use windows::Win32::System::Registry::RegQueryValueExW;
        let name: Vec<u16> = RUN_VALUE.encode_utf16().chain(std::iter::once(0)).collect();
        let mut len = 0u32;
        let res = RegQueryValueExW(
            h,
            windows::core::PCWSTR(name.as_ptr()),
            None,
            None,
            None,
            Some(&mut len),
        );
        let _ = windows::Win32::System::Registry::RegCloseKey(h);
        res == windows::Win32::Foundation::ERROR_SUCCESS
    }
}

#[tauri::command]
fn autostart_set(enabled: bool) -> Result<(), String> {
    let Some(h) = open_run_key(true) else {
        return Err("无法打开注册表 Run 键".into());
    };
    unsafe {
        use windows::Win32::System::Registry::{RegDeleteValueW, RegSetValueExW, REG_SZ};
        let name: Vec<u16> = RUN_VALUE.encode_utf16().chain(std::iter::once(0)).collect();
        let res = if enabled {
            let exe = std::env::current_exe().map_err(|e| format!("获取自身路径失败: {e}"))?;
            let mut data: Vec<u16> = exe
                .to_string_lossy()
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            let bytes: Vec<u8> = data.drain(..).flat_map(|c| c.to_le_bytes()).collect();
            RegSetValueExW(
                h,
                windows::core::PCWSTR(name.as_ptr()),
                0,
                REG_SZ,
                Some(&bytes),
            )
        } else {
            RegDeleteValueW(h, windows::core::PCWSTR(name.as_ptr()))
        };
        let _ = windows::Win32::System::Registry::RegCloseKey(h);
        if res == windows::Win32::Foundation::ERROR_SUCCESS {
            Ok(())
        } else {
            Err("设置开机自启失败".into())
        }
    }
}

/// 将面板弹出为独立 OS 窗口（PyCharm 式抽离）。label 为面板 id，title 为窗口标题。
/// 前端在 dock 布局中点击「弹出」时调用；窗口内渲染 `index.html?popout={label}`。
/// async：避免同步命令在主线程创建 WebView2 阻塞主窗口 UI。
#[tauri::command]
async fn create_popout(app: tauri::AppHandle, label: String, title: String) -> Result<(), String> {
    use tauri::{WebviewUrl, WebviewWindowBuilder};

    let win_label = format!("popout-{label}");
    if app.get_webview_window(&win_label).is_some() {
        return Ok(()); // 已存在则幂等返回
    }
    let mut builder = WebviewWindowBuilder::new(
        &app,
        win_label,
        WebviewUrl::App(format!("index.html?popout={label}").into()),
    )
    .title(title)
    .inner_size(520.0, 700.0)
    .min_inner_size(340.0, 320.0)
    .center();
    // 默认放在主窗口右侧，避免与主窗口完全重叠
    if let Some(w) = app.get_webview_window("main") {
        if let (Ok(pos), Ok(size)) = (w.outer_position(), w.outer_size()) {
            builder =
                builder.position(pos.x as f64 + size.width as f64 + 16.0, pos.y as f64);
        }
    }
    builder.build().map_err(|e| e.to_string())?;
    Ok(())
}

/// 钉回主窗口：广播面板钉回事件并强制销毁弹出窗口。
/// 由弹出窗口的「钉回」按钮调用；窗口 X 关闭走 on_window_event（同样强制销毁）。
/// 用 destroy 而非 close，避免再次触发 CloseRequested 造成递归。
#[tauri::command]
async fn dock_back(app: tauri::AppHandle, label: String) {
    let _ = app.emit("psman:popout-closed", &label);
    if let Some(w) = app.get_webview_window(&format!("popout-{label}")) {
        let _ = w.destroy();
    }
}

/// 标准 base64 编码（避免为一个小函数引入 base64 crate）
fn base64_encode(data: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        out.push(T[(b[0] >> 2) as usize] as char);
        out.push(T[(((b[0] & 0x03) << 4) | (b[1] >> 4)) as usize] as char);
        out.push(if chunk.len() > 1 {
            T[(((b[1] & 0x0F) << 2) | (b[2] >> 6)) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[(b[2] & 0x3F) as usize] as char
        } else {
            '='
        });
    }
    out
}

// ── 状态文本 ─────────────────────────────────────────

/// MIB_TCP_STATE → 中文（UDP 无状态显示 -）
fn tcp_state_text(s: u8) -> &'static str {
    match s {
        1 => "关闭",
        2 => "监听",
        3 => "SYN_SENT",
        4 => "SYN_RCVD",
        5 => "已建立",
        6 => "FIN_WAIT1",
        7 => "FIN_WAIT2",
        8 => "等待关闭",
        9 => "CLOSING",
        10 => "LAST_ACK",
        11 => "等待关闭",
        12 => "删除TCB",
        _ => "-",
    }
}

fn status_text(s: &sysinfo::ProcessStatus) -> &'static str {
    use sysinfo::ProcessStatus::*;
    match s {
        Run => "运行中",
        Sleep | UninterruptibleDiskSleep => "睡眠",
        Idle => "空闲",
        Stop | Suspended => "已停止",
        Zombie => "僵尸",
        Tracing => "跟踪",
        Dead => "已死",
        Wakekill | Waking => "唤醒中",
        Parked => "驻留",
        LockBlocked => "锁阻塞",
        Unknown(_) => "未知",
    }
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // 系统托盘：关闭窗口隐藏到托盘常驻；菜单含显示/开机自启/退出；左键单击恢复窗口
            let show = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
            let autostart = CheckMenuItem::with_id(
                app,
                "autostart",
                "开机自启",
                true,
                autostart_enabled(),
                None::<&str>,
            )?;
            let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &autostart, &quit])?;
            app.manage(AutostartItem(Mutex::new(Some(autostart))));

            TrayIconBuilder::with_id("main")
                .tooltip("psman 进程管理")
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        // 显示主窗口 + 全部弹出窗口
                        for (_, w) in app.webview_windows() {
                            let _ = w.show();
                        }
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.set_focus();
                        }
                    }
                    "autostart" => {
                        // 勾选切换：取反后写注册表，并同步托盘菜单勾选状态
                        let next = !autostart_enabled();
                        if let Err(e) = autostart_set(next) {
                            let _ = app.emit("autostart-error", e);
                        } else if let Ok(guard) = app.state::<AutostartItem>().0.lock() {
                            if let Some(item) = guard.as_ref() {
                                let _ = item.set_checked(next);
                            }
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        // 左键单击：切换窗口显隐（主窗口 + 弹出窗口一起）
                        let app = tray.app_handle();
                        if let Some(w) = app.get_webview_window("main") {
                            match w.is_visible() {
                                Ok(true) => {
                                    for (_, w) in app.webview_windows() {
                                        let _ = w.hide();
                                    }
                                }
                                _ => {
                                    for (_, w) in app.webview_windows() {
                                        let _ = w.show();
                                    }
                                    let _ = w.set_focus();
                                }
                            }
                        }
                    }
                })
                .build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            let label = window.label();
            // 弹出窗口：X 关闭 → 强制钉回流程（通知主窗口把面板放回 dock + 销毁本窗口）。
            // 全部在 Rust 侧完成，不依赖前端 JS / IPC 权限，保证窗口永远可关。
            if label.starts_with("popout-") {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.emit("psman:popout-closed", label.to_string());
                    let _ = window.destroy();
                }
                return;
            }
            // 仅主窗口：关闭按钮 → 隐藏全部窗口到托盘，不退出进程（弹出窗口直接关掉即可）
            if label != "main" {
                return;
            }
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                for (_, w) in window.app_handle().webview_windows() {
                    let _ = w.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            list_processes,
            get_process_detail,
            query_ports,
            file_holders,
            process_open_files,
            kill_processes,
            kill_tree_process,
            relaunch_as_admin,
            system_overview,
            process_icon,
            reveal_in_folder,
            autostart_enabled,
            autostart_set,
            create_popout,
            dock_back,
            list_services,
            service_action,
            list_startup_items,
            startup_set_enabled,
            process_priority,
            process_suspend,
            process_resume,
        ])
        .run(tauri::generate_context!())
        .expect("error while running psman");
}
