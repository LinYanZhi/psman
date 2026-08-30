//! 启动项管理 — Run 注册表键（HKCU/HKLM/32 位视图）+ 启动文件夹枚举；
//! 禁用/启用写 Explorer\StartupApproved 标记（与任务管理器「启动」页同机制）。
//! 面板命令：list_startup_items / startup_set_enabled。

use serde::Serialize;
use windows::Win32::Foundation::{ERROR_SUCCESS, WIN32_ERROR};
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegEnumValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
    HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_BINARY,
    REG_OPEN_CREATE_OPTIONS, REG_VALUE_TYPE,
};

const RUN_SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const APPROVED_RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
const APPROVED_FOLDER: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\StartupFolder";

/// 启动项来源（决定读写哪个注册表键 / 文件夹与对应的 StartupApproved 键）
#[derive(Clone, Copy, PartialEq)]
enum Source {
    HkcuRun,
    HklmRun,
    HklmRun32,
    UserFolder,
    CommonFolder,
}

impl Source {
    fn label(self) -> &'static str {
        match self {
            Source::HkcuRun => "当前用户运行键",
            Source::HklmRun => "本机运行键",
            Source::HklmRun32 => "本机 32 位运行键",
            Source::UserFolder => "用户启动文件夹",
            Source::CommonFolder => "系统启动文件夹",
        }
    }

    /// 枚举主键路径（reg 项）或启动文件夹路径（folder 项）
    fn path(self) -> String {
        match self {
            Source::HkcuRun => format!("HKCU\\{RUN_SUBKEY}"),
            Source::HklmRun => format!("HKLM\\{RUN_SUBKEY}"),
            Source::HklmRun32 => format!("HKLM\\Software\\Wow6432Node\\{RUN_SUBKEY}"),
            Source::UserFolder => std::env::var_os("APPDATA")
                .map(|p| {
                    std::path::Path::new(&p)
                        .join(r"Microsoft\Windows\Start Menu\Programs\Startup")
                        .to_string_lossy()
                        .into_owned()
                })
                .unwrap_or_default(),
            Source::CommonFolder => std::env::var_os("PROGRAMDATA")
                .map(|p| {
                    std::path::Path::new(&p)
                        .join(r"Microsoft\Windows\Start Menu\Programs\Startup")
                        .to_string_lossy()
                        .into_owned()
                })
                .unwrap_or_default(),
        }
    }

    /// 注册表根键（HKCU/HKLM）
    fn hive(self) -> HKEY {
        match self {
            Source::HkcuRun | Source::UserFolder => HKEY_CURRENT_USER,
            _ => HKEY_LOCAL_MACHINE,
        }
    }

    fn registry_subkey(self) -> &'static str {
        match self {
            Source::HkcuRun | Source::HklmRun => RUN_SUBKEY,
            Source::HklmRun32 => r"Software\Wow6432Node\Microsoft\Windows\CurrentVersion\Run",
            Source::UserFolder => "",
            Source::CommonFolder => "",
        }
    }

    fn approved_subkey(self) -> &'static str {
        match self {
            Source::HkcuRun | Source::HklmRun => APPROVED_RUN,
            Source::HklmRun32 => r"Software\Wow6432Node\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run",
            Source::UserFolder | Source::CommonFolder => APPROVED_FOLDER,
        }
    }
}

fn all_sources() -> [Source; 5] {
    [
        Source::HkcuRun,
        Source::HklmRun,
        Source::HklmRun32,
        Source::UserFolder,
        Source::CommonFolder,
    ]
}

/// 启动项行
#[derive(Serialize)]
pub struct StartupItem {
    /// 值名（reg）或文件名（folder）
    pub name: String,
    /// 命令（folder 项为空）
    pub command: String,
    /// 来源中文标签
    pub source: String,
    /// "reg" | "folder"
    pub kind: &'static str,
    /// 是否启用（StartupApproved 判定）
    pub enabled: bool,
    /// 完整路径（folder 项为文件路径；reg 项为空）
    pub path: String,
}

/// 枚举全部启动项（5 个来源去重：同名同命令合并）
pub fn all_items() -> Vec<StartupItem> {
    let mut items: Vec<StartupItem> = Vec::new();
    for src in all_sources() {
        match src {
            Source::UserFolder | Source::CommonFolder => {
                let dir = src.path();
                if dir.is_empty() {
                    continue;
                }
                let Ok(entries) = std::fs::read_dir(&dir) else {
                    continue;
                };
                for e in entries.flatten() {
                    let p = e.path();
                    if p.extension().map(|x| x.to_string_lossy().to_lowercase()) != Some("lnk".into()) {
                        continue;
                    }
                    let name = e.file_name().to_string_lossy().into_owned();
                    items.push(StartupItem {
                        name: name.clone(),
                        command: String::new(),
                        source: src.label().into(),
                        kind: "folder",
                        enabled: approved_state(src, &name).unwrap_or(true),
                        path: p.to_string_lossy().into_owned(),
                    });
                }
            }
            _ => {
                let (hive, subkey) = (src.hive(), src.registry_subkey());
                let Some(key) = open_key(hive, subkey, false) else {
                    continue;
                };
                for (name, data) in enum_values(key) {
                    // Run 值默认是 REG_SZ（UTF-16LE 字节）→ 转字符串；二进制类型显示为空
                    let command = if data.is_empty() {
                        String::new()
                    } else {
                        let units: Vec<u16> = data
                            .chunks_exact(2)
                            .map(|c| u16::from_le_bytes([c[0], c[1]]))
                            .collect();
                        String::from_utf16_lossy(&units)
                    };
                    items.push(StartupItem {
                        name: name.clone(),
                        command,
                        source: src.label().into(),
                        kind: "reg",
                        enabled: approved_state(src, &name).unwrap_or(true),
                        path: String::new(),
                    });
                }
                let _ = close_key(key);
            }
        }
    }
    items
}

/// 设置启用/禁用（写 StartupApproved 标记；reg 项不动原 Run 值，folder 项不动文件）
/// source: 前端回传的来源标签；name: 值名/文件名
pub fn set_enabled(source: &str, name: &str, enabled: bool) -> Result<(), String> {
    let Some(src) = all_sources().into_iter().find(|s| s.label() == source) else {
        return Err(format!("未知来源：{source}"));
    };
    let (hive, subkey) = (src.hive(), src.approved_subkey());
    let Some(key) = open_key(hive, subkey, true) else {
        return Err("无法打开 StartupApproved 注册表键（可能需要管理员权限）".into());
    };
    let blob = if enabled {
        // 启用：02 00 00 00 + 8 个 0（与任务管理器写入一致）
        vec![2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
    } else {
        // 禁用：03 00 00 00 + 禁用时间戳 FILETIME（100ns 自 1601）
        let mut b = vec![3, 0, 0, 0];
        let ft = filetime_now();
        b.extend_from_slice(&ft.to_le_bytes());
        b
    };
    let name_w = wide(name);
    let res = unsafe {
        RegSetValueExW(
            key,
            windows::core::PCWSTR(name_w.as_ptr()),
            0,
            REG_BINARY,
            Some(&blob),
        )
    };
    let _ = close_key(key);
    if res == ERROR_SUCCESS {
        Ok(())
    } else {
        Err("写入 StartupApproved 失败".into())
    }
}

// ── 内部工具 ─────────────────────────────────────────

/// 读取 StartupApproved 状态：无记录视为启用；首字节 0x02 视为启用，否则禁用
fn approved_state(src: Source, name: &str) -> Option<bool> {
    let key = open_key(src.hive(), src.approved_subkey(), false)?;
    let name_w = wide(name);
    let mut len = 0u32;
    let mut kind = REG_VALUE_TYPE(0);
    let res = unsafe {
        RegQueryValueExW(
            key,
            windows::core::PCWSTR(name_w.as_ptr()),
            None,
            Some(&mut kind),
            None,
            Some(&mut len),
        )
    };
    if res != ERROR_SUCCESS || len == 0 {
        let _ = close_key(key);
        return None;
    }
    let mut buf = vec![0u8; len as usize];
    let res = unsafe {
        RegQueryValueExW(
            key,
            windows::core::PCWSTR(name_w.as_ptr()),
            None,
            None,
            Some(buf.as_mut_ptr() as *mut _),
            Some(&mut len),
        )
    };
    let _ = close_key(key);
    if res != ERROR_SUCCESS {
        return None;
    }
    Some(buf.first() == Some(&2))
}

/// 打开注册表键（不存在时创建可选）
fn open_key(hive: HKEY, subkey: &str, write: bool) -> Option<HKEY> {
    let sub = wide(subkey);
    let mut h = HKEY::default();
    let access = if write { KEY_SET_VALUE } else { KEY_QUERY_VALUE };
    let res = if write {
        unsafe {
            RegCreateKeyExW(
                hive,
                windows::core::PCWSTR(sub.as_ptr()),
                0,
                None,
                REG_OPEN_CREATE_OPTIONS(0),
                access,
                None,
                &mut h,
                None,
            )
        }
    } else {
        unsafe {
            RegOpenKeyExW(hive, windows::core::PCWSTR(sub.as_ptr()), 0, access, &mut h)
        }
    };
    (res == ERROR_SUCCESS).then_some(h)
}

fn close_key(h: HKEY) -> WIN32_ERROR {
    unsafe { RegCloseKey(h) }
}

/// 枚举键下全部值 → (名称, 数据字节)。REG_SZ 数据转为 UTF-16 字节便于统一处理。
fn enum_values(h: HKEY) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    let mut i = 0u32;
    loop {
        let mut name = [0u16; 1024];
        let mut name_len = name.len() as u32;
        let mut kind = 0u32;
        let mut data = vec![0u8; 8192];
        let mut data_len = data.len() as u32;
        let res = unsafe {
            RegEnumValueW(
                h,
                i,
                windows::core::PWSTR(name.as_mut_ptr()),
                &mut name_len,
                None,
                Some(&mut kind),
                Some(data.as_mut_ptr() as *mut _),
                Some(&mut data_len),
            )
        };
        if res != ERROR_SUCCESS {
            break;
        }
        i += 1;
        let n = String::from_utf16_lossy(&name[..name_len as usize]);
        data.truncate(data_len as usize);
        out.push((n, data));
    }
    out
}

/// 当前时间 → FILETIME（100ns 间隔，自 1601-01-01）用于禁用时间戳
fn filetime_now() -> u64 {
    let since_epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    // 1601→1970 差值（秒）× 10^7
    ((since_epoch.as_secs() as u128 + 11_644_473_600) * 10_000_000 + since_epoch.subsec_nanos() as u128 / 100) as u64
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
