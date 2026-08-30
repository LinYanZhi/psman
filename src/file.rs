//! 文件占用 — Restart Manager 原生 API（RmStartSession / RmRegisterResources / RmGetList）。
//! 查询指定文件/目录被哪些进程占用，供 GUI「文件占用」视图与解锁（结束占用进程）使用。
//! 不依赖 handle.exe 等外部工具。

use serde::Serialize;
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Storage::FileSystem::GetFullPathNameW;
use windows::Win32::System::RestartManager::{
    RmEndSession, RmGetList, RmRegisterResources, RmStartSession, RM_APP_TYPE, RM_PROCESS_INFO,
};

/// 一次可容纳的占用进程数上限（RmGetList 有 60 秒冷却，须一次拿全；
/// 256 对常规场景足够，超出时返回错误提示）
const MAX_HOLDERS: usize = 256;

const ERROR_MORE_DATA: u32 = 234; // 缓冲区不够

/// 单个占用者
#[derive(Debug, Clone, Serialize)]
pub struct FileHolder {
    pub pid: u32,
    /// 进程 exe 名（strAppName，如 notepad.exe）；服务为空时回退 exe 名
    pub name: String,
    /// 应用类型（1=主窗口 2=其他窗口 3=服务 4=资源管理器 5=控制台 1000=系统关键）
    pub app_type: u32,
    /// 当前状态（1=运行中 2=已停止 8=已重启 16=停止出错 128=未找到 …）
    pub status: u32,
    /// 是否允许重启（bRestartable）
    pub restartable: bool,
}

impl FileHolder {
    pub fn app_type_text(&self) -> String {
        match self.app_type {
            0 => "未知".into(),
            1 => "主窗口".into(),
            2 => "其他窗口".into(),
            3 => "服务".into(),
            4 => "资源管理器".into(),
            5 => "控制台".into(),
            1000 => "系统关键".into(),
            _ => format!("类型{}", self.app_type),
        }
    }

    pub fn status_text(&self) -> String {
        match self.status {
            1 => "运行中".into(),
            2 => "已停止".into(),
            4 => "已停止(其他)".into(),
            8 => "已重启".into(),
            16 => "停止出错".into(),
            32 => "重启出错".into(),
            64 => "关闭中".into(),
            128 => "未找到".into(),
            _ => "未知".into(),
        }
    }
}

/// 查询占用指定文件/目录的进程列表。
/// 成功返回占用者（可能为空 = 无进程占用）；失败返回中文错误描述。
pub fn holders_of(path: &str) -> Result<Vec<FileHolder>, String> {
    // 归一化为无 \\?\ 前缀的绝对路径（Restart Manager 用普通绝对路径）
    let full = full_path(path)?;

    let mut session: u32 = 0;
    let mut key = [0u16; 64];
    let start = unsafe { RmStartSession(&mut session, 0, PWSTR(key.as_mut_ptr())) };
    start
        .ok()
        .map_err(|e| format!("RmStartSession 失败: {e}"))?;

    let result = (|| -> Result<Vec<FileHolder>, String> {
        let path_w: Vec<u16> = full.encode_utf16().chain(std::iter::once(0)).collect();
        let files = [PCWSTR(path_w.as_ptr())];
        let reg = unsafe { RmRegisterResources(session, Some(&files), None, None) };
        reg.ok()
            .map_err(|e| format!("RmRegisterResources 失败（路径不可访问或已锁定）: {e}"))?;

        // RmGetList 有 60 秒冷却：一次分配足够大的缓冲区拿全，不先查尺寸再二次调用
        let mut apps: Vec<RM_PROCESS_INFO> = Vec::with_capacity(MAX_HOLDERS);
        apps.resize(MAX_HOLDERS, RM_PROCESS_INFO::default());
        let mut needed: u32 = 0;
        let mut count: u32 = 0;
        let mut reboot: u32 = 0;
        let res = unsafe {
            RmGetList(
                session,
                &mut needed,
                &mut count,
                Some(apps.as_mut_ptr()),
                &mut reboot,
            )
        };
        if !res.is_ok() {
            if res.0 as u32 == ERROR_MORE_DATA {
                return Err(format!("占用进程过多（>{} 个），无法完整列出", MAX_HOLDERS));
            }
            return Err(format!("RmGetList 失败: {}", res.0 as u32));
        }
        let n = (count as usize).min(MAX_HOLDERS);
        let mut out = Vec::with_capacity(n);
        for a in apps.iter().take(n) {
            out.push(FileHolder {
                pid: a.Process.dwProcessId,
                name: read_fixed_utf16(&a.strAppName),
                app_type: a.ApplicationType.0 as u32,
                status: a.AppStatus,
                restartable: a.bRestartable.as_bool(),
            });
        }
        Ok(out)
    })();

    let _ = unsafe { RmEndSession(session) };
    result
}

/// 相对路径/短路径 → 绝对路径（去掉 \\?\ 前缀，Restart Manager 用普通路径）
fn full_path(path: &str) -> Result<String, String> {
    let w: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    let mut buf = vec![0u16; 1024];
    let len = unsafe { GetFullPathNameW(PCWSTR(w.as_ptr()), Some(&mut buf), None) };
    if len == 0 || len as usize >= buf.len() {
        return Err(format!("无法解析路径: {path}"));
    }
    let s = String::from_utf16_lossy(&buf[..len as usize]);
    Ok(s.strip_prefix(r"\\?\").unwrap_or(&s).to_string())
}

/// 固定长度 UTF-16 数组 → String（去掉尾部 \0）
fn read_fixed_utf16<const N: usize>(arr: &[u16; N]) -> String {
    let end = arr.iter().position(|&c| c == 0).unwrap_or(N);
    String::from_utf16_lossy(&arr[..end]).trim().to_string()
}

#[allow(dead_code)]
fn _app_type(t: &RM_APP_TYPE) -> u32 {
    t.0 as u32
}
