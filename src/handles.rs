//! 进程句柄枚举 — 查询指定进程打开的文件对象（Process Explorer / handle.exe 同款机制）。
//! 流程：NtQuerySystemInformation(SystemExtendedHandleInformation) 一次快照全系统句柄表，
//! 按 PID 过滤 → 逐句柄 NtDuplicateObject + NtQueryObject 解析类型（仅保留 File 类型）→ 查文件名。
//! 文件名为内核设备路径（\Device\HarddiskVolumeN\...），用 QueryDosDeviceW 还原为盘符路径。
//! 注意：非提权时只能枚举同权限进程；「以管理员重启」后可看系统进程与其他用户进程。

use std::collections::HashMap;
use std::ffi::c_void;
use std::mem::size_of;

use serde::Serialize;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Storage::FileSystem::QueryDosDeviceW;
use windows::Win32::System::Threading::{OpenProcess, PROCESS_DUP_HANDLE};

// windows crate 未暴露的 ntdll API，手动链接（与 proc.rs 的 NtQueryInformationProcess 同模式）
#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtQuerySystemInformation(
        system_information_class: u32,
        system_information: *mut c_void,
        system_information_length: u32,
        return_length: *mut u32,
    ) -> i32;
    fn NtQueryObject(
        handle: HANDLE,
        object_information_class: u32,
        object_information: *mut c_void,
        object_information_length: u32,
        return_length: *mut u32,
    ) -> i32;
    fn NtDuplicateObject(
        source_process_handle: HANDLE,
        source_handle: HANDLE,
        target_process_handle: HANDLE,
        target_handle: *mut HANDLE,
        desired_access: u32,
        inherit_handle: u8,
        options: u32,
    ) -> i32;
}

// 信息类别
const SYSTEM_EXTENDED_HANDLE_INFORMATION: u32 = 64; // 句柄条目带完整 64 位 PID（class 16 的 PID 是 u8 会截断）
const OBJECT_TYPE_INFORMATION: u32 = 2; // 对象类型（结构体头部含类型名 UNICODE_STRING）
const OBJECT_NAME_INFORMATION: u32 = 1; // 对象名（结构体头部是 UNICODE_STRING）

// NTSTATUS 常量（i32 值，避免类型转换噪音）
const STATUS_INFO_LENGTH_MISMATCH: i32 = -1_073_741_820; // 0xC0000004
const STATUS_BUFFER_OVERFLOW: i32 = -2_147_483_643; // 0x80000005

// SystemExtendedHandleInformation 头部 = NumberOfHandles + Reserved 两个 usize
const HANDLE_INFO_HEADER: usize = size_of::<usize>() * 2;

/// 单条句柄条目（SYSTEM_HANDLE_TABLE_ENTRY_INFO_EX 内存布局）
#[derive(Clone, Copy)]
#[repr(C)]
struct HandleEntry {
    object: usize,
    unique_process_id: usize,
    handle_value: usize,
    granted_access: u32,
    creator_back_trace_index: u16,
    object_type_index: u16,
    handle_attributes: u32,
    reserved: u32,
}

/// UNICODE_STRING（仅取名称所需的前三个字段）
#[repr(C)]
struct UnicodeString {
    length: u16,
    max_len: u16,
    buffer: *mut u16,
}

/// 进程打开的一个文件（按路径去重，count = 该路径的句柄数）
#[derive(Debug, Clone, Serialize)]
pub struct OpenFile {
    pub path: String,
    pub count: u32,
}

/// 查询进程打开的（File 类型）文件列表，按路径字典序。
/// 权限不足或进程已退出时返回空列表。
pub fn open_files_of(pid: u32) -> Vec<OpenFile> {
    let _ = enable_debug_privilege(); // 尽力提权（管理员会话可看全量），失败静默
    let Ok(proc) = (unsafe { OpenProcess(PROCESS_DUP_HANDLE, false, pid) }) else {
        return Vec::new();
    };
    let entries = system_handle_snapshot();
    // 盘符 → NT 设备路径映射（每次查询构建一次，避免逐文件重复系统调用）
    let drives = drive_map();
    // 对象类型索引 → 类型名缓存（进程句柄涉及的类型有限，避免逐句柄查类型）
    let mut type_names: HashMap<u16, Option<String>> = HashMap::new();
    let mut seen: HashMap<String, u32> = HashMap::new();

    for e in entries.iter().filter(|e| e.unique_process_id as u32 == pid) {
        // 类型名：命中缓存直接用；未命中则复制该句柄查一次（失败不缓存、跳过此句柄）
        let Some(type_name) = (match type_names.get(&e.object_type_index) {
            Some(t) => t.clone(),
            None => match query_dup_type(proc, e.handle_value) {
                Some(t) => {
                    type_names.insert(e.object_type_index, Some(t.clone()));
                    Some(t)
                }
                None => continue,
            },
        }) else {
            continue;
        };
        if type_name != "File" {
            continue;
        }
        let Some(name) = query_dup_name(proc, e.handle_value) else { continue };
        let path = nt_to_win_path(&name, &drives);
        if path.is_empty() {
            continue;
        }
        *seen.entry(path).or_insert(0) += 1;
    }
    let _ = unsafe { CloseHandle(proc) };

    let mut out: Vec<OpenFile> = seen
        .into_iter()
        .map(|(path, count)| OpenFile { path, count })
        .collect();
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// 进程句柄总数（全系统句柄快照 + 按 PID 过滤计数，不解析对象名/路径）。
/// 详情面板展示用；权限不足时返回 0。
pub fn handle_count_of(pid: u32) -> usize {
    let _ = enable_debug_privilege();
    system_handle_snapshot()
        .iter()
        .filter(|e| e.unique_process_id as u32 == pid)
        .count()
}

/// 复制句柄到自身进程（NtCurrentProcess），查对象类型名
fn query_dup_type(proc: HANDLE, hv: usize) -> Option<String> {
    let dup = duplicate_handle(proc, hv)?;
    let buf = query_object(dup, OBJECT_TYPE_INFORMATION)?;
    let _ = unsafe { CloseHandle(dup) };
    read_unicode(&buf)
}

/// 复制句柄到自身进程，查对象名（\Device\... 内核路径）
fn query_dup_name(proc: HANDLE, hv: usize) -> Option<String> {
    let dup = duplicate_handle(proc, hv)?;
    let buf = query_object(dup, OBJECT_NAME_INFORMATION)?;
    let _ = unsafe { CloseHandle(dup) };
    read_unicode(&buf)
}

/// 把目标进程的句柄复制进自身进程（句柄可能恰在此刻被关闭，失败即 None）
fn duplicate_handle(proc: HANDLE, hv: usize) -> Option<HANDLE> {
    unsafe {
        let mut dup = HANDLE::default();
        let st = NtDuplicateObject(
            proc,
            HANDLE(hv as *mut c_void),
            HANDLE(-1isize as *mut c_void), // NtCurrentProcess
            &mut dup,
            0,
            0,
            0,
        );
        (st >= 0).then_some(dup)
    }
}

/// 查询对象信息（类型/名称），缓冲区不足时按返回长度扩容重试
fn query_object(handle: HANDLE, class: u32) -> Option<Vec<u8>> {
    let mut buf = vec![0u8; 512];
    loop {
        let mut ret: u32 = 0;
        let st = unsafe {
            NtQueryObject(
                handle,
                class,
                buf.as_mut_ptr() as *mut c_void,
                buf.len() as u32,
                &mut ret,
            )
        };
        if st >= 0 {
            return Some(buf);
        }
        if st != STATUS_INFO_LENGTH_MISMATCH && st != STATUS_BUFFER_OVERFLOW {
            return None;
        }
        if buf.len() > 65536 {
            return None; // 名字超 64KB 视为异常，放弃
        }
        let need = if ret > buf.len() as u32 {
            ret as usize
        } else {
            buf.len() + 512 // 返回长度不足时强制扩容，避免死循环
        };
        buf.resize(need, 0);
    }
}

/// 从对象信息缓冲区头部读 UNICODE_STRING 名称。
/// 内核把名字紧随结构体写在同一缓冲区内，防御性校验指针与长度避免越界。
fn read_unicode(buf: &[u8]) -> Option<String> {
    if buf.len() < size_of::<UnicodeString>() {
        return None;
    }
    let us = unsafe { &*(buf.as_ptr() as *const UnicodeString) };
    if us.buffer.is_null() || us.length == 0 {
        return None;
    }
    let n = (us.length as usize) / 2;
    if n == 0 || n > 32767 {
        return None;
    }
    let start = buf.as_ptr() as usize;
    let off = us.buffer as usize;
    if off < start || off + n * 2 > start + buf.len() {
        return None;
    }
    Some(String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(us.buffer, n) }))
}

/// 全系统句柄快照（SystemExtendedHandleInformation，一次调用，几十毫秒）
fn system_handle_snapshot() -> Vec<HandleEntry> {
    let mut buf: Vec<u8> = Vec::new();
    loop {
        let mut ret: u32 = 0;
        let st = unsafe {
            NtQuerySystemInformation(
                SYSTEM_EXTENDED_HANDLE_INFORMATION,
                buf.as_mut_ptr() as *mut c_void,
                buf.len() as u32,
                &mut ret,
            )
        };
        if st >= 0 {
            break;
        }
        if st != STATUS_INFO_LENGTH_MISMATCH || ret == 0 {
            return Vec::new();
        }
        buf.resize(ret as usize, 0);
    }
    if buf.len() < HANDLE_INFO_HEADER {
        return Vec::new();
    }
    unsafe {
        let n = *(buf.as_ptr() as *const usize); // NumberOfHandles
        let entries_ptr = (buf.as_ptr() as *const u8).add(HANDLE_INFO_HEADER) as *const HandleEntry;
        if buf.len() < HANDLE_INFO_HEADER + n * size_of::<HandleEntry>() {
            return Vec::new();
        }
        std::slice::from_raw_parts(entries_ptr, n).to_vec()
    }
}

/// 构建 盘符 → NT 设备路径 映射（如 C: → \Device\HarddiskVolume3）。
/// QueryDosDeviceW 返回的长度含结尾 NUL，需去除后再匹配。
fn drive_map() -> Vec<(String, String)> {
    let mut map = Vec::new();
    for letter in b'A'..=b'Z' {
        let letter = letter as char;
        let name: Vec<u16> = format!("{letter}:")
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let mut dev = [0u16; 256];
        let len = unsafe { QueryDosDeviceW(PCWSTR(name.as_ptr()), Some(&mut dev)) };
        if len == 0 {
            continue;
        }
        let mut dev_str = String::from_utf16_lossy(&dev[..len as usize]);
        while dev_str.ends_with('\0') {
            dev_str.pop();
        }
        if !dev_str.is_empty() {
            map.push((format!("{letter}:"), dev_str));
        }
    }
    map
}

/// NT 设备路径 → 盘符路径（\Device\HarddiskVolume3\a\b.txt → C:\a\b.txt）
fn nt_to_win_path(nt: &str, drives: &[(String, String)]) -> String {
    if let Some(rest) = nt.strip_prefix(r"\Device\NamedPipe\") {
        return format!(r"\\.\pipe\{rest}");
    }
    for (letter, dev) in drives {
        if let Some(rest) = nt.strip_prefix(dev.as_str()) {
            return format!("{letter}{rest}");
        }
    }
    nt.to_string()
}

/// 启用 SeDebugPrivilege（枚举其他用户/系统进程句柄所需；非管理员时失败静默）
fn enable_debug_privilege() -> bool {
    use windows::Win32::Foundation::LUID;
    use windows::Win32::Security::{
        AdjustTokenPrivileges, LookupPrivilegeValueW, TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES,
        TOKEN_QUERY, SE_PRIVILEGE_ENABLED,
    };
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
            &mut token,
        )
        .is_err()
        {
            return false;
        }
        let mut luid = LUID::default();
        // SeDebugPrivilege 是 C 宏常量，windows-rs 不生成，用字面量字符串
        let ok = LookupPrivilegeValueW(None, windows::core::w!("SeDebugPrivilege"), &mut luid).is_ok();
        if ok {
            let mut tp = TOKEN_PRIVILEGES::default();
            tp.PrivilegeCount = 1;
            tp.Privileges[0].Luid = luid;
            tp.Privileges[0].Attributes = SE_PRIVILEGE_ENABLED;
            let _ = AdjustTokenPrivileges(token, false, Some(&mut tp), 0, None, None);
        }
        let _ = CloseHandle(token);
        ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 冒烟测试：自身进程打开一个临时文件后必须能在句柄表里查到它，
    /// 且路径被还原为盘符形式（验证 ntdll 结构体布局与设备路径转换的真实可用性）。
    #[test]
    fn open_files_sees_own_handle() {
        let mut path = std::env::temp_dir();
        path.push("psman_handles_smoke.tmp");
        let file = std::fs::File::create(&path).expect("创建临时文件失败");
        let files = open_files_of(std::process::id());
        drop(file);
        let _ = std::fs::remove_file(&path);

        let hit = files
            .iter()
            .find(|f| f.path.ends_with("psman_handles_smoke.tmp"));
        assert!(
            hit.is_some(),
            "未在自身句柄中发现临时文件；实际文件列表：{files:#?}"
        );
        let hit = hit.unwrap();
        assert!(hit.count >= 1);
        let b = hit.path.as_bytes();
        assert!(
            b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':',
            "路径未还原为盘符形式：{}",
            hit.path
        );
    }
}
