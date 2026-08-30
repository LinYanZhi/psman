//! Windows 服务管理 — OpenSCManagerW + EnumServicesStatusExW 枚举（含所属 PID），
//! OpenServiceW + StartServiceW / ControlService 启停。
//! 服务面板命令：list_services / service_action。

use serde::Serialize;
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::System::Services::{
    CloseServiceHandle, ControlService, EnumServicesStatusExW, OpenSCManagerW, OpenServiceW,
    QueryServiceConfigW, QueryServiceStatus, StartServiceW, ENUM_SERVICE_STATUS_PROCESSW,
    QUERY_SERVICE_CONFIGW, SC_ENUM_PROCESS_INFO, SC_HANDLE, SC_MANAGER_CONNECT,
    SC_MANAGER_ENUMERATE_SERVICE, SERVICE_CONTROL_STOP, SERVICE_QUERY_CONFIG, SERVICE_QUERY_STATUS,
    SERVICE_START, SERVICE_STATE_ALL, SERVICE_STATUS, SERVICE_STOP, SERVICE_WIN32,
};

/// 服务列表行
#[derive(Serialize)]
pub struct ServiceInfo {
    pub name: String,
    pub display: String,
    /// 运行状态中文（运行中/已停止/启动中/停止中/已暂停…）
    pub state: String,
    /// 原始状态码（前端按钮可用性判断）
    pub state_code: u32,
    /// 启动类型中文（自动/手动/禁用…）
    pub start_type: String,
    /// 服务进程 PID（未运行 / 共享进程时为 0）
    pub pid: u32,
    /// 可执行文件路径
    pub path: String,
}

/// 枚举全部 Win32 服务（枚举 + 逐服务查询启动类型/路径，几百个量级毫秒内完成）
pub fn all_services() -> Vec<ServiceInfo> {
    let Some(mgr) = open_manager(SC_MANAGER_CONNECT | SC_MANAGER_ENUMERATE_SERVICE) else {
        return Vec::new();
    };
    let mut out = Vec::new();

    // EnumServicesStatusExW：先查所需缓冲大小（返回 ERROR_INSUFFICIENT_BUFFER），再取数据
    let mut need = 0u32;
    let mut returned = 0u32;
    let _ = unsafe {
        EnumServicesStatusExW(
            mgr,
            SC_ENUM_PROCESS_INFO,
            SERVICE_WIN32,
            SERVICE_STATE_ALL,
            None,
            &mut need,
            &mut returned,
            None,
            None,
        )
    };
    if need == 0 {
        let _ = unsafe { CloseServiceHandle(mgr) };
        return Vec::new();
    }
    let mut buf = vec![0u8; need as usize + 64];
    let ok = unsafe {
        EnumServicesStatusExW(
            mgr,
            SC_ENUM_PROCESS_INFO,
            SERVICE_WIN32,
            SERVICE_STATE_ALL,
            Some(&mut buf[..]),
            &mut need,
            &mut returned,
            None,
            None,
        )
        .is_ok()
    };
    if !ok {
        let _ = unsafe { CloseServiceHandle(mgr) };
        return Vec::new();
    }

    let row_size = std::mem::size_of::<ENUM_SERVICE_STATUS_PROCESSW>();
    for i in 0..returned as usize {
        let off = i * row_size;
        if off + row_size > buf.len() {
            break;
        }
        let row = unsafe {
            &*(buf.as_ptr().add(off) as *const ENUM_SERVICE_STATUS_PROCESSW)
        };
        let name = wstr(row.lpServiceName);
        let display = wstr(row.lpDisplayName);
        let st = &row.ServiceStatusProcess;
        let (start_type, path) = query_config(mgr, &name);
        out.push(ServiceInfo {
            name,
            display,
            state: state_text(st.dwCurrentState.0).into(),
            state_code: st.dwCurrentState.0,
            start_type,
            pid: st.dwProcessId,
            path,
        });
    }

    let _ = unsafe { CloseServiceHandle(mgr) };
    out
}

/// 启动服务
pub fn start_service(name: &str) -> Result<(), String> {
    let Some(mgr) = open_manager(SC_MANAGER_CONNECT) else {
        return Err("无法连接服务控制管理器".into());
    };
    let name_w = wide(name);
    let svc = unsafe { OpenServiceW(mgr, PCWSTR(name_w.as_ptr()), SERVICE_START) };
    let Ok(svc) = svc else {
        let _ = unsafe { CloseServiceHandle(mgr) };
        return Err("打开服务失败（可能不存在或无权限）".into());
    };
    let res = unsafe { StartServiceW(svc, None) };
    let _ = unsafe { CloseServiceHandle(svc) };
    let _ = unsafe { CloseServiceHandle(mgr) };
    res.map_err(|e| format!("启动失败：{e}"))?;
    Ok(())
}

/// 停止服务（请求 SERVICE_CONTROL_STOP，等待由 SCM 异步完成）
pub fn stop_service(name: &str) -> Result<(), String> {
    let Some(mgr) = open_manager(SC_MANAGER_CONNECT) else {
        return Err("无法连接服务控制管理器".into());
    };
    let name_w = wide(name);
    let svc = unsafe { OpenServiceW(mgr, PCWSTR(name_w.as_ptr()), SERVICE_STOP) };
    let Ok(svc) = svc else {
        let _ = unsafe { CloseServiceHandle(mgr) };
        return Err("打开服务失败（可能不存在或无权限）".into());
    };
    let mut status = SERVICE_STATUS::default();
    let res = unsafe { ControlService(svc, SERVICE_CONTROL_STOP, &mut status) };
    let _ = unsafe { CloseServiceHandle(svc) };
    let _ = unsafe { CloseServiceHandle(mgr) };
    res.map_err(|e| format!("停止失败：{e}"))?;
    Ok(())
}

/// 重启服务：停止（轮询等待真正停止，最多 30s）后立即重新启动。
/// 未运行的服务直接启动；过渡态（启动中/停止中）发停止可能被 SCM 拒绝，忽略失败继续等待。
pub fn restart_service(name: &str) -> Result<(), String> {
    let Some(mgr) = open_manager(SC_MANAGER_CONNECT) else {
        return Err("无法连接服务控制管理器".into());
    };
    let name_w = wide(name);
    let svc = unsafe {
        OpenServiceW(
            mgr,
            PCWSTR(name_w.as_ptr()),
            SERVICE_START | SERVICE_STOP | SERVICE_QUERY_STATUS,
        )
    };
    let Ok(svc) = svc else {
        let _ = unsafe { CloseServiceHandle(mgr) };
        return Err("打开服务失败（可能不存在或无权限）".into());
    };

    // 已停止（SERVICE_STOPPED=1）跳过停止，直接启动
    let mut status = SERVICE_STATUS::default();
    if unsafe { QueryServiceStatus(svc, &mut status) }.is_err() {
        let _ = unsafe { CloseServiceHandle(svc) };
        let _ = unsafe { CloseServiceHandle(mgr) };
        return Err("查询服务状态失败".into());
    }
    if status.dwCurrentState.0 != 1 {
        let _ = unsafe { ControlService(svc, SERVICE_CONTROL_STOP, &mut status) };
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            let mut st = SERVICE_STATUS::default();
            if unsafe { QueryServiceStatus(svc, &mut st) }.is_ok() && st.dwCurrentState.0 == 1 {
                break;
            }
            if std::time::Instant::now() >= deadline {
                let _ = unsafe { CloseServiceHandle(svc) };
                let _ = unsafe { CloseServiceHandle(mgr) };
                return Err("等待服务停止超时".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
    }

    let res = unsafe { StartServiceW(svc, None) };
    let _ = unsafe { CloseServiceHandle(svc) };
    let _ = unsafe { CloseServiceHandle(mgr) };
    res.map_err(|e| format!("启动失败：{e}"))?;
    Ok(())
}

// ── 内部工具 ─────────────────────────────────────────

fn open_manager(access: u32) -> Option<SC_HANDLE> {
    unsafe { OpenSCManagerW(None, None, access).ok() }
}

/// 查询服务启动类型与可执行路径（QUERY_SERVICE_CONFIGW，先查尺寸再取数据）
fn query_config(mgr: SC_HANDLE, name: &str) -> (String, String) {
    let name_w = wide(name);
    let Ok(svc) = (unsafe { OpenServiceW(mgr, PCWSTR(name_w.as_ptr()), SERVICE_QUERY_CONFIG) }) else {
        return ("未知".into(), String::new());
    };
    let mut need = 0u32;
    let _ = unsafe {
        QueryServiceConfigW(
            svc,
            None,
            0,
            &mut need,
        )
    };
    if need == 0 {
        let _ = unsafe { CloseServiceHandle(svc) };
        return ("未知".into(), String::new());
    }
    let mut buf = vec![0u8; need as usize + 64];
    let ok = unsafe {
        QueryServiceConfigW(
            svc,
            Some(buf.as_mut_ptr() as *mut _),
            buf.len() as u32,
            &mut need,
        )
        .is_ok()
    };
    let _ = unsafe { CloseServiceHandle(svc) };
    if !ok {
        return ("未知".into(), String::new());
    }
    let cfg = unsafe { &*(buf.as_ptr() as *const QUERY_SERVICE_CONFIGW) };
    (start_type_text(cfg.dwStartType.0).into(), wstr(cfg.lpBinaryPathName))
}

/// PWSTR → String（空指针返回空串）
fn wstr(p: PWSTR) -> String {
    if p.is_null() {
        return String::new();
    }
    let len = unsafe { (0usize..).take_while(|&i| *p.0.add(i) != 0).count() };
    if len == 0 {
        return String::new();
    }
    String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(p.0, len) })
}

/// 字符串 → 以 NUL 结尾的 UTF-16 缓冲
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// SERVICE_STATE → 中文
fn state_text(s: u32) -> &'static str {
    match s {
        1 => "已停止",
        2 => "启动中",
        3 => "停止中",
        4 => "运行中",
        5 => "继续中",
        6 => "暂停中",
        7 => "已暂停",
        _ => "未知",
    }
}

/// SERVICE_START_TYPE → 中文
fn start_type_text(t: u32) -> &'static str {
    match t {
        0 => "启动加载",
        1 => "系统启动",
        2 => "自动",
        3 => "手动",
        4 => "禁用",
        _ => "未知",
    }
}
