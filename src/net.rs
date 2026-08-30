//! 端口占用 — Windows IP Helper 原生 API（GetExtendedTcpTable / GetExtendedUdpTable），
//! 不 shell 调用 netstat，支持 TCP/UDP v4/v6 四张表。
//! 基础设施模块：当前无命令挂接，待用户逐个需求启用。
#![allow(dead_code)]

use std::mem::size_of;
use std::ptr::read_unaligned;

use serde::Serialize;
use windows::Win32::Foundation::BOOL;
use windows::Win32::NetworkManagement::IpHelper::{
    GetExtendedTcpTable, GetExtendedUdpTable, MIB_TCP6ROW_OWNER_PID, MIB_TCPROW_OWNER_PID,
    MIB_UDP6ROW_OWNER_PID, MIB_UDPROW_OWNER_PID, TCP_TABLE_OWNER_PID_ALL, UDP_TABLE_OWNER_PID,
};

// winsock2 的 AF_INET / AF_INET6
const AF_INET: u32 = 2;
const AF_INET6: u32 = 23;

const ERROR_INSUFFICIENT_BUFFER: u32 = 122;

#[derive(Clone, Serialize)]
pub struct PortRow {
    pub port: u16,
    pub proto: &'static str,
    pub pid: u32,
    /// TCP: MIB_TCP_STATE（2=监听 5=已建立 11=等待关闭…）；UDP 恒为 0
    pub state: u8,
    /// 远程地址 "ip:port"（仅已建立的 TCP 连接；监听 / UDP 为空串）
    pub remote: String,
}

/// 收集全部 TCP/UDP（v4+v6）监听/连接端口 → PID 映射，跳过端口 0
pub fn all_rows() -> Vec<PortRow> {
    let mut rows = Vec::new();
    rows.extend(tcp_rows(AF_INET, parse_tcp4));
    rows.extend(tcp_rows(AF_INET6, parse_tcp6));
    rows.extend(udp_rows(AF_INET, parse_udp4));
    rows.extend(udp_rows(AF_INET6, parse_udp6));
    rows.retain(|r| r.port != 0);
    rows
}

/// 按端口过滤
pub fn rows_by_port(port: u16) -> Vec<PortRow> {
    all_rows().into_iter().filter(|r| r.port == port).collect()
}

/// 按 PID 过滤
pub fn rows_by_pid(pid: u32) -> Vec<PortRow> {
    all_rows().into_iter().filter(|r| r.pid == pid).collect()
}

// ── 底层表获取与解析 ─────────────────────────────────

/// 表查询最大重试次数（两次查询之间表可能增长，缓冲区不够时按新尺寸重取）
const TABLE_RETRIES: usize = 3;

/// 通用模式：先查尺寸再取数据，缓冲区不足时按更新后的尺寸重试
/// `get` 闭包封装 GetExtendedTcpTable / GetExtendedUdpTable 的调用，返回 WIN32_ERROR 码。
fn fetch_table<F>(mut get: F, parse: fn(&[u8]) -> Vec<PortRow>) -> Vec<PortRow>
where
    F: FnMut(*mut core::ffi::c_void, &mut u32) -> u32,
{
    let mut size: u32 = 0;
    get(std::ptr::null_mut(), &mut size);
    if size == 0 {
        return Vec::new();
    }
    for _ in 0..TABLE_RETRIES {
        let mut buf = vec![0u8; size as usize];
        let ret = get(buf.as_mut_ptr() as *mut core::ffi::c_void, &mut size);
        if ret == 0 {
            return parse(&buf);
        }
        // ret == ERROR_INSUFFICIENT_BUFFER 时 size 已被更新为所需大小，继续重试；
        // 其他错误直接放弃（size 归零防止死循环）
        if ret != ERROR_INSUFFICIENT_BUFFER || size == 0 {
            break;
        }
    }
    Vec::new()
}

fn tcp_rows(af: u32, parse: fn(&[u8]) -> Vec<PortRow>) -> Vec<PortRow> {
    fetch_table(
        |buf, size| unsafe {
            GetExtendedTcpTable(
                Option::<*mut core::ffi::c_void>::from(buf),
                size,
                BOOL(0),
                af,
                TCP_TABLE_OWNER_PID_ALL,
                0,
            )
        },
        parse,
    )
}

fn udp_rows(af: u32, parse: fn(&[u8]) -> Vec<PortRow>) -> Vec<PortRow> {
    fetch_table(
        |buf, size| unsafe {
            GetExtendedUdpTable(
                Option::<*mut core::ffi::c_void>::from(buf),
                size,
                BOOL(0),
                af,
                UDP_TABLE_OWNER_PID,
                0,
            )
        },
        parse,
    )
}

/// 网络字节序端口（dwLocalPort 高 16 位为真实端口）
fn be_port(p: u32) -> u16 {
    u16::from_be((p & 0xFFFF) as u16)
}

/// IPv4 地址（网络字节序 u32）→ "a.b.c.d"；全 0 表示监听，返回空串
fn fmt_remote4(addr: u32, port: u32) -> String {
    if addr == 0 {
        return String::new();
    }
    format!(
        "{}.{}.{}.{}:{}",
        (addr >> 24) & 0xFF,
        (addr >> 16) & 0xFF,
        (addr >> 8) & 0xFF,
        addr & 0xFF,
        be_port(port),
    )
}

/// IPv6 地址（网络字节序 16 字节）→ "xxxx:xxxx:...:xxxx:port"；全 0 表示监听，返回空串
fn fmt_remote6(addr: &[u8; 16], port: u32) -> String {
    if addr.iter().all(|&b| b == 0) {
        return String::new();
    }
    let groups: Vec<String> = addr
        .chunks(2)
        .map(|g| format!("{:x}", u16::from_be_bytes([g[0], g[1]])))
        .collect();
    format!("{}:{}", groups.join(":"), be_port(port))
}

fn parse_tcp4(buf: &[u8]) -> Vec<PortRow> {
    let count = count_of(buf);
    let row_size = size_of::<MIB_TCPROW_OWNER_PID>();
    let mut rows = Vec::new();
    for i in 0..count {
        let off = 4 + i * row_size;
        if off + row_size > buf.len() {
            break;
        }
        let row: MIB_TCPROW_OWNER_PID = unsafe { read_unaligned(buf.as_ptr().add(off) as *const MIB_TCPROW_OWNER_PID) };
        rows.push(PortRow {
            port: be_port(row.dwLocalPort),
            proto: "TCP4",
            pid: row.dwOwningPid,
            state: row.dwState as u8,
            remote: fmt_remote4(row.dwRemoteAddr, row.dwRemotePort),
        });
    }
    rows
}

fn parse_tcp6(buf: &[u8]) -> Vec<PortRow> {
    let count = count_of(buf);
    let row_size = size_of::<MIB_TCP6ROW_OWNER_PID>();
    let mut rows = Vec::new();
    for i in 0..count {
        let off = 4 + i * row_size;
        if off + row_size > buf.len() {
            break;
        }
        let row: MIB_TCP6ROW_OWNER_PID = unsafe { read_unaligned(buf.as_ptr().add(off) as *const MIB_TCP6ROW_OWNER_PID) };
        rows.push(PortRow {
            port: be_port(row.dwLocalPort),
            proto: "TCP6",
            pid: row.dwOwningPid,
            state: row.dwState as u8,
            remote: fmt_remote6(&row.ucRemoteAddr, row.dwRemotePort),
        });
    }
    rows
}

fn parse_udp4(buf: &[u8]) -> Vec<PortRow> {
    let count = count_of(buf);
    let row_size = size_of::<MIB_UDPROW_OWNER_PID>();
    let mut rows = Vec::new();
    for i in 0..count {
        let off = 4 + i * row_size;
        if off + row_size > buf.len() {
            break;
        }
        let row: MIB_UDPROW_OWNER_PID = unsafe { read_unaligned(buf.as_ptr().add(off) as *const MIB_UDPROW_OWNER_PID) };
        rows.push(PortRow { port: be_port(row.dwLocalPort), proto: "UDP4", pid: row.dwOwningPid, state: 0, remote: String::new() });
    }
    rows
}

fn parse_udp6(buf: &[u8]) -> Vec<PortRow> {
    let count = count_of(buf);
    let row_size = size_of::<MIB_UDP6ROW_OWNER_PID>();
    let mut rows = Vec::new();
    for i in 0..count {
        let off = 4 + i * row_size;
        if off + row_size > buf.len() {
            break;
        }
        let row: MIB_UDP6ROW_OWNER_PID = unsafe { read_unaligned(buf.as_ptr().add(off) as *const MIB_UDP6ROW_OWNER_PID) };
        rows.push(PortRow { port: be_port(row.dwLocalPort), proto: "UDP6", pid: row.dwOwningPid, state: 0, remote: String::new() });
    }
    rows
}

fn count_of(buf: &[u8]) -> usize {
    if buf.len() < 4 {
        return 0;
    }
    u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize
}
