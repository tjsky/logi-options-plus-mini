# Script to install latest Logi Options+ on Windows (Multi-language Support)
# Fixed encoding
# Region Detection & Language Setup 
$isCN = $false
$traceUrl = "https://cloudflare.com/cdn-cgi/trace"

Write-Host "Detecting region..." -NoNewline
try {
    # Attempt to detect region
    $traceResponse = Invoke-RestMethod -Uri $traceUrl -TimeoutSec 5
    if ($traceResponse -match "loc=CN") {
        $isCN = $true
        Write-Host " [China Detected]" -ForegroundColor Green
    } else {
        Write-Host " [International Detected]" -ForegroundColor Green
    }
} catch {
    Write-Host " [Detection Failed, defaulting to International]" -ForegroundColor Yellow
}

# Language Setup 
$txt = @{}

if ($isCN) {
    # --- Chinese (CN) ---
    $txt.TitleStart        = "开始安装"
    $txt.DownloadUrlMsg    = "检测到您位于中国地区，使用 CN 下载源"
    $txt.DownloadDefMsg    = "使用默认下载源"
    $txt.Downloading       = "正在下载安装包:"
    $txt.DownloadSuccess   = "下载成功。"
    $txt.DownloadFail      = "下载安装包失败。"
    $txt.SelectKeep        = "请选择你想保留/开启的功能："
    $txt.Menu0             = "0.  quiet:               静默安装（无人值守安装）"
    $txt.Menu1             = "1.  analytics:           允许收集个人使用数据和诊断信息"
    $txt.Menu2             = "2.  flow:                Flow 跨屏功能 (默认为 Yes)"
    $txt.Menu3             = "3.  sso:                 启用 罗技账户 登录模块"
    $txt.Menu4             = "4.  update:              启用 Logi Options+ 自动更新"
    $txt.Menu5             = "5.  dfu:                 启用 设备固件 自动更新"
    $txt.Menu6             = "6.  backlight:           启用 键盘背光 支持"
    $txt.Menu7             = "7.  logivoice:           启用 罗技语音功能（LogiVoice）"
    $txt.Menu8             = "8.  aipromptbuilder:     启用 AI 功能"
    $txt.Menu9             = "9.  device-recommendation: 启用 新产品推荐（新产品广告）"
    $txt.Menu10            = "10. smartactions:        启用 Smart Actions (按键宏)"
    $txt.Menu11            = "11. actions-ring:        启用 Actions Ring（快捷启动）"
    $txt.Menu12            = "12. all (全部开启)"
    $txt.MenuNone          = "直接按回车键，则执行最小化精简安装，不开启任何额外功能"
    $txt.InputPrompt       = "如需自定义请输入选项 (例如 '0 2 6 10', 默认为 最小化精简安装)"
    $txt.ConfirmTitle      = "请确认以下设置:"
    $txt.ConfirmPrompt     = "设置是否正确？请输入： [y/n] (默认: y)"
    $txt.InstallCancel     = "安装已取消。"
    $txt.BackupStart       = "正在备份现有配置..."
    $txt.BackupDone        = "配置已备份至"
    $txt.UninstallStart    = "正在卸载旧版本..."
    $txt.RestoreStart      = "正在从备份恢复配置..."
    $txt.RestoreDone       = "配置已恢复。"
    $txt.InstallStart      = "正在安装新版本..."
    $txt.InstallSuccess    = "安装成功。"
    $txt.Cleanup           = "清理临时文件完成。"
    $txt.ExitMsg           = "安装完成，按任意键退出..."
    $txt.InstallFail       = "安装失败。退出提示:"
} else {
    # --- English (EN) ---
    $txt.TitleStart        = "Starting install of"
    $txt.DownloadUrlMsg    = "Detected region: China, using CN download URL"
    $txt.DownloadDefMsg    = "Using default download URL"
    $txt.Downloading       = "Downloading Installer from:"
    $txt.DownloadSuccess   = "Download completed successfully."
    $txt.DownloadFail      = "Failed to download installer."
    $txt.SelectKeep        = "Please select the features you want to keep:"
    $txt.Menu0             = "0.  quiet:               Install the application silently without UI."
    $txt.Menu1             = "1.  analytics:           Shows or hides choice for users to opt in to share app usage and diagnostics data."
    $txt.Menu2             = "2.  flow:                Shows or hides the Flow feature. Default value is Yes"
    $txt.Menu3             = "3.  sso:                 Shows or hides ability for users to sign into the app."
    $txt.Menu4             = "4.  update:              Enables or disables app updates."
    $txt.Menu5             = "5.  dfu:                 Enables or disables device firmware updates."
    $txt.Menu6             = "6.  backlight:           Enables or disables keyboard backlight on the supported keyboards."
    $txt.Menu7             = "7.  logivoice:           Enables or disables LogiVoice feature."
    $txt.Menu8             = "8.  aipromptbuilder:     Enables or disables AI Prompt Builder feature."
    $txt.Menu9             = "9.  device-recommendation: Enables or disables device recommendation feature."
    $txt.Menu10            = "10. smartactions:        Enables or disables Smart Actions feature."
    $txt.Menu11            = "11. actions-ring:        Enables or disables Actions Ring feature."
    $txt.Menu12            = "12. all"
    $txt.MenuNone          = "Press enter for none"
    $txt.InputPrompt       = "Enter your choices (e.g. 2 6, default is none)"
    $txt.ConfirmTitle      = "Please confirm the following settings:"
    $txt.ConfirmPrompt     = "Are these settings correct? [y/n](default: y)"
    $txt.InstallCancel     = "Installation cancelled."
    $txt.BackupStart       = "Backing up existing configuration..."
    $txt.BackupDone        = "Configuration backed up to"
    $txt.UninstallStart    = "Uninstalling existing version..."
    $txt.RestoreStart      = "Restoring configuration from backup..."
    $txt.RestoreDone       = "Configuration restored from"
    $txt.InstallStart      = "Installing..."
    $txt.InstallSuccess    = "installed successfully."
    $txt.Cleanup           = "Cleaned up temporary files."
    $txt.ExitMsg           = "Installation completed, press any key to exit..."
    $txt.InstallFail       = "Failed to install. Exit code:"
}

# Variables
$appName = "Logi Options+"
$installerName = "logioptionsplus_installer.exe"
$downloadUrl = "https://download01.logi.com/web/ftp/pub/techsupport/optionsplus/logioptionsplus_installer.exe"
$downloadUrlCN = "https://download.logitech.com.cn/web/ftp/pub/techsupport/optionsplus/logioptionsplus_installer.exe"
$downloadPath = "$env:TEMP\$installerName"
$configPath = "$env:LOCALAPPDATA\LogiOptionsPlus"
$backupPath = "$env:LOCALAPPDATA\LogiOptionsPlus_bak"

Write-Host ""
Write-Host "##############################################################"
Write-Host "$(Get-Date) | $($txt.TitleStart) $appName"
Write-Host "##############################################################"
Write-Host ""

# Determine URL based on pre-detected region
if ($isCN) {
    Write-Host "$(Get-Date) | $($txt.DownloadUrlMsg)"
    $selectedDownloadUrl = $downloadUrlCN
} else {
    Write-Host "$(Get-Date) | $($txt.DownloadDefMsg)"
    $selectedDownloadUrl = $downloadUrl
}

Write-Host "$(Get-Date) | $($txt.Downloading) $selectedDownloadUrl"
Invoke-WebRequest -Uri $selectedDownloadUrl -OutFile $downloadPath

# Check if download was successful
if (Test-Path $downloadPath) {
    Write-Host "$(Get-Date) | $($txt.DownloadSuccess)"
} else {
    Write-Host "$(Get-Date) | $($txt.DownloadFail) $appName"
    exit 1
}

# Interactive Feature Selection
Write-Host ""
Write-Host $txt.SelectKeep
Write-Host $txt.Menu0
Write-Host $txt.Menu1
Write-Host $txt.Menu2
Write-Host $txt.Menu3
Write-Host $txt.Menu4
Write-Host $txt.Menu5
Write-Host $txt.Menu6
Write-Host $txt.Menu7
Write-Host $txt.Menu8
Write-Host $txt.Menu9
Write-Host $txt.Menu10
Write-Host $txt.Menu11
Write-Host $txt.Menu12
Write-Host $txt.MenuNone
Write-Host ""

# Get user input for feature selection
$selectedFeatures = Read-Host $txt.InputPrompt
if ($selectedFeatures -eq "") {
    $selectedFeatures = "none"
}

# Initialize all options as "No"
$quiet = " "
$analytics = "No"
$flow = "No"
$sso = "No"
$update = "No"
$dfu = "No"
$backlight = "No"
$logivoice = "No"
$aipromptbuilder = "No"
$device_recommendation = "No"
$smartactions = "No"
$actions_ring = "No"

# Logic for feature flags (Kept same as original)
if ($selectedFeatures -eq "12") {
    $quiet = " "
    $analytics = "Yes"
    $flow = "Yes"
    $sso = "Yes"
    $update = "Yes"
    $dfu = "Yes"
    $backlight = "Yes"
    $logivoice = "Yes"
    $aipromptbuilder = "Yes"
    $device_recommendation = "Yes"
    $smartactions = "Yes"
    $actions_ring = "Yes"
} else {
    $featureList = $selectedFeatures -split " "
    foreach ($feature in $featureList) {
        switch ($feature) {
            "0" { $quiet = "/quiet" }
            "1" { $analytics = "Yes" }
            "2" { $flow = "Yes" }
            "3" { $sso = "Yes" }
            "4" { $update = "Yes" }
            "5" { $dfu = "Yes" }
            "6" { $backlight = "Yes" }
            "7" { $logivoice = "Yes" }
            "8" { $aipromptbuilder = "Yes" }
            "9" { $device_recommendation = "Yes" }
            "10" { $smartactions = "Yes" }
            "11" { $actions_ring = "Yes" }
        }
    }
}

# Confirm settings with the user
Write-Host ""
Write-Host $txt.ConfirmTitle
Write-Host "quiet:                $quiet"
Write-Host "analytics:            $analytics"
Write-Host "flow:                 $flow"
Write-Host "sso:                  $sso"
Write-Host "update:               $update"
Write-Host "dfu:                  $dfu"
Write-Host "backlight:            $backlight"
Write-Host "logivoice:            $logivoice"
Write-Host "aipromptbuilder:      $aipromptbuilder"
Write-Host "device-recommendation: $device_recommendation"
Write-Host "smartactions:         $smartactions"
Write-Host "actions-ring:         $actions_ring"
Write-Host ""

$confirm = Read-Host $txt.ConfirmPrompt
if ($confirm -ne "Y" -and $confirm -ne "y" -and $confirm -ne "") {
    Write-Host "$(Get-Date) | $($txt.InstallCancel)"
    exit 1
}

# Backup existing configuration
if (Test-Path $configPath) {
    Write-Host "$(Get-Date) | $($txt.BackupStart)"
    if (Test-Path $backupPath) {
        Remove-Item $backupPath -Recurse -Force
    }
    Copy-Item $configPath $backupPath -Recurse
    Write-Host "$(Get-Date) | $($txt.BackupDone) $backupPath"
}

# Uninstall existing version
Write-Host "$(Get-Date) | $($txt.UninstallStart) $appName..."
Start-Process -FilePath $downloadPath -ArgumentList "/uninstall" -Wait -Verb RunAs

# Restore configuration backup
if (Test-Path $backupPath) {
    Write-Host "$(Get-Date) | $($txt.RestoreStart)"
    if (Test-Path $configPath) {
        Remove-Item $configPath -Recurse -Force
    }
    Move-Item -Path $backupPath -Destination $configPath
    Write-Host "$(Get-Date) | $($txt.RestoreDone) $backupPath"
}

# Install new version
Write-Host "$(Get-Date) | $($txt.InstallStart) $appName..."
$installArgs = $quiet, "/analytics", $analytics, "/flow", $flow, "/sso", $sso, "/update", $update, "/dfu", $dfu, "/backlight", $backlight, "/logivoice", $logivoice, "/aipromptbuilder", $aipromptbuilder, "/device-recommendation", $device_recommendation, "/smartactions", $smartactions, "/actions-ring", $actions_ring
$process = Start-Process -FilePath $downloadPath -ArgumentList $installArgs -PassThru -Verb RunAs
$Handle = $process.Handle
$process.WaitForExit()

if ($process.ExitCode -eq 0) {
    Write-Host "$(Get-Date) | $appName $($txt.InstallSuccess)"
    # Clean up
    Remove-Item $downloadPath -Force
    Write-Host "$(Get-Date) | $($txt.Cleanup)"
    Write-Host $txt.ExitMsg
    [void][System.Console]::ReadKey($true)
    exit 0
} else {
    Write-Host "$(Get-Date) | $($txt.InstallFail) $($process.ExitCode)"
    Write-Host $txt.ExitMsg
    [void][System.Console]::ReadKey($true)
    exit 1
}
