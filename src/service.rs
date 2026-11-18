//! 安装主流程。顺序严格对齐 C# 版，每一步的坑都在注释里写清楚了。

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

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

/// 官方卸载器把真正的删除交给后台进程，主进程几秒内就返回 0。
/// 所以「退出码 0」之后不能立刻查状态——必须轮询到注册表和安装目录真的消失。
///
/// 2026-11-18 实测：返回后立刻查询会得到「程序还在」，几秒后再查却已经彻底删干净。
/// 11-10 记在案的「官方卸载器静默空转」其实就是这个查询太早造成的误判。
fn wait_removed(timeout: Duration, log: Logger) -> bool {
    if !winapi::detect_installed().found() {
        return true;
    }

    let deadline = Instant::now() + timeout;
    let mut waited = 0u64;

    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(500));
        waited += 500;

        if !winapi::detect_installed().found() {
            log(format!("程序已移除（等待了约 {:.1} 秒）。", waited as f64 / 1000.0));
            return true;
        }

        if waited % 10_000 == 0 {
            log(format!("卸载器仍在后台清理…（已等待 {} 秒）", waited / 1000));
        }
    }

    false
}

/// 卸载后复核：退出码不足为凭，按系统实际状态等待并判断；
/// 确认没动静才重试一次。
fn verify_uninstall(file: &Path, args: &[String], log: Logger) {
    if wait_removed(Duration::from_secs(75), log) {
        return;
    }

    log("卸载器已经返回，但等待后程序仍在——重试一次…".to_string());
    std::thread::sleep(Duration::from_secs(1));

    match winapi::run_elevated(file, args, None) {
        Ok(outcome) if outcome.cancelled => {
            log("重试的卸载被取消。".to_string());
        }
        Ok(_) => {
            if wait_removed(Duration::from_secs(75), log) {
                log("重试后卸载成功。".to_string());
            } else {
                log("重试后程序仍在。可以重启后再试，或者从「设置 → 应用」里卸载。"
                    .to_string());
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

// ---------------------------------------------------------------- 卸载

pub struct UninstallRequest {
    /// 同时删掉按键配置、备份目录、更新器状态机和下载缓存
    pub purge_config: bool,
    pub dry_run: bool,
}

pub fn run_uninstall(req: &UninstallRequest, log: Logger, progress: Progress) -> Outcome {
    match uninstall_inner(req, log, progress) {
        Ok(message) => Outcome {
            success: true,
            cancelled: false,
            message,
        },
        Err(failure) => Outcome {
            success: false,
            cancelled: failure.cancelled,
            message: failure.message,
        },
    }
}

fn uninstall_inner(
    req: &UninstallRequest,
    log: Logger,
    progress: Progress,
) -> Result<String, Failure> {
    std::fs::create_dir_all(winapi::work_dir()).ok();

    let installed = winapi::detect_installed();
    if installed.found() {
        log(format!(
            "已安装：版本 {}，位置 {}",
            installed.version.clone().unwrap_or_else(|| "未知".into()),
            installed.location.clone().unwrap_or_else(|| "未知".into())
        ));
    } else {
        log("本机没有检测到 Logi Options+，跳过卸载。".to_string());
    }

    let command = if installed.found() {
        uninstall_command(&installed)
    } else {
        None
    };

    if installed.found() && command.is_none() {
        return Err(Failure::new(
            "没找到可用的卸载程序：产品自带卸载器和缓存安装包都不在。\
             先跑一次「开始安装」把官方安装包下载下来，再回来卸载即可。",
        ));
    }

    let mut uninstalled = false;

    if let Some((file, args, kind)) = &command {
        log(format!(
            "将使用{kind}卸载：{} {}",
            file.display(),
            to_command_line(args)
        ));

        if req.dry_run {
            log("[只看不做] 未执行卸载，系统没有任何改动。".to_string());
        } else {
            winapi::stop_logitech_processes(log);
            progress(None, "正在卸载…（官方卸载器可能要 1~2 分钟）".to_string());

            match winapi::run_elevated(file, args, None) {
                Ok(outcome) if outcome.cancelled => return Err(Failure::cancelled()),
                Ok(outcome) if outcome.exit_code != 0 => log(format!(
                    "卸载器返回退出码 {}。退出码不可信，按系统实际状态复核。",
                    outcome.exit_code
                )),
                Ok(outcome) => log(format!("卸载器退出码 {}", outcome.exit_code)),
                Err(error) => return Err(Failure::new(format!("卸载器启动失败：{error}"))),
            }

            // 实测官方卸载器会静默空转：退出码 0，注册表和安装目录纹丝不动
            verify_uninstall(file, args, log);

            if winapi::detect_installed().found() {
                return Err(Failure::new(
                    "卸载没有生效：注册表和安装目录仍然存在。\
                     重启后再试一次，或者从「设置 → 应用」里卸载。",
                ));
            }

            log("卸载完成：注册表项与安装目录都已清除。".to_string());
            uninstalled = true;
        }
    }

    if req.purge_config {
        let mut denied: Vec<PathBuf> = Vec::new();

        for path in purge_targets() {
            if req.dry_run {
                log(format!("[只看不做] 将删除 {}", path.display()));
            } else if !remove_dir(&path, log) {
                denied.push(path);
            }
        }

        // `%ProgramData%\LogiOptionsPlus` 属主是管理员，普通权限删不掉，
        // 只能为一个目录多弹一次授权（用户主动勾了「彻底清理」才走到这里）。
        if !denied.is_empty() {
            log("有目录需要管理员权限，正在提权删除…".to_string());
            for path in denied {
                elevate_remove_dir(&path, log);
            }
        }
    } else if !req.dry_run {
        log("按键配置和下载缓存按设置保留着（想彻底清掉请勾「同时删除配置与残留」）。".to_string());
    }

    progress(None, String::new());

    Ok(if req.dry_run {
        "检查完成，系统没有改动。".to_string()
    } else if uninstalled {
        "已卸载干净。".to_string()
    } else {
        "本机本来就没有安装 Logi Options+。".to_string()
    })
}

/// 卸载器选取顺序和安装流程一致：优先产品自带卸载器（注册表 UninstallString 原文），
/// 其次用缓存里上次下载的安装包走 `/uninstall`。
fn uninstall_command(installed: &winapi::Installed) -> Option<(PathBuf, Vec<String>, &'static str)> {
    if let Some(updater) = winapi::resolve_uninstaller(installed.location.as_deref()) {
        return Some((
            updater,
            vec![
                "--uninstall".to_string(),
                "--full".to_string(),
                "--shadow".to_string(),
            ],
            "产品自带卸载器",
        ));
    }

    for name in [
        "logioptionsplus_installer.exe",
        "logioptionsplus_installer_offline.exe",
    ] {
        let cached = winapi::cache_dir().join(name);
        if cached.exists() {
            return Some((cached, vec!["/uninstall".to_string()], "缓存安装包"));
        }
    }

    None
}

/// 「干净」指的不只是卸载，还包括更新器状态机和下载缓存这些不会自己消失的东西。
fn purge_targets() -> Vec<PathBuf> {
    vec![
        winapi::config_dir(),
        winapi::backup_dir(),
        winapi::program_data().join("LogiOptionsPlus"),
        winapi::cache_dir(),
    ]
}

/// 返回 true 表示目录已经不存在（删掉了，或者本来就没有）。
fn remove_dir(path: &Path, log: Logger) -> bool {
    if !path.exists() {
        return true;
    }

    match std::fs::remove_dir_all(path) {
        Ok(()) => {
            log(format!("已删除 {}", path.display()));
            true
        }
        Err(error) => {
            log(format!("删除 {} 没成功：{error}", path.display()));
            false
        }
    }
}

/// 提权只为一个目录服务：`%ProgramData%\LogiOptionsPlus` 的属主是管理员，
/// 普通权限连里面单个 json 都改不了。用系统自带的 cmd /c rd，不引入额外依赖。
fn elevate_remove_dir(path: &Path, log: Logger) -> bool {
    if !path.exists() {
        return true;
    }

    let cmd = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("C:/Windows"))
        .join("System32")
        .join("cmd.exe");

    let args = vec![
        "/c".to_string(),
        "rd".to_string(),
        "/s".to_string(),
        "/q".to_string(),
        path.to_string_lossy().to_string(),
    ];

    match winapi::run_elevated(&cmd, &args, None) {
        Ok(outcome) if outcome.cancelled => {
            log(format!("提权删除被取消，{} 还留着。", path.display()));
            false
        }
        Ok(_) => {
            if path.exists() {
                log(format!("{} 仍然存在（可能被占用）。", path.display()));
                false
            } else {
                log(format!("已删除 {}", path.display()));
                true
            }
        }
        Err(error) => {
            log(format!("提权删除 {} 失败：{error}", path.display()));
            false
        }
    }
}

pub fn describe_uninstall(req: &UninstallRequest) -> Vec<String> {
    let mut lines = Vec::new();
    let installed = winapi::detect_installed();

    lines.push(match &installed.version {
        Some(version) => format!("本机已装 {version}，将执行卸载"),
        None => "本机未安装，无需卸载".to_string(),
    });

    if installed.found() {
        match uninstall_command(&installed) {
            Some((file, args, kind)) => lines.push(format!(
                "用{kind}卸载：{} {}",
                file.display(),
                to_command_line(&args)
            )),
            None => lines.push("找不到卸载程序，需要先下载官方安装包".to_string()),
        }
    }

    if req.purge_config {
        for path in purge_targets() {
            lines.push(format!("删除 {}", path.display()));
        }
    } else {
        lines.push("保留按键配置与下载缓存".to_string());
    }

    lines
}
