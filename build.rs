use std::env;
use std::fs;
use std::path::Path;

fn main() {
    // 仅构建 psman 自身二进制（CLI）时嵌入图标资源；
    // 作为依赖被 psman-gui 链接时跳过，避免 VERSION 资源与 tauri-build 的 resource.lib 冲突（CVT1100）
    if env::var("CARGO_BIN_NAME").is_err() {
        return;
    }

    // 图标变更时重新编译资源
    println!("cargo:rerun-if-changed=psman.ico");

    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let out_dir = env::var("OUT_DIR").unwrap();

    let src = Path::new(&manifest_dir).join("psman.ico");
    let dst = Path::new(&out_dir).join("psman.ico");
    let _ = fs::copy(&src, &dst);

    let mut res = winres::WindowsResource::new();
    res.set_icon("psman.ico");
    res.compile().unwrap();
}
