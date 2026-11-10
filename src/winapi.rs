//! Win32 交互：签名校验、提权执行、已安装检测。
//!
//! 这里刻意**不引 `windows` crate**：只用到 4 个 API，直接手写 FFI 就够了。
//! 代价是几十行声明，收益是少一棵巨型依赖树（编译时间与产物体积都省），
//! 而且结构体布局能用编译期断言钉死（WINTRUST_DATA 必须 88 字节、SHELLEXECUTEINFOW 必须 112 字节，
//! 与 C# 版实测值一致）。

#![allow(non_snake_case, non_camel_case_types)]

use std::ffi::c_void;
use std::mem::size_of;
use std::path::{Path, PathBuf};

type BOOL = i32;
type HANDLE = *mut c_void;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GUID {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}

const WINTRUST_ACTION_GENERIC_VERIFY_V2: GUID = GUID {
    data1: 0x00aa_c56b,
    data2: 0xcd44,
    data3: 0x11d0,
    data4: [0x8c, 0xc2, 0x00, 0xc0, 0x4f, 0xc2, 0x95, 0xee],
};

#[repr(C)]
struct WINTRUST_FILE_INFO {
    cbStruct: u32,
    pcwszFilePath: *const u16,
    hFile: HANDLE,
    pgKnownSubject: *mut GUID,
}

#[repr(C)]
struct WINTRUST_DATA {
    cbStruct: u32,
    pPolicyCallbackData: *mut c_void,
    pSIPClientData: *mut c_void,
    dwUIChoice: u32,
    fdwRevocationChecks: u32,
    dwUnionChoice: u32,
    pFile: *mut WINTRUST_FILE_INFO,
    dwStateAction: u32,
    hWVTStateData: HANDLE,
    pwszURLReference: *mut u16,
    dwProvFlags: u32,
    dwUIContext: u32,
    pSignatureSettings: *mut c_void,
}

#[repr(C)]
struct SHELLEXECUTEINFOW {
    cbSize: u32,
    fMask: u32,
    hwnd: *mut c_void,
    lpVerb: *const u16,
    lpFile: *const u16,
    lpParameters: *const u16,
    lpDirectory: *const u16,
    nShow: i32,
    hInstApp: *mut c_void,
    lpIDList: *mut c_void,
    lpClass: *const u16,
    hkeyClass: *mut c_void,
    dwHotKey: u32,
    hIcon: *mut c_void,
    hProcess: HANDLE,
}

// 布局自检：与 C# 版在 x64 上的实测值完全一致，改结构体字段会在这里炸掉。
const _: () = assert!(size_of::<WINTRUST_FILE_INFO>() == 32);
const _: () = assert!(size_of::<WINTRUST_DATA>() == 88);
const _: () = assert!(size_of::<SHELLEXECUTEINFOW>() == 112);

#[link(name = "wintrust")]
unsafe extern "system" {
    fn WinVerifyTrust(hwnd: *mut c_void, action: *mut GUID, data: *mut c_void) -> i32;
}

#[link(name = "shell32")]
unsafe extern "system" {
    fn ShellExecuteExW(info: *mut SHELLEXECUTEINFOW) -> BOOL;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn WaitForSingleObject(handle: HANDLE, milliseconds: u32) -> u32;
    fn GetExitCodeProcess(handle: HANDLE, code: *mut u32) -> BOOL;
    fn CloseHandle(handle: HANDLE) -> BOOL;
    fn GetLastError() -> u32;
}

const WTD_UI_NONE: u32 = 2;
const WTD_REVOKE_NONE: u32 = 0;
const WTD_CHOICE_FILE: u32 = 1;
const WTD_STATEACTION_VERIFY: u32 = 1;
const WTD_STATEACTION_CLOSE: u32 = 2;
const WTD_CACHE_ONLY_URL_RETRIEVAL: u32 = 0x1000;

const SEE_MASK_NOCLOSEPROCESS: u32 = 0x40;
const SW_SHOWNORMAL: i32 = 1;
const INFINITE: u32 = 0xFFFF_FFFF;
const ERROR_CANCELLED: u32 = 1223;

pub fn to_wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

// ---------------------------------------------------------------- 签名校验

pub enum SignatureStatus {
    Valid,
    NoSignature,
    Untrusted,
    Error,
}

pub struct SignatureResult {
    pub status: SignatureStatus,
    pub detail: String,
}

impl SignatureResult {
    pub fn is_valid(&self) -> bool {
        matches!(self.status, SignatureStatus::Valid)
    }
}

const TRUST_E_NOSIGNATURE: i32 = 0x800B0100u32 as i32;
const TRUST_E_SUBJECT_FORM_UNKNOWN: i32 = 0x800B0003u32 as i32;
const TRUST_E_BAD_DIGEST: i32 = 0x80096010u32 as i32;
const TRUST_E_EXPLICIT_DISTRUST: i32 = 0x800B0111u32 as i32;
const CERT_E_UNTRUSTEDROOT: i32 = 0x800B0109u32 as i32;
const CERT_E_CHAINING: i32 = 0x800B010Au32 as i32;

/// 用 Authenticode（WinVerifyTrust）校验签名。
/// 先只用本地缓存（快、离线可用），证书链不齐时再联网重试一次。
pub fn verify_signature(path: &Path) -> SignatureResult {
    if !path.exists() {
        return SignatureResult {
            status: SignatureStatus::Error,
            detail: "安装包不存在，无法校验签名".into(),
        };
    }

    let path_str = path.to_string_lossy().to_string();
    let mut result = verify_core(&path_str, WTD_CACHE_ONLY_URL_RETRIEVAL);
    if matches!(result.status, SignatureStatus::Untrusted) && result.detail.contains("证书链") {
        result = verify_core(&path_str, 0);
    }
    result
}

fn verify_core(path: &str, provider_flags: u32) -> SignatureResult {
    let wide = to_wide(path);

    unsafe {
        let mut file_info = WINTRUST_FILE_INFO {
            cbStruct: size_of::<WINTRUST_FILE_INFO>() as u32,
            pcwszFilePath: wide.as_ptr(),
            hFile: std::ptr::null_mut(),
            pgKnownSubject: std::ptr::null_mut(),
        };

        let mut data = WINTRUST_DATA {
            cbStruct: size_of::<WINTRUST_DATA>() as u32,
            pPolicyCallbackData: std::ptr::null_mut(),
            pSIPClientData: std::ptr::null_mut(),
            dwUIChoice: WTD_UI_NONE,
            fdwRevocationChecks: WTD_REVOKE_NONE,
            dwUnionChoice: WTD_CHOICE_FILE,
            pFile: &mut file_info,
            dwStateAction: WTD_STATEACTION_VERIFY,
            hWVTStateData: std::ptr::null_mut(),
            pwszURLReference: std::ptr::null_mut(),
            dwProvFlags: provider_flags,
            dwUIContext: 0,
            pSignatureSettings: std::ptr::null_mut(),
        };

        let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
        let hr = WinVerifyTrust(
            std::ptr::null_mut(),
            &mut action,
            &mut data as *mut _ as *mut c_void,
        );

        // 无论成败都要 CLOSE，否则 WinTrust 内部状态泄漏
        data.dwStateAction = WTD_STATEACTION_CLOSE;
        let _ = WinVerifyTrust(
            std::ptr::null_mut(),
            &mut action,
            &mut data as *mut _ as *mut c_void,
        );

        let (status, detail) = match hr {
            0 => (SignatureStatus::Valid, "签名有效".to_string()),
            TRUST_E_NOSIGNATURE => (SignatureStatus::NoSignature, "安装包没有数字签名".to_string()),
            TRUST_E_SUBJECT_FORM_UNKNOWN => (
                SignatureStatus::NoSignature,
                "文件不是有效的可执行格式".to_string(),
            ),
            TRUST_E_BAD_DIGEST => (
                SignatureStatus::Untrusted,
                "签名摘要不匹配，文件可能被篡改".to_string(),
            ),
            TRUST_E_EXPLICIT_DISTRUST => (
                SignatureStatus::Untrusted,
                "证书已被系统列入不信任列表".to_string(),
            ),
            CERT_E_UNTRUSTEDROOT => (
                SignatureStatus::Untrusted,
                "证书链不受信任（根证书缺失）".to_string(),
            ),
            CERT_E_CHAINING => (
                SignatureStatus::Untrusted,
                "无法构建证书链（缺少中间证书）".to_string(),
            ),
            other => (
                SignatureStatus::Untrusted,
                format!("签名校验失败 (HRESULT 0x{:08X})", other),
            ),
        };

        SignatureResult { status, detail }
    }
}

// ---------------------------------------------------------------- 提权执行

pub struct RunOutcome {
    pub exit_code: u32,
    pub cancelled: bool,
}

/// 以管理员身份启动并等待结束。UAC 被取消时 GetLastError 返回 1223。
pub fn run_elevated(
    exe: &Path,
    args: &[String],
    work_dir: Option<&Path>,
) -> Result<RunOutcome, String> {
    let exe_wide = to_wide(&exe.to_string_lossy());
    let params_wide = to_wide(&join_args(args));
    let verb_wide = to_wide("runas");
    let dir_wide = work_dir.map(|d| to_wide(&d.to_string_lossy()));

    unsafe {
        let mut info = SHELLEXECUTEINFOW {
            cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask: SEE_MASK_NOCLOSEPROCESS,
            hwnd: std::ptr::null_mut(),
            lpVerb: verb_wide.as_ptr(),
            lpFile: exe_wide.as_ptr(),
            lpParameters: params_wide.as_ptr(),
            lpDirectory: match &dir_wide {
                Some(dir) => dir.as_ptr(),
                None => std::ptr::null(),
            },
            nShow: SW_SHOWNORMAL,
            hInstApp: std::ptr::null_mut(),
            lpIDList: std::ptr::null_mut(),
            lpClass: std::ptr::null(),
            hkeyClass: std::ptr::null_mut(),
            dwHotKey: 0,
            hIcon: std::ptr::null_mut(),
            hProcess: std::ptr::null_mut(),
        };

        if ShellExecuteExW(&mut info) == 0 {
            let code = GetLastError();
            if code == ERROR_CANCELLED {
                return Ok(RunOutcome {
                    exit_code: 0,
                    cancelled: true,
                });
            }
            return Err(format!("启动安装器失败（GetLastError={code}）"));
        }

        if info.hProcess.is_null() {
            return Err("安装器已启动，但拿不到进程句柄（可能被安全软件拦截）".into());
        }

        WaitForSingleObject(info.hProcess, INFINITE);

        let mut exit_code = 0u32;
        let got = GetExitCodeProcess(info.hProcess, &mut exit_code) != 0;
        let _ = CloseHandle(info.hProcess);

        if !got {
            return Err("无法读取安装器退出码".into());
        }

        Ok(RunOutcome {
            exit_code,
            cancelled: false,
        })
    }
}

fn join_args(args: &[String]) -> String {
    args.iter()
        .map(|value| {
            if value.is_empty() {
                "\"\"".to_string()
            } else if value.contains([' ', '\t']) {
                format!("\"{value}\"")
            } else {
                value.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------------------------------------------------------------- 环境探测

pub fn local_app_data() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("C:/Users/Public"))
}

pub fn program_files() -> PathBuf {
    std::env::var_os("ProgramW6432")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("C:/Program Files"))
}

pub fn work_dir() -> PathBuf {
    local_app_data().join("LogiOptionsPlusMini")
}

pub fn cache_dir() -> PathBuf {
    work_dir().join("cache")
}

pub fn log_dir() -> PathBuf {
    work_dir().join("logs")
}

/// Logi Options+ 的用户配置目录（上游脚本做备份/还原的同一个目录）。
pub fn config_dir() -> PathBuf {
    local_app_data().join("LogiOptionsPlus")
}

pub fn backup_dir() -> PathBuf {
    local_app_data().join("LogiOptionsPlus_bak")
}

/// 安装器把功能开关写在这里，用来判断「同版本重装被静默跳过」。
pub fn permissions_path() -> PathBuf {
    program_files()
        .join("LogiOptionsPlus")
        .join("app_permissions.json")
}

pub struct Installed {
    pub version: Option<String>,
    pub location: Option<String>,
}

impl Installed {
    pub fn found(&self) -> bool {
        self.version.is_some() || self.location.is_some()
    }
}

/// 已安装信息：优先读罗技自己写的注册表键（有 Version 也有 MainAppPath），
/// 其次按卸载项 DisplayName 匹配，最后才看安装目录。
pub fn detect_installed() -> Installed {
    use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY};
    use winreg::RegKey;

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);

    if let Ok(key) = hklm.open_subkey_with_flags(
        r"SOFTWARE\Logitech\LogiOptionsPlus",
        KEY_READ | KEY_WOW64_64KEY,
    ) {
        let version = key
            .get_value::<String, _>("Version")
            .ok()
            .filter(|v| !v.trim().is_empty());
        let location = key
            .get_value::<String, _>("MainAppPath")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .and_then(|path| {
                PathBuf::from(path)
                    .parent()
                    .map(|p| p.to_string_lossy().to_string())
            });

        if version.is_some() || location.is_some() {
            return Installed { version, location };
        }
    }

    for root in [
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
        r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
    ] {
        if let Ok(uninstall) = hklm.open_subkey_with_flags(root, KEY_READ) {
            for name in uninstall.enum_keys().flatten() {
                if let Ok(sub) = uninstall.open_subkey(&name) {
                    let display: String = sub.get_value("DisplayName").unwrap_or_default();
                    if !display.contains("Logi Options+") && !display.contains("LogiOptionsPlus") {
                        continue;
                    }

                    return Installed {
                        version: sub
                            .get_value::<String, _>("DisplayVersion")
                            .ok()
                            .filter(|v| !v.trim().is_empty()),
                        location: sub
                            .get_value::<String, _>("InstallLocation")
                            .ok()
                            .filter(|v| !v.trim().is_empty()),
                    };
                }
            }
        }
    }

    let fallback = program_files().join("LogiOptionsPlus");
    if fallback.join("LogiOptionsPlus.exe").exists() {
        return Installed {
            version: None,
            location: Some(fallback.to_string_lossy().to_string()),
        };
    }

    Installed {
        version: None,
        location: None,
    }
}

/// 优先用产品自己注册的卸载命令（注册表 UninstallString 里那条
/// `logioptionsplus_updater.exe --uninstall --full --shadow`）。
/// 实测安装包的 `/uninstall` 在已装环境里会崩（0xC0000005），产品自带卸载器才靠得住。
pub fn resolve_uninstaller(location: Option<&str>) -> Option<PathBuf> {
    let candidates = location
        .map(PathBuf::from)
        .into_iter()
        .chain(std::iter::once(program_files().join("LogiOptionsPlus")));

    for dir in candidates {
        let updater = dir.join("logioptionsplus_updater.exe");
        if updater.exists() {
            return Some(updater);
        }
    }

    None
}

/// 结束 Logi Options+ 自身进程。刻意只匹配 "logioptionsplus"，
/// 不碰 Logi Bolt / Unifying，也排除本工具自己。
pub fn stop_logitech_processes(log: &dyn Fn(String)) -> usize {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let Ok(output) = std::process::Command::new("tasklist")
        .args(["/FO", "CSV", "/NH"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
    else {
        log("无法枚举进程（tasklist 调用失败），跳过清理。".to_string());
        return 0;
    };

    let self_pid = std::process::id();
    let text = String::from_utf8_lossy(&output.stdout);
    let mut killed = 0;

    for line in text.lines() {
        let mut fields = line.split("\",\"");
        let raw_name = fields.next().unwrap_or_default().trim_start_matches('"');
        let raw_pid = fields.next().unwrap_or_default().trim_end_matches('"');

        if !raw_name.to_ascii_lowercase().contains("logioptionsplus") {
            continue;
        }

        let Ok(pid) = raw_pid.parse::<u32>() else {
            continue;
        };
        if pid == self_pid {
            continue;
        }

        log(format!("结束进程：{raw_name} (PID {pid})"));

        let _ = std::process::Command::new("taskkill")
            .args(["/F", "/PID", &pid.to_string()])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        killed += 1;
    }

    if killed == 0 {
        log("没有正在运行的 Logi Options+ 进程。".to_string());
    }

    killed
}

/// 系统是深色还是浅色（跟随「设置 → 个性化 → 颜色」）。
pub fn system_uses_dark_theme() -> bool {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize")
        .and_then(|key| key.get_value::<u32, _>("AppsUseLightTheme"))
        .map(|light| light == 0)
        .unwrap_or(false)
}

