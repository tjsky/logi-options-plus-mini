//! 只做一件事：把图标、版本信息、DPI 清单编进 exe。
//!
//! 不用 `winres` crate —— 它会调 `reg.exe` 去查 SDK 路径，被安全策略拦下就整段失败。
//! 这里直接扫 `Windows Kits\10\bin\<版本>\x64\rc.exe`，找得到就编，找不到就跳过（不影响构建）。

use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=assets/app.rc");
    println!("cargo:rerun-if-changed=assets/app.ico");
    println!("cargo:rerun-if-changed=assets/app.manifest");

    let Some(rc) = find_rc() else {
        println!("cargo:warning=找不到 rc.exe，图标与版本信息将被跳过");
        return;
    };

    let Ok(out_dir) = std::env::var("OUT_DIR") else {
        return;
    };
    let res = PathBuf::from(out_dir).join("app.res");

    // 注意：rc.exe **不认 `/nologo`**，传了会直接 RC1107 invalid usage。
    let status = Command::new(&rc)
        .arg("/fo")
        .arg(&res)
        .arg("assets/app.rc")
        .status();

    match status {
        Ok(status) if status.success() => {
            println!("cargo:rustc-link-arg={}", res.display());
        }
        Ok(status) => println!("cargo:warning=rc.exe 退出码 {status}，跳过图标"),
        Err(error) => println!("cargo:warning=调用 rc.exe 失败：{error}"),
    }
}

fn find_rc() -> Option<PathBuf> {
    let base = PathBuf::from(r"C:\Program Files (x86)\Windows Kits\10\bin");
    let mut candidates: Vec<(Vec<u32>, PathBuf)> = std::fs::read_dir(base)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let rc = path.join("x64").join("rc.exe");
            if !rc.exists() {
                return None;
            }
            let key = path
                .file_name()?
                .to_string_lossy()
                .split('.')
                .filter_map(|part| part.parse::<u32>().ok())
                .collect();
            Some((key, rc))
        })
        .collect();

    candidates.sort_by(|left, right| left.0.cmp(&right.0));
    candidates.pop().map(|(_, path)| path)
}
