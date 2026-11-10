//! Logi Options+ 精简安装器（Rust 版）
//!
//! 三条流程：在线版「先卸载再装」、离线版「覆盖安装」、以及安装前的配置备份/还原。
//!
//! 命令行自检（不进 GUI）：
//!   --check          只读版本清单，对比本机版本
//!   --verify         下载安装包并校验签名，不执行安装
//!   --plan           打印安装计划（卸载方式、备份路径、安装参数），不动系统
//!   --install        真正执行安装；可叠加 --offline / --no-backup / --interactive / --keep

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod features;
mod net;
mod profile;
mod service;
mod winapi;

use std::cell::RefCell;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Duration;

use eframe::egui;

use features::{preset, Edition, Feature, FEATURES};
use profile::Settings;
use service::InstallRequest;

enum Cmd {
    CheckUpdate,
    Install(InstallRequest),
}

enum Msg {
    Log(String),
    Progress {
        label: String,
        fraction: Option<f32>,
    },
    Manifest {
        local: Option<String>,
        remote: Option<String>,
    },
    Done,
}

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if !args.is_empty() {
        run_cli(&args);
        return Ok(());
    }

    let (cmd_tx, cmd_rx) = channel::<Cmd>();
    let (msg_tx, msg_rx) = channel::<Msg>();

    std::thread::spawn(move || worker(cmd_rx, msg_tx));

    let settings = profile::load();
    let dark = settings
        .dark_theme
        .unwrap_or_else(winapi::system_uses_dark_theme);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([820.0, 560.0])
            .with_min_inner_size([680.0, 500.0])
            .with_title("Logi Options+ 精简安装器"),
        ..Default::default()
    };

    eframe::run_native(
        "Logi Options+ 精简安装器",
        options,
        Box::new(move |cc| {
            install_cjk_font(&cc.egui_ctx);
            cc.egui_ctx.set_visuals(if dark {
                egui::Visuals::dark()
            } else {
                egui::Visuals::light()
            });
            enlarge_text(&cc.egui_ctx);
            Ok(Box::new(App::new(cmd_tx, msg_rx, settings, dark)))
        }),
    )
}

/// 默认字号在高分屏上偏小，整体上调一档。
/// 用 `all_styles_mut` 同时改深浅两套样式，切换主题时字号不会掉回去。
fn enlarge_text(ctx: &egui::Context) {
    use egui::{FontFamily, FontId, TextStyle};

    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Heading, FontId::new(22.0, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(16.0, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(15.5, FontFamily::Proportional)),
            (TextStyle::Small, FontId::new(13.0, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(13.5, FontFamily::Monospace)),
        ]
        .into();
    });
}

/// 中文必须靠系统字体：内嵌一份中文字体会让 exe 胖 8~10MB，与瘦身目标相反。
fn install_cjk_font(ctx: &egui::Context) {
    let candidates = [
        (r"C:\Windows\Fonts\msyh.ttc", 0u32),
        (r"C:\Windows\Fonts\msyh.ttf", 0),
        (r"C:\Windows\Fonts\simhei.ttf", 0),
        (r"C:\Windows\Fonts\Deng.ttf", 0),
    ];

    for (path, index) in candidates {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };

        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "cjk".to_owned(),
            std::sync::Arc::new(egui::FontData::from_blob(std::sync::Arc::new(bytes), index)),
        );
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts
                .families
                .entry(family)
                .or_default()
                .push("cjk".to_owned());
        }
        ctx.set_fonts(fonts);
        return;
    }
}

// ---------------------------------------------------------------- 命令行自检

fn run_cli(args: &[String]) {
    let buffer = RefCell::new(Vec::<String>::new());
    let log = |line: String| buffer.borrow_mut().push(line);

    let has = |flag: &str| args.iter().any(|a| a == flag);
    let offline = has("--offline");

    // --features dfu,backlight 只开这两项，其余全关；不传则用对应版本默认预设
    let selected: Option<Vec<String>> = args
        .iter()
        .position(|a| a == "--features")
        .and_then(|index| args.get(index + 1))
        .map(|spec| spec.split(',').map(str::to_string).collect());

    let request = || InstallRequest {
        offline,
        quiet: !has("--interactive"),
        features: match &selected {
            Some(keys) => features::select(&keys.iter().map(String::as_str).collect::<Vec<_>>()),
            None => preset(if offline {
                Edition::Offline
            } else {
                Edition::Online
            }),
        },
        backup_config: !has("--no-backup"),
        cover_install: has("--cover") || offline,
        keep_installer: has("--keep"),
        reuse_installer: has("--reuse"),
        dry_run: has("--plan"),
    };

    let progress = |fraction: Option<f32>, label: String| {
        if label.is_empty() {
            return;
        }
        let percent = fraction
            .map(|f| format!(" {:.0}%", f * 100.0))
            .unwrap_or_default();
        buffer.borrow_mut().push(format!("  … {label}{percent}"));
    };

    let mut exit_code = 0;

    if has("--check") {
        check_update(&|msg| match msg {
            Msg::Log(line) => buffer.borrow_mut().push(line),
            _ => {}
        });
    }

    if has("--plan") {
        log("安装计划：".to_string());
        for line in service::describe_plan(&request()) {
            log(format!("  · {line}"));
        }
    }

    if has("--backup-test") {
        log(format!("配置目录：{}", winapi::config_dir().display()));
        match profile::backup_config(&log) {
            Ok(files) => {
                log(format!("备份成功：{files} 个文件 → {}", winapi::backup_dir().display()));
                match profile::restore_config(&log) {
                    Ok(_) => log("还原成功，配置目录已回到原位。".to_string()),
                    Err(error) => log(format!("还原失败：{error}")),
                }
            }
            Err(reason) => log(reason),
        }
    }

    if has("--verify") || has("--install") {
        let outcome = service::run(&request(), &log, &progress);
        log(if outcome.success { "✓ " } else { "✗ " }.to_string() + &outcome.message);
        if !outcome.success && !outcome.cancelled {
            exit_code = 1;
        }
    }

    let text = buffer.borrow().join("\n");
    println!("{text}");

    let dir = winapi::work_dir();
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(dir.join("cli.log"), text.as_bytes());

    std::process::exit(exit_code);
}

// ---------------------------------------------------------------- 界面

struct App {
    cmd_tx: Sender<Cmd>,
    msg_rx: Receiver<Msg>,
    settings: Settings,
    local_version: Option<String>,
    remote_version: Option<String>,
    logs: Vec<String>,
    progress: Option<f32>,
    progress_label: String,
    busy: bool,
    show_log: bool,
    dark: bool,
}

impl App {
    fn new(cmd_tx: Sender<Cmd>, msg_rx: Receiver<Msg>, settings: Settings, dark: bool) -> Self {
        let installed = winapi::detect_installed();
        Self {
            cmd_tx,
            msg_rx,
            settings,
            local_version: installed.version,
            remote_version: None,
            logs: vec!["就绪。安装包直接取自罗技官方服务器，下载后会校验数字签名。".to_string()],
            progress: None,
            progress_label: String::new(),
            busy: false,
            show_log: false,
            dark,
        }
    }

    fn request(&self) -> InstallRequest {
        InstallRequest {
            offline: self.settings.offline,
            quiet: self.settings.quiet,
            features: self.settings.features.clone(),
            backup_config: self.settings.backup_config,
            cover_install: self.settings.cover_install || self.settings.offline,
            keep_installer: self.settings.keep_installer,
            reuse_installer: self.settings.reuse_installer,
            dry_run: false,
        }
    }

    fn pump_messages(&mut self) {
        while let Ok(msg) = self.msg_rx.try_recv() {
            match msg {
                Msg::Log(line) => {
                    self.logs.push(line);
                    if self.logs.len() > 500 {
                        self.logs.drain(0..100);
                    }
                }
                Msg::Progress { label, fraction } => {
                    self.progress_label = label;
                    self.progress = fraction;
                }
                Msg::Manifest { local, remote } => {
                    if local.is_some() {
                        self.local_version = local;
                    }
                    if remote.is_some() {
                        self.remote_version = remote;
                    }
                }
                Msg::Done => {
                    self.busy = false;
                    self.progress = None;
                    self.progress_label.clear();
                }
            }
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.pump_messages();

        if self.busy {
            ui.ctx().request_repaint_after(Duration::from_millis(120));
        }

        let fill = ui.style().visuals.panel_fill;

        // App::ui 给的是一块裸 Ui，不铺底色的话窗口剩余区域会是黑的
        ui.painter().rect_filled(ui.max_rect(), 0.0, fill);

        egui::Frame::new()
            .inner_margin(egui::Margin::same(16))
            .fill(fill)
            .show(ui, |ui| {
                header(ui, self);
                ui.add_space(10.0);

                version_row(ui, self);
                ui.add_space(10.0);

                actions(ui, self);
                ui.add_space(10.0);
                ui.separator();
                ui.add_space(8.0);

                options_row(ui, self);
                ui.add_space(10.0);
                feature_grid(ui, self);

                if let Some(fraction) = self.progress {
                    ui.add_space(8.0);
                    ui.add(
                        egui::ProgressBar::new(fraction)
                            .show_percentage()
                            .text(self.progress_label.clone()),
                    );
                }

                ui.add_space(8.0);
                log_panel(ui, self);
            });
    }
}

fn header(ui: &mut egui::Ui, app: &mut App) {
    ui.horizontal(|ui| {
        ui.heading("Logi Options+ 精简安装器");
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new("只装你要的，其余全部关掉")
                .weak()
                .size(12.0),
        );

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // 用文字而不是 ☀/☾：系统字体里不一定有这两个字形，会渲染成小方块
            let label = if app.dark { "浅色" } else { "深色" };
            if ui
                .button(label)
                .on_hover_text("切换深浅色（默认跟随系统）")
                .clicked()
            {
                app.dark = !app.dark;
                app.settings.dark_theme = Some(app.dark);
                ui.ctx().set_visuals(if app.dark {
                    egui::Visuals::dark()
                } else {
                    egui::Visuals::light()
                });
                profile::save(&app.settings);
            }
        });
    });
}

fn version_row(ui: &mut egui::Ui, app: &App) {
    ui.horizontal(|ui| {
        ui.label("本机版本：");
        match &app.local_version {
            Some(version) => ui.label(egui::RichText::new(version).strong()),
            None => ui.label(egui::RichText::new("未检测到").weak()),
        };
        ui.add_space(20.0);
        ui.label("官方最新：");
        match &app.remote_version {
            Some(version) => ui.label(egui::RichText::new(version).strong()),
            None => ui.label(egui::RichText::new("未检查").weak()),
        };
    });
}

fn actions(ui: &mut egui::Ui, app: &mut App) {
    ui.horizontal(|ui| {
        if ui
            .add_enabled(!app.busy, egui::Button::new("检查更新"))
            .clicked()
        {
            app.busy = true;
            app.progress = None;
            let _ = app.cmd_tx.send(Cmd::CheckUpdate);
        }

        if ui
            .add_enabled(!app.busy, egui::Button::new("开始安装"))
            .clicked()
        {
            let request = app.request();
            app.busy = true;
            app.show_log = true;
            profile::save(&app.settings);
            let _ = app.cmd_tx.send(Cmd::Install(request));
        }

        if ui
            .button("打开下载目录")
            .on_hover_text("安装包下载到这里的缓存目录")
            .clicked()
        {
            let dir = winapi::cache_dir();
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::process::Command::new("explorer").arg(&dir).spawn();
        }
    });
}

fn options_row(ui: &mut egui::Ui, app: &mut App) {
    ui.label(egui::RichText::new("安装源").strong());

    let mut changed = false;

    // 一行一个版本，下载和装完两个数字都写清楚：
    // 只写「装完 750MB」挂在在线版后面，很容易被读成在线版更省空间。
    // 数字写「左右」：官方一更新就会变，写死反而显得工具在说谎。
    egui::Grid::new("edition-grid")
        .num_columns(2)
        .spacing([14.0, 6.0])
        .show(ui, |ui| {
            changed |= ui
                .radio_value(&mut app.settings.offline, false, "在线版")
                .on_hover_text(
                    "官方安装器本体只有 50MB，但组件是装的时候联网拉的，装完反而最大",
                )
                .changed();
            ui.label(
                egui::RichText::new("官方安装器本体 50MB → 装完 750MB 左右")
                    .weak()
                    .size(13.5),
            );
            ui.end_row();

            changed |= ui
                .radio_value(&mut app.settings.offline, true, "离线版")
                .on_hover_text(
                    "罗技官方出的「去掉需联网功能版本」，安装器本体 700MB，\
                     装完反而更小，而且以后不联网也能用",
                )
                .changed();
            ui.label(
                egui::RichText::new("官方安装器本体 700MB → 装完 500MB 左右")
                    .weak()
                    .size(13.5),
            );
            ui.end_row();
        });

    if changed {
        app.settings.features = preset(if app.settings.offline {
            Edition::Offline
        } else {
            Edition::Online
        });
        profile::save(&app.settings);
    }

    // 数字都摆出来了，还得把结论和离线版的定位说明白，否则用户要自己算
    ui.add(
        egui::Label::new(
            egui::RichText::new(
                "离线版是罗技官方出的「去掉需联网功能版本」，装完占用更小（500MB 左右，\
                 对比在线版的 750MB 左右），但更新会比在线版晚几周。",
            )
            .weak()
            .size(13.0),
        )
        .wrap(),
    );

    let mut dirty = false;

    ui.horizontal_wrapped(|ui| {
        dirty |= ui
            .checkbox(&mut app.settings.quiet, "静默安装")
            .on_hover_text("不弹安装向导，全程无人值守")
            .changed();

        ui.add_space(16.0);

        dirty |= ui
            .checkbox(&mut app.settings.backup_config, "保住现有键鼠配置")
            .on_hover_text("安装前备份按键与鼠标自定义，装完自动还原")
            .changed();

        ui.add_space(16.0);

        if !app.settings.offline {
            dirty |= ui
                .checkbox(&mut app.settings.cover_install, "直接覆盖")
                .on_hover_text("不卸载旧版直接覆盖安装。适合只想快速改开关的情况")
                .changed();
            ui.add_space(16.0);
        }

        dirty |= ui
            .checkbox(&mut app.settings.keep_installer, "安装后保留安装包")
            .on_hover_text("装完不删下载目录里的安装包，方便重复安装")
            .changed();
    });

    if dirty {
        profile::save(&app.settings);
    }

    offline_suggestion(ui, app);
}

/// 用户把开关收得很干净时，提示换成离线版——这类人多半正要的正是离线版。
fn offline_suggestion(ui: &mut egui::Ui, app: &mut App) {
    if app.settings.offline || !features::is_close_to_offline(&app.settings.features, 2) {
        return;
    }

    ui.add_space(6.0);
    let fill = ui.visuals().faint_bg_color;
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(10, 6))
        .corner_radius(egui::CornerRadius::same(6))
        .fill(fill)
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("你选的很少，正好是离线版的形态，装完还能断网用。");
                if ui.button("改用离线版").clicked() {
                    app.settings.offline = true;
                    profile::save(&app.settings);
                }
            });
        });
}

fn feature_grid(ui: &mut egui::Ui, app: &mut App) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("要启用哪些功能").strong());
        ui.label(
            egui::RichText::new("勾上 = 启用，不勾 = 关闭")
                .weak()
                .size(13.0),
        );

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .button("恢复默认配置")
                .on_hover_text("把下面所有开关一键恢复成官方的默认值（按当前安装源）")
                .clicked()
            {
                app.settings.features = preset(if app.settings.offline {
                    Edition::Offline
                } else {
                    Edition::Online
                });
                profile::save(&app.settings);
            }
        });
    });
    ui.add_space(4.0);

    let mut values = std::mem::take(&mut app.settings.features);
    let mut dirty = false;

    ui.columns(2, |columns| {
        for (index, feature) in FEATURES.iter().enumerate() {
            let column = &mut columns[index % 2];
            dirty |= draw_feature(column, index, feature, &mut values);
        }
    });

    app.settings.features = values;
    if dirty {
        profile::save(&app.settings);
    }
}

/// 返回是否被改动。
fn draw_feature(ui: &mut egui::Ui, index: usize, feature: &Feature, values: &mut [bool]) -> bool {
    // 官方已经强制开启的项不画复选框：禁用的 checkbox 画不出勾，
    // 看起来像「没开」，和「强制开启」的说明自相矛盾。
    // 也不用 ✓ 之类的符号——微软雅黑里没有这个字形，会渲染成方块。
    if !feature.supported {
        ui.horizontal(|ui| {
            ui.add_space(18.0);
            ui.label(egui::RichText::new(feature.name).weak());
            ui.add_space(6.0);
            ui.label(egui::RichText::new("官方强制开启").small().weak())
                .on_hover_text(feature.tip);
        });
        return false;
    }

    let mut row_changed = false;

    ui.horizontal(|ui| {
        let response = ui.add(egui::Checkbox::new(&mut values[index], feature.name));

        row_changed |= response.changed();
        response.on_hover_text(feature.tip);
    });

    row_changed
}

fn log_panel(ui: &mut egui::Ui, app: &mut App) {
    // 收起时把开关贴到窗口底部，避免中间留一大块空白；展开时日志占满剩余空间。
    if !app.show_log {
        let spacer = (ui.available_height() - 24.0).max(0.0);
        ui.add_space(spacer);
    } else {
        ui.add_space(4.0);
    }

    ui.horizontal(|ui| {
        ui.checkbox(&mut app.show_log, "显示运行日志");

        if app.show_log && ui.small_button("打开日志目录").clicked() {
            let dir = winapi::work_dir();
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::process::Command::new("explorer").arg(&dir).spawn();
        }
    });

    if !app.show_log {
        return;
    }

    let height = (ui.available_height() - 8.0).max(120.0);
    egui::ScrollArea::vertical()
        .max_height(height)
        .auto_shrink([false, false])
        .stick_to_bottom(true)
        .show(ui, |ui| {
            for line in &app.logs {
                ui.label(egui::RichText::new(line).monospace().size(12.0));
            }
        });
}

// ---------------------------------------------------------------- 后台任务

fn worker(rx: Receiver<Cmd>, tx: Sender<Msg>) {
    while let Ok(cmd) = rx.recv() {
        match cmd {
            Cmd::CheckUpdate => {
                let emit = |msg: Msg| {
                    let _ = tx.send(msg);
                };
                check_update(&emit);
            }
            Cmd::Install(request) => {
                let log = |line: String| {
                    let _ = tx.send(Msg::Log(line));
                };
                let progress = |fraction: Option<f32>, label: String| {
                    let _ = tx.send(Msg::Progress { label, fraction });
                };

                let outcome = service::run(&request, &log, &progress);
                let _ = tx.send(Msg::Log(
                    if outcome.success { "✓ " } else { "✗ " }.to_string() + &outcome.message,
                ));

                let installed = winapi::detect_installed();
                let _ = tx.send(Msg::Manifest {
                    local: installed.version,
                    remote: None,
                });
            }
        }

        let _ = tx.send(Msg::Done);
    }
}

fn check_update(emit: &dyn Fn(Msg)) {
    let local = winapi::detect_installed().version;
    emit(Msg::Log(format!(
        "本机已安装：{}",
        local.clone().unwrap_or_else(|| "未检测到".into())
    )));

    match net::fetch_manifest() {
        Ok(manifest) => {
            emit(Msg::Log(format!(
                "官方清单：版本 {}，最后修改 {}",
                manifest.version.clone().unwrap_or_else(|| "未知".into()),
                manifest.last_modified.unwrap_or_else(|| "未知".into())
            )));

            if let (Some(remote), Some(local)) = (&manifest.version, &local) {
                let verdict = match service::compare_versions(remote, local) {
                    1 => format!("有新版本 {remote}（当前 {local}）"),
                    0 => format!("已是最新（{local}）"),
                    _ => format!("官方版本 {remote} 比本机 {local} 旧"),
                };
                emit(Msg::Log(verdict));
            }

            emit(Msg::Manifest {
                local,
                remote: manifest.version,
            });
        }
        Err(error) => {
            emit(Msg::Log(format!("读取官方清单失败：{error}")));
            emit(Msg::Manifest { local, remote: None });
        }
    }
}
