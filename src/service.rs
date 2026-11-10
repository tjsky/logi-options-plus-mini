//! 安装主流程。顺序严格对齐 C# 版，每一步的坑都在注释里写清楚了。

use std::path::Path;
use std::time::SystemTime;

use crate::features::{build_args, to_command_line};
use crate::net;
use crate::profile;
use crate::winapi;

pub struct InstallRequest {
    pub offline: bool,
    pub quiet: bool,
    pub features: Vec<bool>,
    pub backup_config: bool,
    /// 不卸载直接覆盖安装。离线版强制开启（官方离线包先卸载再装必崩）。
    pub cover_install: bool,
    pub keep_installer: bool,
    pub reuse_installer: bool,
    /// 只看不做：把要执行的命令和改动列出来，不动系统。
    pub dry_run: bool,
}

pub struct Outcome {
    pub success: bool,
    pub cancelled: bool,
    pub message: String,
}

struct Failure {
    message: String,
    cancelled: bool,
}

impl Failure {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            cancelled: false,
        }
    }

    fn cancelled() -> Self {
        Self {
            message: "你取消了管理员授权，流程已停（配置备份还留着）。".into(),
            cancelled: true,
        }
    }
}

type Logger<'a> = &'a dyn Fn(String);
type Progress<'a> = &'a dyn Fn(Option<f32>, String);

pub fn run(req: &InstallRequest, log: Logger, progress: Progress) -> Outcome {
    let mut backup_done = false;
    let mut restored = false;

    match run_inner(req, log, progress, &mut backup_done, &mut restored) {
        Ok(message) => Outcome {
            success: true,
            cancelled: false,
            message,
        },
        Err(failure) => {
            if backup_done && !restored {
                log("流程中断，尝试把配置从备份还原…".to_string());
                if let Err(error) = profile::restore_config(log) {
                    log(format!("还原失败：{error}（备份仍在 {}）", winapi::backup_dir().display()));
                }
            }

            Outcome {
                success: false,
                cancelled: failure.cancelled,
                message: failure.message,
            }
        }
    }
}

fn run_inner(
    req: &InstallRequest,
    log: Logger,
    progress: Progress,
    backup_done: &mut bool,
    restored: &mut bool,
) -> Result<String, Failure> {
    std::fs::create_dir_all(winapi::work_dir()).ok();

    // ---- 1. 现状
    let installed = winapi::detect_installed();
    if installed.found() {
        log(format!(
            "已安装：版本 {}，位置 {}",
            installed.version.clone().unwrap_or_else(|| "未知".into()),
            installed.location.clone().unwrap_or_else(|| "未知".into())
        ));
    } else {
        log("未检测到已安装的 Logi Options+，按全新安装处理（跳过卸载）。".to_string());
    }

    if req.offline {
        log("离线版：官方安装器本体 700MB 左右，装完占用 500MB 左右——比在线版的 750MB 左右更小。\
             它是罗技官方出的「去掉需联网功能版本」，装完不联网也能用，但更新会比在线版晚几周。"
            .to_string());
    }

    // ---- 2. 安装包：复用缓存 / 下载
    let file_name = if req.offline {
        "logioptionsplus_installer_offline.exe"
    } else {
        "logioptionsplus_installer.exe"
    };
    let installer_path = winapi::cache_dir().join(file_name);
    let mut verified = None;

    if req.reuse_installer && installer_path.exists() {
        let cached = winapi::verify_signature(&installer_path);
        if cached.is_valid() {
            log(format!("复用上次下载的安装包（签名已校验）：{}", installer_path.display()));
            verified = Some(cached);
        } else {
            log(format!("缓存的安装包没通过签名校验（{}），重新下载。", cached.detail));
            let _ = std::fs::remove_file(&installer_path);
        }
    }

    if verified.is_none() {
        let china = net::detect_china();
        let url = if req.offline {
            net::offline_url(china)
        } else {
            net::installer_url(china)
        };
        log(format!(
            "下载源：{url}（{}）",
            if china { "中国区" } else { "国际区" }
        ));

        let downloaded = net::download(url, &installer_path, |received, total| {
            let fraction = total.map(|t| (received as f32 / t as f32).clamp(0.0, 1.0));
            let label = match total {
                Some(t) => format!(
                    "正在下载安装包：{:.1} / {:.1} MB",
                    mb(received),
                    mb(t)
                ),
                None => format!("正在下载安装包：{:.1} MB", mb(received)),
            };
            progress(fraction, label);
        });

        match downloaded {
            Ok(bytes) => log(format!("下载完成：{:.1} MB", mb(bytes))),
            Err(error) => return Err(Failure::new(format!("下载失败：{error}"))),
        }
    }

    // ---- 3. 签名：宁可装不上，也不装来源不明的包
    progress(None, "正在校验安装包数字签名…".to_string());
    let signature = match verified {
        Some(result) => result,
        None => winapi::verify_signature(&installer_path),
    };
    log(format!("签名校验：{}", signature.detail));
    if !signature.is_valid() {
        return Err(Failure::new(
            "签名未通过，已停止。要强行继续请在高级设置里放开（不建议）。",
        ));
    }

    // ---- 4. 结束占用进程
    winapi::stop_logitech_processes(log);

    // ---- 5. 备份配置
    if req.backup_config {
        match profile::backup_config(log) {
            Ok(files) => {
                log(format!(
                    "配置已备份到 {}（{files} 个文件）",
                    winapi::backup_dir().display()
                ));
                *backup_done = true;
            }
            Err(reason) => log(reason),
        }
    } else {
        log("已选择不备份配置：卸载后按键自定义会丢。".to_string());
    }

    // ---- 6. 卸载旧版本
    // 离线包是「覆盖安装」设计：先卸载再装会在解包阶段抛 C++ 异常（0xE06D7363），
    // 而且会把配置目录清掉，所以离线版一律跳过卸载。
    let skip_uninstall = req.cover_install || req.offline;

    if installed.found() && skip_uninstall {
        log(if req.offline {
            "离线版按「覆盖安装」处理，不卸载旧版本（官方推荐方式）。".to_string()
        } else {
            "按设置跳过卸载，直接覆盖安装。".to_string()
        });
    } else if installed.found() {
        let uninstaller = winapi::resolve_uninstaller(installed.location.as_deref());
        let (file, args, kind) = match &uninstaller {
            Some(path) => (
                path.clone(),
                vec!["--uninstall".to_string(), "--full".to_string(), "--shadow".to_string()],
                "产品自带卸载器",
            ),
            None => (
                installer_path.clone(),
                vec!["/uninstall".to_string()],
                "安装包 /uninstall",
            ),
        };

        progress(None, "正在卸载旧版本…".to_string());
        log(format!("使用{kind}卸载：{} {}", file.display(), to_command_line(&args)));

        if req.dry_run {
            log("[只看不做] 跳过实际卸载。".to_string());
        } else {
            match winapi::run_elevated(&file, &args, None) {
                Ok(outcome) if outcome.cancelled => return Err(Failure::cancelled()),
                Ok(outcome) if outcome.exit_code != 0 => {
                    // 上游脚本忽略卸载退出码，这里也只警告不中断：旧版本可能本来就没装干净
                    log(format!(
                        "卸载程序返回退出码 {}，继续尝试安装新版本。",
                        outcome.exit_code
                    ));
                }
                Ok(outcome) => {
                    log(format!("卸载完成（退出码 {}）", outcome.exit_code));
                    verify_uninstall(&file, &args, log);
                }
                Err(error) => log(format!("卸载启动失败：{error}，继续尝试安装。")),
            }
        }
    }

    // ---- 7. 还原配置：必须在安装之前，让新版本直接沿用旧配置
    if *backup_done {
        progress(None, "正在还原配置…".to_string());
        if req.dry_run {
            log(format!(
                "[只看不做] 将把 {} 还原到 {}",
                winapi::backup_dir().display(),
                winapi::config_dir().display()
            ));
        } else {
            match profile::restore_config(log) {
                Ok(_) => *restored = true,
                Err(error) => log(format!("还原失败：{error}")),
            }
        }
    }

    // ---- 8. 安装
    let args = build_args(req.quiet, &req.features, Some(&winapi::log_dir().to_string_lossy()));
    log(format!("安装命令：{} {}", file_name, to_command_line(&args)));

    if req.dry_run {
        log("[只看不做] 未实际执行安装，系统没有任何改动。".to_string());
        return Ok("检查完成，系统没有改动。".to_string());
    }

    progress(Some(1.0), "正在安装…（官方安装器可能需要几分钟）".to_string());

    // 官方安装器遇到「同版本」会直接跳过，参数不会重新写入，这里留时间戳事后比对
    let permissions_before = mtime(&winapi::permissions_path());

    let install = winapi::run_elevated(
        &installer_path,
        &args,
        Some(&winapi::cache_dir()),
    );

    let exit_code = match install {
        Ok(outcome) if outcome.cancelled => return Err(Failure::cancelled()),
        Ok(outcome) => outcome.exit_code,
        Err(error) => return Err(Failure::new(format!("安装器启动失败：{error}"))),
    };

    let permissions_after = mtime(&winapi::permissions_path());
    if permissions_after.is_some() && permissions_after == permissions_before {
        log("官方安装器判定为同一版本并跳过了安装，功能开关没有被重新写入。\
             想让新的勾选生效：先卸载再装、换一个不同版本，或改用离线版覆盖安装\
             （实测离线包会直接重装并重写开关，代价是包内版本通常旧几周）。"
            .to_string());
    }

    // ---- 9. 按结果复核：退出码不可信（实测会返回 1008，但应用其实装好了）
    let after = winapi::detect_installed();

    if exit_code != 0 {
        if !after.found() {
            return Err(Failure::new(format!(
                "安装器报错 {exit_code}，也没找到装好的程序。"
            )));
        }

        log(format!(
            "安装器返回退出码 {exit_code}，但已确认安装成功（版本 {}）。退出码 1008 属于已知误报。",
            after.version.clone().unwrap_or_else(|| "未知".into())
        ));
    }

    if let (Some(before), Some(now)) = (&installed.version, &after.version) {
        if req.offline && compare_versions(now, before) < 0 {
            log(format!(
                "注意：装完的版本 {now} 低于原来的 {before}。官方离线包通常比在线版晚几周发布，\
                 用离线版升级会出现版本倒退。"
            ));
        }
    }

    // ---- 10. 清理
    if req.keep_installer || req.reuse_installer {
        log(format!("安装包已保留在缓存：{}", installer_path.display()));
    } else {
        let _ = std::fs::remove_file(&installer_path);
        log("已清理临时安装包（想留着下次用就勾「保留安装包」）。".to_string());
    }

    progress(None, String::new());
    Ok(format!(
        "装好了，现在是 {}。",
        after.version.unwrap_or_else(|| "未知版本".into())
    ))
}

fn mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|meta| meta.modified()).ok()
}

/// 官方卸载器偶尔会静默空转：退出码 0 但注册表和安装目录纹丝不动
/// （2026-11-10 真机实测踩到一次，同样命令稍后手动重跑即成功）。
/// 所以退出码不足为凭，必须按系统实际状态复核；确认空转就重试一次。
fn verify_uninstall(file: &Path, args: &[String], log: Logger) {
    if !winapi::detect_installed().found() {
        return;
    }

    log("卸载器退出码为 0，但程序还在（注册表键和安装目录都没动）——这次卸载是空操作，重试一次…"
        .to_string());
    std::thread::sleep(std::time::Duration::from_secs(2));

    match winapi::run_elevated(file, args, None) {
        Ok(outcome) if outcome.cancelled => {
            log("重试的卸载被取消，继续按原状态尝试安装。".to_string());
        }
        Ok(_) => {
            if winapi::detect_installed().found() {
                log("重试后程序仍在。接下来官方安装器大概率会判定同版本而跳过，功能开关不会生效。\
                     可以改用离线版覆盖安装——实测离线包不查更新状态，会直接重装并重写开关。"
                    .to_string());
            } else {
                log("重试后卸载成功。".to_string());
            }
        }
        Err(error) => log(format!("重试启动失败：{error}")),
    }
}

fn mb(bytes: u64) -> f32 {
    bytes as f32 / 1024.0 / 1024.0
}

/// 只用于「版本有没有倒退」这一处提示，够用即可。
pub fn compare_versions(left: &str, right: &str) -> i32 {
    let parse = |value: &str| -> Vec<u64> {
        value
            .split(['.', '-', '+'])
            .map(|part| part.parse::<u64>().unwrap_or(0))
            .collect()
    };

    let a = parse(left);
    let b = parse(right);

    for index in 0..a.len().max(b.len()) {
        let x = a.get(index).copied().unwrap_or(0);
        let y = b.get(index).copied().unwrap_or(0);
        if x != y {
            return if x > y { 1 } else { -1 };
        }
    }

    0
}

/// 给 UI 用：把「会做什么」拼成一句话，不执行任何操作。
pub fn describe_plan(req: &InstallRequest) -> Vec<String> {
    let mut lines = Vec::new();
    let installed = winapi::detect_installed();
    let edition = if req.offline { "离线版" } else { "在线版" };

    lines.push(match &installed.version {
        Some(version) => format!("本机已装 {version}，将按{edition}流程处理"),
        None => format!("本机未安装，将用{edition}全新安装"),
    });

    if installed.found() {
        if req.cover_install || req.offline {
            lines.push("不卸载，直接覆盖安装".into());
        } else if let Some(updater) = winapi::resolve_uninstaller(installed.location.as_deref()) {
            lines.push(format!("先用 {} 卸载", updater.display()));
        } else {
            lines.push("用安装包的 /uninstall 卸载".into());
        }
    }

    if req.backup_config {
        lines.push(format!(
            "备份配置：{} → {}",
            winapi::config_dir().display(),
            winapi::backup_dir().display()
        ));
    }

    lines.push(format!(
        "安装参数：{}",
        to_command_line(&build_args(req.quiet, &req.features, None))
    ));

    lines
}
