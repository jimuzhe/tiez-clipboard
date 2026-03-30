<div align="center">

<img src="docs/images/show.png" alt="TieZ Logo" />

# TieZ

轻量、快速、常驻后台的跨平台剪贴板管理器，专注于把复制、搜索、粘贴、同步和传输这几件事做顺手。

[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](https://www.gnu.org/licenses/gpl-3.0)
[![Version](https://img.shields.io/badge/version-0.3.1-green.svg)](https://github.com/jimuzhe/tiez-clipboard/releases)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS-lightgrey.svg)](https://github.com/jimuzhe/tiez-clipboard/releases)
[![Built with Tauri](https://img.shields.io/badge/built%20with-Tauri%202-4a90d9.svg)](https://tauri.app/)

[English](./README.md) | [简体中文](./README.zh-CN.md)

[下载 Releases](https://github.com/jimuzhe/tiez-clipboard/releases) · [反馈问题](https://github.com/jimuzhe/tiez-clipboard/issues)

</div>

---

## 预览

<div align="center">
  <img src="docs/images/ui预览.png" alt="TieZ 界面预览" width="860" />
</div>

## 简介

**TieZ** 是一款基于 [Tauri 2](https://tauri.app/) 构建的跨平台桌面剪贴板管理工具，支持 **Windows** 和 **macOS**。它常驻系统托盘，可通过全局快捷键快速呼出，帮助你更高效地管理文字、图片、富文本与标签。

## 亮点

| 亮点 | 说明 |
| --- | --- |
| 快 | `Alt+V` 一键呼出，常用操作尽量减少层级 |
| 全 | 支持文本、图片、富文本、标签、Emoji |
| 稳 | 本地优先，适合作为日常高频工具长期驻留 |
| 灵活 | 主题、快捷键、持久化、同步方式都可按需调整 |

## 功能一览

### 1. 采集与监听

- 系统原生剪贴板事件驱动监听，非轮询方案
- 纯文本采集
- 富文本（HTML）采集
- 图片自动采集与外存为 `.png`
- 文件及文件夹路径记录
- 基于哈希（Hash）的内容去重
- 代码片段类型识别

### 2. 存储管理

- 可自定义存储条数上限
- 置顶记录保护，不受上限清理影响
- 带标签记录保护，不受上限清理影响
- 定期自动清理旧记录
- 历史数据按天分组显示
- 记录使用次数统计（Use Count）

### 3. 查看与检索

- 全文内容搜索
- 所属应用名称搜索
- 标签分类搜索
- 紧凑模式 / 详细模式切换预览
- 置顶项优先显示
- 历史记录分页加载

### 4. 组织与操作

- 自定义多色标签（Tags）系统
- 标签全局重命名与管理
- 记录置顶（Pin / Unpin）
- 置顶项手动拖拽排序
- 手动删除单条 / 多条记录
- 一键清空非保护记录
- 数据 JSON 格式导出 / 导入

### 5. 交互流与外部协作

- 快捷键一键唤起界面
- 外部编辑器打开内容（File Handler）
- 外部修改自动回写同步（File Watcher）
- 顺序粘贴模式（Sequential Mode）
- 点击 / 回车自动粘贴
- 粘贴后自动置顶逻辑（可选）
- 粘贴后自动删除逻辑（可选）

### 6. 安全与隐私

- 端到端数据库加密
- 敏感标签（Sensitive Tag）自动加密
- 身份证、手机号、邮箱等隐私信息自动正则识别与屏蔽

### 7. 网络与多端

- WebDAV 云同步
- 同步墓碑机制，确保多端删除同步
- 无感验证码同步
- 多设备同步冲突处理

### 8. 系统个性化

- Mica（云母）/ Acrylic（亚克力）背景效果
- 暗黑 / 常规模式及系统跟随
- 窗口透明度调节
- 边缘吸附与窗口置顶
- 跟随鼠标位置弹出
- 系统托盘图标隐藏
- 开机自动启动控制
- 操作音效开关控制

## 系统要求

| 平台 | 要求 |
| --- | --- |
| Windows | Windows 10/11（x64）；Windows 10 需安装 [Microsoft Edge WebView2](https://developer.microsoft.com/zh-cn/microsoft-edge/webview2/) |
| macOS | macOS 10.15 Catalina 及以上（Apple Silicon / Intel） |

## 快速开始

前往 [Releases 页面](https://github.com/jimuzhe/tiez-clipboard/releases) 下载对应平台的安装包。

| 平台 | 安装包 |
| --- | --- |
| Windows | `.exe` 安装包 / `.zip` 便携包 |
| macOS | `.dmg` 安装镜像 |

## 赞助与交流

如果 TieZ 对你有帮助，欢迎赞助支持项目持续更新。

- 打赏后记得备注昵称或称呼，会添加到我们的[打赏名单](https://tiez.name666.top/zh/sponsors.html)
- QQ 群可扫描下方二维码加入

<div align="center">
  <table>
    <tr>
      <td align="center">
        <p><strong>微信赞赏</strong></p>
        <img src="docs/images/wx.jpeg" alt="微信收款码" width="220" />
      </td>
      <td align="center">
        <p><strong>支付宝赞赏</strong></p>
        <img src="docs/images/zfb.jpeg" alt="支付宝收款码" width="220" />
      </td>
    </tr>
  </table>
  <p><strong>QQ 群</strong></p>
  <img src="docs/images/qq.jpeg" alt="QQ 群二维码" width="220" />
</div>

---

<div align="center">

如果 TieZ 对你有帮助，欢迎点个 Star。

</div>
