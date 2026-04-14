# 分叉说明

本项目是从 [jimuzhe/tiez-clipboard](https://github.com/jimuzhe/tiez-clipboard) fork 而来的版本。

创建此 fork 的主要目的是围绕个人使用需求，持续推进一些与上游维护方向不同的改进。

包括：

- 更精简的项目
  - 移除了对`MacOS`的支持，因为没有测试条件
  - 移除了所有联网功能（包括云同步，MQTT，ai助手），因为个人不需要
  - 移除了一些冗余代码
- **新增 Linux X11 支持**
  - 添加 Linux 平台基础支持（X11 环境）
  - 使用 x11-clipboard 实现剪贴板监听
  - 使用 xdotool 实现粘贴模拟
  - 支持 deb、AppImage、rpm 打包格式
- 更易扩展的主题支持
  - 尽可能减少主题设置在代码中的硬编码，以便于后续的主题定制
  - 增加`macos`，由AI辅助设计的MacOS风格新主题
  - 增加`scifi`, 由AI辅助设计的科幻风格新主题；这两套主题使用同一个base框架
  - 扩展主题能定制的控件的范围：例如，现在下拉栏的风格会随主题改变了
- 简单且有用的新功能
  - 新增右键图片时粘贴`base64`编码的支持
  - 新增对多个呼出快捷键的支持
- 若干问题修复与可维护性优化
  - 修复`tauri:dev`模式不可用的问题，加快开发速度
  - 修复上游在合并pr时因未删除冗余代码，导致拖拽功能偶现失效的问题

## Linux 安装

### 依赖

在 Linux 上运行需要安装以下依赖：

**Ubuntu/Debian:**
```bash
sudo apt update
sudo apt install -y libgtk-3-dev libwebkit2gtk-4.0-dev libappindicator3-dev xdotool
```

**Arch Linux:**
```bash
sudo pacman -S gtk3 webkit2gtk libappindicator-gtk3 xdotool
```

**Fedora:**
```bash
sudo dnf install gtk3-devel webkit2gtk3-devel libappindicator-gtk3-devel xdotool
```

### 构建

```bash
# 安装前端依赖
npm install

# 开发模式
npm run tauri:dev

# 构建 Linux 版本
npm run tauri:build
```

构建完成后，安装包位于 `src-tauri/target/release/bundle/` 目录下。

本 fork 主要基于个人需求进行维护，后续更新频率不作保证。

如需查看当前仓库文档，可参考 [README.en-US](docs/markdown/README.en-US.md) 与 [README.zh-CN](docs/markdown/README.zh-CN.md)

如需查看上游仓库文档：

- [English README](https://github.com/jimuzhe/tiez-clipboard/blob/master/README.md)
- [中文 README](https://github.com/jimuzhe/tiez-clipboard/blob/master/README.zh-CN.md)
- [上游仓库](https://github.com/jimuzhe/tiez-clipboard)

