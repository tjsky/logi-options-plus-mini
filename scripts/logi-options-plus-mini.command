#!/bin/bash
#set -x

############################################################################################
##
## Script to install latest Logi Options+ on macOS
##
############################################################################################

# Path.
package="logioptionsplus_installer"
unarchived_name="logioptionsplus_installer.app"
downloaded_path="/private/tmp/logioptionsplus"
downloaded_package_path="$downloaded_path/$package.zip"
package_unarchived_path="$downloaded_path/$unarchived_name"
weburl="https://download01.logi.com/web/ftp/pub/techsupport/optionsplus/logioptionsplus_installer.zip"
weburl_cn="https://download.logitech.com.cn/web/ftp/pub/techsupport/optionsplus/logioptionsplus_installer.zip"
appname="Logi Options+"
installer_name="logioptionsplus_installer"
install_path="$package_unarchived_path/Contents/MacOS/$installer_name"
config_path="$HOME/Library/Application Support/LogiOptionsPlus"
backup_path="$HOME/Library/Application Support/LogiOptionsPlus_bak"


#Region Detection & Language Setup
echo "Detecting region..."

IS_CN=0
selected_weburl="$weburl"

# Detect Region via Cloudflare with timeout
if trace_response=$(curl -s --connect-timeout 5 --max-time 5 "https://cloudflare.com/cdn-cgi/trace" 2>/dev/null); then
    if echo "$trace_response" | grep -q "loc=CN"; then
        IS_CN=1
        selected_weburl="$weburl_cn"
        echo " [China Detected] - Using CN download source."
    else
        echo " [International Detected] - Using default download source."
    fi
else
    echo " [Detection Failed] - Defaulting to International source."
fi


# Define Language Strings
if [ "$IS_CN" -eq 1 ]; then
    # --- Chinese (CN) ---
    TXT_START="开始安装"
    TXT_SELECT_KEEP="请选择你想保留/开启的功能："
    TXT_MENU_0="0.  quiet:               静默安装（无人值守安装）"
    TXT_MENU_1="1.  analytics:           允许收集个人使用数据和诊断信息"
    TXT_MENU_2="2.  flow:                启用 Flow 跨屏功能"
    TXT_MENU_3="3.  sso:                 启用 罗技账户 登录模块"
    TXT_MENU_4="4.  update:              启用 Logi Options+ 自动更新"
    TXT_MENU_5="5.  dfu:                 启用 设备固件 自动更新"
    TXT_MENU_6="6.  backlight:           启用 键盘背光 支持"
    TXT_MENU_7="7.  logivoice:           启用 罗技语音功能（LogiVoice）"
    TXT_MENU_8="8.  device-recommendation: 启用 新产品推荐（新产品广告）"
    TXT_MENU_9="9.  smartactions:        启用 Smart Actions （按键宏）"
    TXT_MENU_10="10. actions-ring:        启用 Actions Ring（快捷启动）"
    TXT_MENU_11="11. all (全部开启)"
    TXT_MENU_NONE="直接按回车键，则执行最小化精简安装，不开启任何额外功能"
    TXT_INPUT_PROMPT="如需自定义请输入选项 (例如 '0 2 6 10', 默认为 最小化精简安装) "
    TXT_CONFIRM_TITLE="请确认以下设置:"
    TXT_CONFIRM_PROMPT="设置是否正确？请输入：[y/n] (默认: y)"
    TXT_CANCEL="安装已取消。"
    TXT_CLEAN_CACHE="正在清理旧缓存..."
    TXT_CREATE_DIR="正在创建下载目录..."
    TXT_DOWNLOADING="正在下载安装包:"
    TXT_UNZIPPING="正在解压安装包..."
    TXT_BACKUP="正在备份现有配置..."
    TXT_UNINSTALL="正在卸载旧版本..."
    TXT_RESTORE="正在从备份恢复配置..."
    TXT_INSTALLING="正在安装新版本..."
    TXT_SUCCESS="安装成功。"
    TXT_CLEANUP="清理完成。"
    TXT_FAIL="安装失败。"
    TXT_FAIL_DL="下载安装包失败。"
    TXT_FAIL_UNZIP="解压安装包失败。"
else
    # --- English (EN) ---
    TXT_START="Starting install of"
    TXT_SELECT_KEEP="Please select the features you want to keep:"
    TXT_MENU_0="0.  quiet:               Install the application silently without UI."
    TXT_MENU_1="1.  analytics:           Shows or hides choice for users to opt in to share app usage and diagnostics data."
    TXT_MENU_2="2.  flow:                Shows or hides the Flow feature. Default value is Yes"
    TXT_MENU_3="3.  sso:                 Shows or hides ability for users to sign into the app."
    TXT_MENU_4="4.  update:              Enables or disables app updates."
    TXT_MENU_5="5.  dfu:                 Enables or disables device firmware updates."
    TXT_MENU_6="6.  backlight:           Enables or disables keyboard backlight on the supported keyboards."
    TXT_MENU_7="7.  logivoice:           Enables or disables LogiVoice feature."
    TXT_MENU_8="8.  device-recommendation: Enables or disables device recommendation feature."
    TXT_MENU_9="9.  smartactions:        Enables or disables Smart Actions feature."
    TXT_MENU_10="10. actions-ring:        Enables or disables Actions Ring feature."
    TXT_MENU_11="11. all"
    TXT_MENU_NONE="Press enter for none"
    TXT_INPUT_PROMPT="Enter your choices(e.g. 2 6, default is none): "
    TXT_CONFIRM_TITLE="Please confirm the following settings:"
    TXT_CONFIRM_PROMPT="Are these settings correct? [y/n](default: y): "
    TXT_CANCEL="Installation cancelled."
    TXT_CLEAN_CACHE="Cleaning up previous cache."
    TXT_CREATE_DIR="Creating directory..."
    TXT_DOWNLOADING="Downloading Installer from:"
    TXT_UNZIPPING="Unarchiving to"
    TXT_BACKUP="Backing up existing configuration..."
    TXT_UNINSTALL="Uninstalling existing version of"
    TXT_RESTORE="Restoring configuration from backup..."
    TXT_INSTALLING="Installing..."
    TXT_SUCCESS="Installed successfully."
    TXT_CLEANUP="Cleaning Up"
    TXT_FAIL="Failed to install"
    TXT_FAIL_DL="Failed to download installer"
    TXT_FAIL_UNZIP="Failed to unzip installer"
fi

# ----------------------------------------------------------------
# 4. Main Script Execution
# ----------------------------------------------------------------

echo ""
echo "##############################################################"
echo "$(date) | $TXT_START $appname"
echo "##############################################################"
echo ""

echo "$TXT_SELECT_KEEP"
echo "$TXT_MENU_0"
echo "$TXT_MENU_1"
echo "$TXT_MENU_2"
echo "$TXT_MENU_3"
echo "$TXT_MENU_4"
echo "$TXT_MENU_5"
echo "$TXT_MENU_6"
echo "$TXT_MENU_7"
echo "$TXT_MENU_8"
echo "$TXT_MENU_9"
echo "$TXT_MENU_10"
echo "$TXT_MENU_11"
echo "$TXT_MENU_NONE"
echo ""

read -p "$TXT_INPUT_PROMPT" features

# Initialize all options as "No"
quiet=""
analytics="No"
flow="No"
sso="No"
update="No"
dfu="No"
backlight="No"
logivoice="No"
device_recommendation="No"
smartactions="No"
actions_ring="No"

# Feature selection logic
if [[ "$features" == *11* ]]; then
  analytics="Yes"
  flow="Yes"
  sso="Yes"
  update="Yes"
  dfu="Yes"
  backlight="Yes"
  logivoice="Yes"
  device_recommendation="Yes"
  smartactions="Yes"
  actions_ring="Yes"
else
  for feature in $features; do
    case $feature in
      0) quiet="--quiet" ;;
      1) analytics="Yes" ;;
      2) flow="Yes" ;;
      3) sso="Yes" ;;
      4) update="Yes" ;;
      5) dfu="Yes" ;;
      6) backlight="Yes" ;;
      7) logivoice="Yes" ;;
      8) device_recommendation="Yes" ;;
      9) smartactions="Yes" ;;
      10) actions_ring="Yes" ;;
      *) ;; # Ignore invalid
    esac
  done
fi

echo ""
echo "$TXT_CONFIRM_TITLE"
echo "quiet:                  $quiet"
echo "analytics:              $analytics"
echo "flow:                   $flow"
echo "sso:                    $sso"
echo "update:                 $update"
echo "dfu:                    $dfu"
echo "backlight:              $backlight"
echo "logivoice:              $logivoice"
echo "device-recommendation:  $device_recommendation"
echo "smartactions:           $smartactions"
echo "actions-ring:           $actions_ring"
echo ""

read -p "$TXT_CONFIRM_PROMPT" confirm
if [[ $confirm != "Y" && $confirm != "y" && $confirm != "" ]]; then
    echo "$TXT_CANCEL"
    exit 1
fi

# Prepare directories
if [ -d "$downloaded_path" ]; then
    echo "$(date) | $TXT_CLEAN_CACHE"
    rm -rf "$downloaded_path"
fi

echo "$(date) | $TXT_CREATE_DIR"
mkdir -p "$downloaded_path"

# Downloading
echo "$(date) | $TXT_DOWNLOADING $selected_weburl"
curl -L -f -o "$downloaded_package_path" "$selected_weburl" || { echo "$TXT_FAIL_DL"; exit 1; }

# Unzipping
echo "$(date) | $TXT_UNZIPPING $downloaded_path..."
ditto -x -k "$downloaded_package_path" "$downloaded_path" || { echo "$TXT_FAIL_UNZIP"; exit 1; }

# Configure backup
# Safety check: Remove existing backup folder to avoid nesting errors
if [ -d "$config_path" ]; then
    echo "$(date) | $TXT_BACKUP"
    if [ -d "$backup_path" ]; then
        rm -rf "$backup_path"
    fi
    mv "$config_path" "$backup_path"
fi

# Uninstall existing
echo "$(date) | $TXT_UNINSTALL $appname"
# Note: Redirecting output to silence it, but errors might still show
sudo "$install_path" --uninstall >> /dev/null 2>&1

# Restore backup
if [ -d "$backup_path" ]; then
    echo "$(date) | $TXT_RESTORE"
    # Ensure target doesn't exist before restoring to avoid nesting
    if [ -d "$config_path" ]; then
        rm -rf "$config_path"
    fi
    mv "$backup_path" "$config_path"
fi

# Installing
echo "$(date) | $TXT_INSTALLING $appname..."

# Construct command string for display (optional, can be removed if cleaner output desired)
# install_command="$install_path $quiet --analytics $analytics ..."
# echo "Executing: $install_command"

sudo "$install_path" \
        $quiet \
        --analytics $analytics \
        --flow $flow \
        --sso $sso \
        --update $update \
        --dfu $dfu \
        --backlight $backlight \
        --logivoice $logivoice \
        --device-recommendation $device_recommendation \
        --smartactions $smartactions \
        --actions-ring $actions_ring

if [ "$?" = "0" ]; then
    echo "$(date) | $appname $TXT_SUCCESS"
    echo "$(date) | $TXT_CLEANUP"
    rm -rf "$downloaded_path"
    exit 0
else
    echo "$(date) | $TXT_FAIL $appname"
    exit -1
fi
