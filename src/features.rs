//! 功能开关清单，与 C# 版 `FeatureCatalog` 对齐。
//! 顺序、键名、默认值都按官方安装器 `/help` 实测结果来，不要凭印象改。

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Edition {
    Online,
    Offline,
}

pub struct Feature {
    pub key: &'static str,
    pub name: &'static str,
    pub tip: &'static str,
    /// 在线版安装器的默认值
    pub default_online: bool,
    /// 官方离线版的默认值（实测除固件更新、背光外全为「无」）
    pub default_offline: bool,
    /// 当前官方安装器是否还接受这个参数。aipromptbuilder 已被移除，传了会卡住。
    pub supported: bool,
}

pub const FEATURES: &[Feature] = &[
    Feature {
        key: "analytics",
        name: "数据上报",
        tip: "把你的键鼠使用情况和错误报告发给罗技服务器",
        default_online: true,
        default_offline: false,
        supported: true,
    },
    Feature {
        key: "flow",
        name: "Flow 跨屏",
        tip: "局域网内一套键鼠操控多台电脑，需要的开",
        default_online: true,
        default_offline: false,
        supported: true,
    },
    Feature {
        key: "sso",
        name: "账户登录",
        tip: "登录罗技账号，在不同电脑间同步设置，需要的开",
        default_online: true,
        default_offline: false,
        supported: true,
    },
    Feature {
        key: "update",
        name: "自动更新",
        tip: "Logi Options+ 自动升级为官方版，别开",
        default_online: true,
        default_offline: false,
        supported: true,
    },
    Feature {
        key: "dfu",
        name: "固件更新",
        tip: "自动升级你的鼠标键盘固件，可以开",
        default_online: true,
        default_offline: true,
        supported: true,
    },
    Feature {
        key: "backlight",
        name: "键盘背光",
        tip: "调键盘灯光的，有背光的开",
        default_online: true,
        default_offline: true,
        supported: true,
    },
    Feature {
        key: "logivoice",
        name: "罗技语音（LogiVoice）",
        tip: "罗技的游戏语音客户端，没用的东西",
        default_online: true,
        default_offline: false,
        supported: true,
    },
    Feature {
        key: "aipromptbuilder",
        name: "AI 提示词（AI Prompt Builder）",
        // 官方安装器已经不接受这个参数了，装了就是开着的，所以两个版本都按「开启」展示
        tip: "官方安装器不再接受这个开关，装完就是开着的，关不掉",
        default_online: true,
        default_offline: true,
        supported: false,
    },
    Feature {
        key: "device-recommendation",
        name: "新设备推荐",
        tip: "应用里推送罗技新品的消息（又名：广告），别开",
        default_online: true,
        default_offline: false,
        supported: true,
    },
    Feature {
        key: "smartactions",
        name: "按键宏（Smart Actions）",
        tip: "将一系列按键或鼠标操作录制为一个快捷键，需要的开",
        default_online: true,
        default_offline: false,
        supported: true,
    },
    Feature {
        key: "actions-ring",
        name: "快捷轮盘（Actions Ring）",
        tip: "圆形快捷操作菜单，不好用",
        default_online: true,
        default_offline: false,
        supported: true,
    },
];

pub fn preset(edition: Edition) -> Vec<bool> {
    FEATURES
        .iter()
        .map(|f| match edition {
            Edition::Online => f.default_online,
            Edition::Offline => f.default_offline,
        })
        .collect()
}

/// 只勾选指定的几项，其余全关。用于命令行验证不同组合。
pub fn select(keys: &[&str]) -> Vec<bool> {
    FEATURES
        .iter()
        .map(|feature| keys.iter().any(|key| key.eq_ignore_ascii_case(feature.key)))
        .collect()
}

/// 当前勾选是不是「正好是离线版那个形态」。
/// 差 2 项以内就算——用户手动微调一两项不必因此失去推荐。
pub fn is_close_to_offline(values: &[bool], tolerance: usize) -> bool {
    let baseline = preset(Edition::Offline);
    let difference = FEATURES
        .iter()
        .enumerate()
        .filter(|(index, _)| {
            let mine = values.get(*index).copied().unwrap_or(false);
            let theirs = baseline.get(*index).copied().unwrap_or(false);
            mine != theirs
        })
        .count();

    difference <= tolerance
}

/// 拼装传给官方安装器的参数。顺序与脚本一致：quiet 打头，其后逐项 /key Yes|No。
pub fn build_args(quiet: bool, values: &[bool], log_dir: Option<&str>) -> Vec<String> {
    let mut args = Vec::new();
    if quiet {
        args.push("/quiet".to_string());
    }

    for (feature, enabled) in FEATURES.iter().zip(values) {
        if !feature.supported {
            continue;
        }
        args.push(format!("/{}", feature.key));
        args.push(if *enabled { "Yes" } else { "No" }.to_string());
    }

    if let Some(dir) = log_dir {
        args.push("/log".to_string());
        args.push(dir.to_string());
    }

    args
}

/// Windows 命令行引用规则：含空格或引号才加引号。
pub fn quote(value: &str) -> String {
    if value.is_empty() {
        return "\"\"".to_string();
    }
    if value.contains([' ', '\t', '"']) {
        format!("\"{}\"", value.replace('"', "\\\""))
    } else {
        value.to_string()
    }
}

pub fn to_command_line(args: &[String]) -> String {
    args.iter().map(|a| quote(a)).collect::<Vec<_>>().join(" ")
}
