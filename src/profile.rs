//! 两件事：
//! 1. 记住用户上次的选择（`profile.json`），免得每次重装都要重新勾一遍；
//! 2. 安装前备份 / 安装后还原 `%LOCALAPPDATA%\LogiOptionsPlus`（按键自定义就在里面）。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::winapi;

#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub quiet: bool,
    pub backup_config: bool,
    pub keep_installer: bool,
    pub reuse_installer: bool,
    pub offline: bool,
    /// 离线版默认「不卸载直接覆盖安装」：官方离线包是先卸载再装必崩（0xE06D7363）。
    pub cover_install: bool,
    pub features: Vec<bool>,
    pub dark_theme: Option<bool>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            quiet: true,
            backup_config: true,
            keep_installer: false,
            reuse_installer: false,
            offline: false,
            cover_install: false,
            features: crate::features::preset(crate::features::Edition::Online),
            dark_theme: None,
        }
    }
}

pub fn profile_path() -> PathBuf {
    winapi::work_dir().join("profile.json")
}

pub fn load() -> Settings {
    let mut settings = std::fs::read_to_string(profile_path())
        .ok()
        .and_then(|text| serde_json::from_str::<Settings>(&text).ok())
        .unwrap_or_default();

    // 功能列表长度可能因为版本升级而变化，长度不对就回落到默认预设，
    // 否则后面按下标取值会越界。
    if settings.features.len() != crate::features::FEATURES.len() {
        settings.features = crate::features::preset(if settings.offline {
            crate::features::Edition::Offline
        } else {
            crate::features::Edition::Online
        });
    }

    settings
}

pub fn save(settings: &Settings) {
    let path = profile_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(settings) {
        let _ = std::fs::write(path, text);
    }
}

// ---------------------------------------------------------------- 配置备份 / 还原

/// 备份配置目录。返回备份的文件数；目录不存在时返回 Err("没有可备份的配置")。
pub fn backup_config(log: &dyn Fn(String)) -> Result<usize, String> {
    let source = winapi::config_dir();
    let target = winapi::backup_dir();

    if !source.exists() {
        return Err("没有已存在的配置目录，跳过备份".into());
    }

    if target.exists() {
        std::fs::remove_dir_all(&target)
            .map_err(|e| format!("旧备份目录删不掉：{e}"))?;
    }

    let (files, failures) = copy_dir(&source, &target).map_err(|e| format!("备份失败：{e}"))?;

    if failures > 0 {
        log(format!(
            "备份时有 {failures} 个文件被占用或无法读取，已跳过（这些文件多半是运行中的缓存）。"
        ));
    }

    Ok(files)
}

/// 还原配置。
///
/// **绝不先删目标目录**。这是实测踩出来的：`remove_dir_all` 是一个个删的，
/// 遇到被 Logi Options+ 锁住的 `*.db` 会先删掉一百多个文件、再在锁文件上失败，
/// 结果配置目录被删残、备份也被后来的备份覆盖，属于不可接受的破坏。
/// 所以目标已存在时一律「就地覆盖」，锁住的文件跳过并保留备份。
pub fn restore_config(log: &dyn Fn(String)) -> Result<usize, String> {
    let source = winapi::backup_dir();
    let target = winapi::config_dir();

    if !source.exists() {
        return Err("没有找到备份目录".into());
    }

    // 目标不存在（全新安装场景）：整目录搬回最快
    if !target.exists() {
        if std::fs::rename(&source, &target).is_ok() {
            log("配置已还原（整个目录搬回原位）".to_string());
            return Ok(0);
        }

        let (files, _) = copy_dir(&source, &target).map_err(|e| format!("复制还原失败：{e}"))?;
        let _ = std::fs::remove_dir_all(&source);
        log(format!("配置已复制还原（{files} 个文件）"));
        return Ok(files);
    }

    // 目标已存在：只覆盖，不删除
    let (files, failures) = copy_dir(&source, &target).map_err(|e| format!("还原失败：{e}"))?;

    if failures > 0 {
        log(format!(
            "还原了 {files} 个文件，{failures} 个被占用跳过（多半是程序还在跑的数据库）。\
             备份保留在 {}，关掉 Logi Options+ 后可以再还原一次。",
            source.display()
        ));
    } else {
        let _ = std::fs::remove_dir_all(&source);
        log(format!("配置已还原（{files} 个文件覆盖回原位）"));
    }

    Ok(files)
}

/// 递归复制。单个文件失败不中断整体，只计数——被锁的缓存文件不该让整个备份失败。
fn copy_dir(source: &Path, destination: &Path) -> std::io::Result<(usize, usize)> {
    std::fs::create_dir_all(destination)?;

    let mut files = 0usize;
    let mut failures = 0usize;
    let mut stack = vec![source.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => {
                failures += 1;
                continue;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let relative = path.strip_prefix(source).unwrap_or(&path);
            let target = destination.join(relative);

            if path.is_dir() {
                let _ = std::fs::create_dir_all(&target);
                stack.push(path);
                continue;
            }

            match std::fs::copy(&path, &target) {
                Ok(_) => files += 1,
                Err(_) => failures += 1,
            }
        }
    }

    Ok((files, failures))
}
