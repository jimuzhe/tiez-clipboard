# TieZ Linux 安装指南

## 系统要求

- **操作系统**: Linux (X11 环境)
- **架构**: x86_64
- **显示服务器**: X11 (Wayland 暂不支持)

## 安装依赖

### Ubuntu/Debian

```bash
sudo apt update
sudo apt install -y libgtk-3-dev libwebkit2gtk-4.0-dev libappindicator3-dev xdotool
```

### Arch Linux

```bash
sudo pacman -S gtk3 webkit2gtk libappindicator-gtk3 xdotool
```

### Fedora

```bash
sudo dnf install gtk3-devel webkit2gtk3-devel libappindicator-gtk3-devel xdotool
```

### openSUSE

```bash
sudo zypper install gtk3-devel webkit2gtk3-devel libappindicator3-devel xdotool
```

## 从源码构建

### 前置要求

1. **Node.js** (v18 或更高)
   ```bash
   # 使用 nvm 安装
   curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.0/install.sh | bash
   nvm install 20
   nvm use 20
   ```

2. **Rust** (最新稳定版)
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   source $HOME/.cargo/env
   ```

3. **Tauri CLI**
   ```bash
   cargo install tauri-cli
   ```

### 构建步骤

```bash
# 克隆仓库
git clone https://github.com/yourusername/tiez-clipboard.git
cd tiez-clipboard

# 安装前端依赖
npm install

# 开发模式运行
npm run tauri:dev

# 构建发布版本
npm run tauri:build
```

构建完成后，安装包位于 `src-tauri/target/release/bundle/` 目录：
- `.deb` - Debian/Ubuntu 安装包
- `.AppImage` - 通用 Linux 可执行文件
- `.rpm` - Fedora/openSUSE 安装包

## 安装方式

### 方法 1: AppImage (推荐)

```bash
# 赋予执行权限
chmod +x TieZ_0.3.1_amd64.AppImage

# 运行
./TieZ_0.3.1_amd64.AppImage

# 可选: 移动到系统目录
sudo mv TieZ_0.3.1_amd64.AppImage /usr/local/bin/tiez
```

### 方法 2: Debian/Ubuntu (.deb)

```bash
sudo dpkg -i tiez_0.3.1_amd64.deb
sudo apt-get install -f  # 修复依赖
```

### 方法 3: Fedora/openSUSE (.rpm)

```bash
sudo rpm -i tiez-0.3.1-1.x86_64.rpm
```

## 已知问题

1. **Wayland 不支持**: 当前仅支持 X11 环境。Wayland 用户需要切换到 X11 会话或使用 XWayland。

2. **xdotool 依赖**: 粘贴功能依赖 xdotool，请确保已安装。

3. **窗口边缘吸附**: Linux 版本暂不支持窗口边缘吸附功能。

4. **应用图标**: 应用来源识别功能有限，可能无法正确识别所有应用。

## 故障排除

### 启动失败

检查依赖是否完整：
```bash
ldd src-tauri/target/release/tiez-app | grep "not found"
```

### 粘贴不工作

检查 xdotool 是否可用：
```bash
xdotool key ctrl+v
```

### 剪贴板监听不工作

确保运行环境是 X11：
```bash
echo $XDG_SESSION_TYPE
# 应该输出: x11
```

## 技术细节

### 架构

- **前端**: React 19 + TypeScript + Vite
- **后端**: Rust + Tauri 2
- **剪贴板库**: arboard + x11-clipboard
- **粘贴模拟**: xdotool
- **系统托盘**: Tauri Tray (GTK)

### Linux 特有实现

位于 `src-tauri/src/infrastructure/linux_api/`:
- `clipboard.rs` - 剪贴板操作
- `window_tracker.rs` - 窗口追踪 (基础实现)
- `apps.rs` - 应用信息 (基础实现)
- `paste.rs` - 粘贴模拟

## 贡献

欢迎提交 PR 改进 Linux 支持！

特别是以下方面：
- Wayland 支持
- 窗口边缘吸附
- 应用图标识别
- 更多发行版支持
