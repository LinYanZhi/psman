//! psman 核心数据层 — 与 UI 无关，CLI 与 Tauri GUI 共用
//!
//! - proc.rs  进程数据层（sysinfo 枚举 + ToolHelp 线程 + TerminateProcess + 路径/命令行读取）
//! - net.rs   端口表（IP Helper 原生 API，不调 netstat）
//! - file.rs  文件占用（Restart Manager，路径 → 占用进程）
//! - handles.rs 进程句柄枚举（NtQuerySystemInformation + NtQueryObject，进程 → 打开的文件）

pub mod file;
pub mod handles;
pub mod net;
pub mod proc;
pub mod services;
pub mod startup;
