//! 进程图标提取：从 exe 路径拿 HICON，编码为 ICO 文件字节
//! （前端 <img src="data:image/x-icon;base64,..."> 直接显示，无需 PNG 编码依赖）

use windows::core::PCWSTR;
use windows::Win32::Graphics::Gdi::{
    BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC,
    DeleteObject, GetDIBits, GetObjectW, HGDIOBJ,
};
use windows::Win32::UI::Shell::ExtractIconExW;
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};

/// 提取 exe 路径对应的小图标，编码为 ICO 文件字节
pub fn icon_ico_bytes(exe_path: &str) -> Option<Vec<u8>> {
    unsafe {
        let mut large = HICON::default();
        let mut small = HICON::default();
        let w: Vec<u16> = exe_path.encode_utf16().chain(std::iter::once(0)).collect();
        if ExtractIconExW(PCWSTR(w.as_ptr()), 0, Some(&mut large), Some(&mut small), 1) == 0 {
            return None;
        }
        let hicon = if !small.is_invalid() { small } else { large };
        let mut ii = ICONINFO::default();
        if !GetIconInfo(hicon, &mut ii).is_ok() {
            let _ = DestroyIcon(hicon);
            return None;
        }
        let bmp = ii.hbmColor;
        if bmp.is_invalid() {
            let _ = DestroyIcon(hicon);
            let _ = DeleteObject(HGDIOBJ(ii.hbmMask.0));
            return None;
        }
        let mut bm = BITMAP::default();
        if GetObjectW(bmp, std::mem::size_of::<BITMAP>() as i32, Some(&mut bm as *mut BITMAP as *mut std::ffi::c_void)) == 0 {
            let _ = DestroyIcon(hicon);
            let _ = DeleteObject(HGDIOBJ(ii.hbmColor.0));
            let _ = DeleteObject(HGDIOBJ(ii.hbmMask.0));
            return None;
        }
        let wpx = bm.bmWidth as u32;
        let hpx = bm.bmHeight as u32;
        if wpx == 0 || hpx == 0 {
            let _ = DestroyIcon(hicon);
            let _ = DeleteObject(HGDIOBJ(ii.hbmColor.0));
            let _ = DeleteObject(HGDIOBJ(ii.hbmMask.0));
            return None;
        }

        // 用 DIB section 读出 32bpp 像素（bottom-up）。
        // 注意：hbmColor 是 DDB，GetDIBits 做颜色转换必须传有效 HDC，空 HDC 会失败返回 0 行 → 全黑
        let mut dib_hdr = BITMAPINFOHEADER::default();
        dib_hdr.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        dib_hdr.biWidth = wpx as i32;
        dib_hdr.biHeight = hpx as i32;
        dib_hdr.biPlanes = 1;
        dib_hdr.biBitCount = 32;
        dib_hdr.biCompression = BI_RGB.0 as u32;
        let mut bmi = BITMAPINFO::default();
        bmi.bmiHeader = dib_hdr;
        let mut pixels = vec![0u8; (wpx * hpx * 4) as usize];
        let dc = CreateCompatibleDC(None);
        let lines = GetDIBits(
            dc,
            bmp,
            0,
            hpx,
            Some(pixels.as_mut_ptr().cast()),
            &mut bmi,
            DIB_RGB_COLORS,
        );
        let _ = DeleteDC(dc);
        if lines == 0 {
            let _ = DestroyIcon(hicon);
            let _ = DeleteObject(HGDIOBJ(ii.hbmColor.0));
            let _ = DeleteObject(HGDIOBJ(ii.hbmMask.0));
            return None;
        }

        let _ = DestroyIcon(hicon);
        let _ = DeleteObject(HGDIOBJ(ii.hbmColor.0));
        let _ = DeleteObject(HGDIOBJ(ii.hbmMask.0));

        // 保底：若 alpha 通道整体丢失（全 0）但 RGB 有内容，强制不透明，避免显示为透明黑块
        if pixels.chunks_exact(4).all(|px| px[3] == 0) && pixels.iter().any(|&b| b != 0) {
            for px in pixels.chunks_exact_mut(4) {
                px[3] = 255;
            }
        }

        // ── 拼 ICO：ICONDIR(6) + ICONDIRENTRY(16) + DIB(BITMAPINFOHEADER + XOR + AND) ──
        let and_stride = ((wpx + 31) / 32) * 4;
        let data_len = 40 + pixels.len() + (and_stride * hpx) as usize;
        let mut ico = Vec::with_capacity(22 + data_len);
        ico.extend_from_slice(&[0u8, 0, 1, 0, 1, 0]); // reserved / type=icon / count=1
        ico.push((wpx as u8).min(255));
        ico.push((hpx as u8).min(255));
        ico.push(0); // colors
        ico.push(0); // reserved
        ico.extend_from_slice(&1u16.to_le_bytes()); // planes
        ico.extend_from_slice(&32u16.to_le_bytes()); // bitcount
        ico.extend_from_slice(&(data_len as u32).to_le_bytes());
        ico.extend_from_slice(&22u32.to_le_bytes()); // 数据偏移
        // ICO 内嵌 DIB 头：biHeight = 2*h（XOR + AND）
        let mut ico_hdr = BITMAPINFOHEADER::default();
        ico_hdr.biSize = 40;
        ico_hdr.biWidth = wpx as i32;
        ico_hdr.biHeight = (hpx * 2) as i32;
        ico_hdr.biPlanes = 1;
        ico_hdr.biBitCount = 32;
        ico_hdr.biCompression = BI_RGB.0 as u32;
        let hdr_ptr = &ico_hdr as *const BITMAPINFOHEADER as *const u8;
        ico.extend_from_slice(std::slice::from_raw_parts(hdr_ptr, 40));
        ico.extend_from_slice(&pixels);
        // AND mask：32bpp 带 alpha，mask 全 0 即可
        ico.resize(ico.len() + (and_stride * hpx) as usize, 0);
        Some(ico)
    }
}
