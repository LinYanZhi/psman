//! 进程数据层 — sysinfo 枚举/详情/进程树 + ToolHelp 线程枚举 + TerminateProcess 终止
//! 基础设施模块：当前无命令挂接，待用户逐个需求启用。
#![allow(dead_code)]

use std::collections::HashMap;
use std::ffi::c_void;
use std::mem::size_of;
use std::ptr::null_mut;

use serde::Serialize;
use sysinfo::{Pid, ProcessesToUpdate, System};
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Diagnostics::Debug::ReadProcessMemory;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32,
};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, TerminateProcess, PROCESS_ACCESS_RIGHTS,
    PROCESS_CREATION_FLAGS, PROCESS_NAME_WIN32, PROCESS_QUERY_INFORMATION,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE, PROCESS_VM_READ,
};
use windows::Win32::Foundation::HANDLE;

// windows 0.58 已移除 NtQueryInformationProcess，手动链接 ntdll（ProcessBasicInformation = 0）
#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtQueryInformationProcess(
        process: HANDLE,
        class: u32,
        info: *mut c_void,
        len: u32,
        ret_len: *mut u32,
    ) -> i32;
}

/// PROCESS_BASIC_INFORMATION（仅需 PebBaseAddress 字段，自行定义避免依赖 crate 裁剪）
#[repr(C)]
struct ProcessBasicInfo {
    _reserved1: *mut c_void,
    pub peb_base: *mut c_void,
    _reserved2: [*mut c_void; 2],
    _unique_pid: usize,
    _inherited: *mut c_void,
}

impl Default for ProcessBasicInfo {
    fn default() -> Self {
        Self {
            _reserved1: null_mut(),
            peb_base: null_mut(),
            _reserved2: [null_mut(); 2],
            _unique_pid: 0,
            _inherited: null_mut(),
        }
    }
}

/// 进程快照：创建后保持同一个 System 实例，重复刷新性能更好
pub struct Snapshot {
    pub sys: System,
}

impl Snapshot {
    pub fn new() -> Self {
        let mut sys = System::new();
        sys.refresh_processes(ProcessesToUpdate::All, true);
        Self { sys }
    }

    pub fn get(&self, pid: u32) -> Option<&sysinfo::Process> {
        self.sys.process(Pid::from_u32(pid))
    }

    /// PID → 进程名 映射（端口表等场景联表用）
    pub fn pid_to_name(&self) -> HashMap<u32, String> {
        self.sys
            .processes()
            .iter()
            .map(|(p, pr)| (p.as_u32(), pr.name().to_string_lossy().to_string()))
            .collect()
    }

    /// 构建 父 PID → 子 PID 列表 映射
    pub fn children_map(&self) -> HashMap<u32, Vec<u32>> {
        let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
        for (p, pr) in self.sys.processes() {
            if let Some(parent) = pr.parent() {
                children.entry(parent.as_u32()).or_default().push(p.as_u32());
            }
        }
        children
    }
}

/// 全局 System 实例（进程列表连续轮询用）：sysinfo 的 CPU 使用率是"自上次
/// refresh 以来的平均"，必须复用同一 System 连续 refresh 才有真实值。
/// 首次调用时 cpu_usage 为 0，之后每次 refresh 更新为相邻两次采样的均值。
pub fn global_system() -> std::sync::MutexGuard<'static, Option<System>> {
    use std::sync::Mutex;
    static SYS: Mutex<Option<System>> = Mutex::new(None);
    SYS.lock().unwrap_or_else(|e| e.into_inner())
}

/// 进程名（取 exe 文件名或名称字段）
pub fn display_name(p: &sysinfo::Process) -> String {
    let n = p.name();
    if !n.is_empty() {
        n.to_string_lossy().to_string()
    } else {
        p.exe()
            .and_then(|e| e.file_name())
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| format!("<{}>", p.pid().as_u32()))
    }
}

// ── exe 完整路径与命令行（sysinfo 的 exe()/cmd() 在 Windows 不可靠，自实现） ──

/// 可执行文件完整路径（QueryFullProcessImageNameW，比 sysinfo exe() 准确）
pub fn exe_path(pid: u32) -> Option<String> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 2048];
        let mut size = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, windows::core::PWSTR(buf.as_mut_ptr()), &mut size)
            .is_ok();
        let _ = CloseHandle(h);
        if ok {
            Some(String::from_utf16_lossy(&buf[..size as usize]))
        } else {
            None
        }
    }
}

/// 启动命令行：优先 ProcessCommandLineInformation（NT 类 60，仅需 LIMITED 权限，
/// 可跨完整性读取提权进程、自动处理 WOW64 位差），失败时回退 PEB 直读。
pub fn cmdline_of(pid: u32) -> Option<String> {
    cmdline_native(pid).or_else(|| cmdline_peb(pid))
}

/// ProcessCommandLineInformation（Win10 TH2+ 提供；phnt 枚举值 = 60）
fn cmdline_native(pid: u32) -> Option<String> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        // 第一步：查所需缓冲区大小（类 60 返回 STATUS_INFO_LENGTH_MISMATCH 并给出 need）
        let mut need = 0u32;
        let st = NtQueryInformationProcess(h, 60, null_mut(), 0, &mut need);
        if st != -1_073_741_820 || need == 0 {
            let _ = CloseHandle(h);
            return None;
        }
        // 缓冲区 = UNICODE_STRING 头(16B) + 数据区；头内 Buffer 指向本缓冲区 16 偏移处
        let n = need.max(32) as usize;
        let mut buf = vec![0u16; n / 2 + 1];
        let head = buf.as_mut_ptr() as *mut u8;
        std::ptr::write_unaligned(head.add(2) as *mut u16, (n - 16).min(u16::MAX as usize) as u16);
        std::ptr::write_unaligned(head.add(8) as *mut u64, head.add(16) as u64);
        let mut ret = 0u32;
        let st = NtQueryInformationProcess(
            h,
            60,
            buf.as_mut_ptr() as *mut c_void,
            buf.len() as u32 * 2,
            &mut ret,
        );
        let _ = CloseHandle(h);
        if st < 0 {
            return None;
        }
        let len = std::ptr::read_unaligned(head as *const u16) as usize;
        if len == 0 {
            return None;
        }
        let end = (8 + len / 2).min(buf.len());
        Some(String::from_utf16_lossy(&buf[8..end]))
    }
}

/// 启动命令行（读目标进程 PEB 的 RtlUserProcessParameters->CommandLine）。
/// x64 偏移：PEB.ProcessParameters = 0x20，RTL_USER_PROCESS_PARAMETERS.CommandLine = 0x70；
/// x86 分别为 0x10 / 0x40。需要 PROCESS_VM_READ，跨完整性读取提权进程会失败。
fn cmdline_peb(pid: u32) -> Option<String> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid).ok()?;

        // 1. PEB 地址（ProcessBasicInformation = 0）
        let mut basic = ProcessBasicInfo::default();
        let status = NtQueryInformationProcess(
            h,
            0, // ProcessBasicInformation
            &mut basic as *mut _ as *mut c_void,
            size_of::<ProcessBasicInfo>() as u32,
            null_mut(),
        );
        if status < 0 || basic.peb_base.is_null() {
            let _ = CloseHandle(h);
            return None;
        }

        // 2. PEB → ProcessParameters
        let pp_off = if cfg!(target_pointer_width = "64") { 0x20usize } else { 0x10 };
        let mut pparams: *mut c_void = null_mut();
        let mut read = 0usize;
        let ok = ReadProcessMemory(
            h,
            basic.peb_base.add(pp_off),
            &mut pparams as *mut _ as *mut c_void,
            size_of::<*mut c_void>(),
            Some(&mut read as *mut usize),
        )
        .is_ok()
            && !pparams.is_null();
        if !ok {
            let _ = CloseHandle(h);
            return None;
        }

        // 3. ProcessParameters → CommandLine（UNICODE_STRING：Length(2) + Buffer(ptr)）
        let cl_off = if cfg!(target_pointer_width = "64") { 0x70usize } else { 0x40 };
        let buf_off = if cfg!(target_pointer_width = "64") { 8usize } else { 4 };
        let mut len16 = 0u16;
        let mut buf_ptr: *mut u16 = null_mut();
        let ok = ReadProcessMemory(
            h,
            pparams.add(cl_off),
            &mut len16 as *mut _ as *mut c_void,
            2,
            Some(&mut read as *mut usize),
        )
        .is_ok()
            && ReadProcessMemory(
                h,
                pparams.add(cl_off + buf_off),
                &mut buf_ptr as *mut _ as *mut c_void,
                size_of::<*mut u16>(),
                Some(&mut read as *mut usize),
            )
            .is_ok()
            && len16 > 0
            && !buf_ptr.is_null();
        if !ok {
            let _ = CloseHandle(h);
            return None;
        }

        // 4. 读命令行内容（UTF-16）
        let mut chars = vec![0u16; len16 as usize / 2];
        let ok = ReadProcessMemory(
            h,
            buf_ptr as *const c_void,
            chars.as_mut_ptr() as *mut c_void,
            len16 as usize,
            Some(&mut read as *mut usize),
        )
        .is_ok();
        let _ = CloseHandle(h);
        if ok {
            Some(String::from_utf16_lossy(&chars))
        } else {
            None
        }
    }
}

// ── 线程枚举（ToolHelp） ─────────────────────────────

#[derive(Clone, Copy, Serialize)]
pub struct ThreadInfo {
    pub tid: u32,
    pub base_prio: i32,
    pub delta_prio: i32,
}

/// 一次性线程快照，统计每个进程的线程数（比逐进程调 threads_of 高效）
pub fn thread_count_map() -> HashMap<u32, usize> {
    unsafe {
        let snap = match CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) {
            Ok(s) => s,
            Err(_) => return HashMap::new(),
        };
        let mut counts: HashMap<u32, usize> = HashMap::new();
        let mut e = THREADENTRY32 {
            dwSize: std::mem::size_of::<THREADENTRY32>() as u32,
            ..Default::default()
        };
        if Thread32First(snap, &mut e).is_ok() {
            loop {
                *counts.entry(e.th32OwnerProcessID).or_insert(0) += 1;
                if Thread32Next(snap, &mut e).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snap);
        counts
    }
}

/// 列出指定进程的所有线程
pub fn threads_of(pid: u32) -> Vec<ThreadInfo> {
    unsafe {
        let snap = match CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let mut out = Vec::new();
        let mut e = THREADENTRY32 {
            dwSize: std::mem::size_of::<THREADENTRY32>() as u32,
            ..Default::default()
        };
        if Thread32First(snap, &mut e).is_ok() {
            loop {
                if e.th32OwnerProcessID == pid {
                    out.push(ThreadInfo {
                        tid: e.th32ThreadID,
                        base_prio: e.tpBasePri,
                        delta_prio: e.tpDeltaPri,
                    });
                }
                if Thread32Next(snap, &mut e).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snap);
        out
    }
}

// ── 进程终止 ─────────────────────────────────────────

/// 强制终止单个进程（TerminateProcess），失败返回 false
pub fn kill_pid(pid: u32) -> bool {
    unsafe {
        match OpenProcess(PROCESS_TERMINATE, false, pid) {
            Ok(h) => {
                let ok = TerminateProcess(h, 1).is_ok();
                let _ = CloseHandle(h);
                ok
            }
            Err(_) => false,
        }
    }
}

// ── 优先级 / 暂停恢复 ────────────────────────────────

/// 设置进程优先级类（SetPriorityClass）。class 取 IDLE/BELOW_NORMAL/NORMAL/
/// ABOVE_NORMAL/HIGH 常量值；系统进程与跨完整性进程可能失败（需提权）。
pub fn set_priority(pid: u32, class: u32) -> Result<(), String> {
    use windows::Win32::System::Threading::SetPriorityClass;
    // PROCESS_SET_INFORMATION = 0x0200（windows 0.58 未暴露常量，用字面量构造）
    unsafe {
        let h = OpenProcess(PROCESS_ACCESS_RIGHTS(0x0200), false, pid)
            .map_err(|e| format!("打开进程失败：{e}"))?;
        let ok = SetPriorityClass(h, PROCESS_CREATION_FLAGS(class)).is_ok();
        let _ = CloseHandle(h);
        if ok {
            Ok(())
        } else {
            Err("设置优先级失败（可能需要管理员权限）".into())
        }
    }
}

/// 查询当前优先级类（GetPriorityClass）
pub fn get_priority(pid: u32) -> Result<u32, String> {
    use windows::Win32::System::Threading::GetPriorityClass;
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)
            .map_err(|e| format!("打开进程失败：{e}"))?;
        let c = GetPriorityClass(h);
        let _ = CloseHandle(h);
        Ok(c)
    }
}

// NtSuspendProcess / NtResumeProcess（ntdll 手动链接，同 NtQueryInformationProcess 模式）
#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtSuspendProcess(handle: HANDLE) -> i32;
    fn NtResumeProcess(handle: HANDLE) -> i32;
}

/// 暂停进程全部线程（NtSuspendProcess）。system(0)/idle(4) 等系统进程会失败。
pub fn suspend_pid(pid: u32) -> Result<(), String> {
    // PROCESS_SUSPEND_RESUME = 0x0800（windows 0.58 未暴露常量，用字面量构造）
    unsafe {
        let h = OpenProcess(PROCESS_ACCESS_RIGHTS(0x0800), false, pid)
            .map_err(|e| format!("打开进程失败：{e}"))?;
        let st = NtSuspendProcess(h);
        let _ = CloseHandle(h);
        if st >= 0 {
            Ok(())
        } else {
            Err(format!("暂停失败（NTSTATUS {st:#x}）"))
        }
    }
}

/// 恢复进程（NtResumeProcess）
pub fn resume_pid(pid: u32) -> Result<(), String> {
    unsafe {
        let h = OpenProcess(PROCESS_ACCESS_RIGHTS(0x0800), false, pid)
            .map_err(|e| format!("打开进程失败：{e}"))?;
        let st = NtResumeProcess(h);
        let _ = CloseHandle(h);
        if st >= 0 {
            Ok(())
        } else {
            Err(format!("恢复失败（NTSTATUS {st:#x}）"))
        }
    }
}

/// 终止进程及其全部子孙进程；返回按终止顺序排列的 PID 列表
pub fn kill_tree(pid: u32) -> Vec<u32> {
    let snap = Snapshot::new();
    let children = snap.children_map();

    fn collect(pid: u32, children: &HashMap<u32, Vec<u32>>, order: &mut Vec<u32>) {
        if let Some(cs) = children.get(&pid) {
            for &c in cs {
                collect(c, children, order);
            }
        }
        order.push(pid);
    }

    let mut order = Vec::new();
    collect(pid, &children, &mut order);

    // 先杀子孙，再杀自身
    for &p in order.iter().filter(|&&p| p != pid) {
        let _ = kill_pid(p);
    }
    let _ = kill_pid(pid);
    order
}
