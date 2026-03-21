# 分叉说明

本项目是从 [jimuzhe/tiez-clipboard](https://github.com/jimuzhe/tiez-clipboard) fork 而来的版本。

创建此 fork 的主要目的是围绕个人使用需求，持续推进一些与上游维护方向不同的改进。

包括：

- 更易扩展的主题支持
  - 尽可能减少主题设置在代码中的硬编码，以便于后续的主题定制
  - 增加一个完全由ai设计的新主题macos
  - 扩展主题能定制的控件的范围

- 简单且有用的新功能
  - 新增右键图片时粘贴base64编码的支持
  - 新增对多个呼出快捷键的支持

- 若干问题修复与可维护性优化
  - 修复tauri:dev模式不可用的问题，加快开发速度
  - 修复上游在合并pr时因未删除冗余代码，导致拖拽功能偶现失效的问题

本 fork 主要基于个人需求进行维护，后续更新频率不作保证。由于我目前没有 macOS 设备，macOS 平台的可用性也暂未经过完整验证。

如果有精力继续维护，则路线图为 [路线图](./RoadMap.md)

如需查看上游提供的README文件，可点击[English](https://github.com/jimuzhe/tiez-clipboard/blob/master/README.md) | [中文](https://github.com/jimuzhe/tiez-clipboard/blob/master/README.zh-CN.md)