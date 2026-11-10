//! 网络层：官方版本清单 + 带进度的安装包下载。
//! 用 ureq（阻塞 + 系统证书链），不引 reqwest/tokio，省体积也省编译时间。

use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;

use serde_json::Value;

pub const UPDATE_MANIFEST_URL: &str =
    "https://updates.optionsplus.logitechg.com/pipeline/v2/update/optionsplus5/win/public/update.json";
pub const INSTALLER_INTL: &str =
    "https://download01.logi.com/web/ftp/pub/techsupport/optionsplus/logioptionsplus_installer.exe";
pub const INSTALLER_CN: &str =
    "https://download.logitech.com.cn/web/ftp/pub/techsupport/optionsplus/logioptionsplus_installer.exe";
pub const OFFLINE_INTL: &str = "https://download01.logi.com/web/ftp/pub/techsupport/optionsplus/logioptionsplus_installer_offline.exe";
pub const OFFLINE_CN: &str = "https://download.logitech.com.cn/web/ftp/pub/techsupport/optionsplus/logioptionsplus_installer_offline.exe";

const UA: &str = "LogiOptionsPlusMini/0.1 (Rust validation shell)";

fn build_agent(timeout_secs: u64) -> ureq::Agent {
    let mut builder = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(timeout_secs))
        .user_agent(UA);

    if let Some(proxy) = proxy_from_env() {
        builder = builder.proxy(proxy);
    }

    builder.build()
}

/// 支持 HTTP_PROXY / HTTPS_PROXY —— 企业网络和这次沙箱环境都需要。
fn proxy_from_env() -> Option<ureq::Proxy> {
    let raw = std::env::var("HTTPS_PROXY")
        .or_else(|_| std::env::var("https_proxy"))
        .or_else(|_| std::env::var("HTTP_PROXY"))
        .or_else(|_| std::env::var("http_proxy"))
        .ok()?;

    if raw.trim().is_empty() {
        return None;
    }

    ureq::Proxy::new(raw.trim()).ok()
}

pub struct Manifest {
    pub version: Option<String>,
    pub last_modified: Option<String>,
}

/// 官方公开的更新清单，无需鉴权，是唯一可靠的「最新版本」来源。
pub fn fetch_manifest() -> Result<Manifest, String> {
    let agent = build_agent(20);
    let text = agent
        .get(UPDATE_MANIFEST_URL)
        .call()
        .map_err(|e| format!("请求失败：{e}"))?
        .into_string()
        .map_err(|e| format!("读取响应失败：{e}"))?;

    let json: Value = serde_json::from_str(&text).map_err(|e| format!("清单不是合法 JSON：{e}"))?;

    Ok(Manifest {
        version: json
            .get("version")
            .and_then(Value::as_str)
            .map(str::to_string),
        last_modified: json
            .get("lastModified")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

/// Cloudflare trace 判地区，与上游脚本同一招。失败时按国际源处理。
pub fn detect_china() -> bool {
    let agent = build_agent(8);
    match agent.get("https://www.cloudflare.com/cdn-cgi/trace").call() {
        Ok(resp) => resp
            .into_string()
            .map(|text| text.contains("loc=CN"))
            .unwrap_or(false),
        Err(_) => false,
    }
}

pub fn installer_url(china: bool) -> &'static str {
    if china {
        INSTALLER_CN
    } else {
        INSTALLER_INTL
    }
}

pub fn offline_url(china: bool) -> &'static str {
    if china {
        OFFLINE_CN
    } else {
        OFFLINE_INTL
    }
}

/// 下载到本地，边下边回报进度。返回实际字节数。
pub fn download<F>(url: &str, dest: &Path, mut on_progress: F) -> Result<u64, String>
where
    F: FnMut(u64, Option<u64>),
{
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建缓存目录失败：{e}"))?;
    }

    let agent = build_agent(60);
    let response = agent.get(url).call().map_err(|e| format!("下载失败：{e}"))?;

    let total = response
        .header("Content-Length")
        .and_then(|v| v.parse::<u64>().ok());
    let mut reader = response.into_reader();
    let mut file = File::create(dest).map_err(|e| format!("写入文件失败：{e}"))?;

    let mut buffer = vec![0u8; 128 * 1024];
    let mut received: u64 = 0;
    let mut last_report: u64 = 0;

    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|e| format!("读取数据流失败：{e}"))?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read])
            .map_err(|e| format!("写入文件失败：{e}"))?;
        received += read as u64;

        if received - last_report >= 1024 * 1024 || Some(received) == total {
            on_progress(received, total);
            last_report = received;
        }
    }

    file.flush().map_err(|e| format!("刷新文件失败：{e}"))?;

    if let Some(expected) = total {
        if received != expected {
            let _ = std::fs::remove_file(dest);
            return Err(format!(
                "下载不完整：期望 {expected} 字节，实际 {received} 字节"
            ));
        }
    }

    Ok(received)
}
