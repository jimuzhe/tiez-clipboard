# 分叉说明

本项目是从 [jimuzhe/tiez-clipboard](https://github.com/jimuzhe/tiez-clipboard) fork 而来的版本。

创建此 fork 的主要目的是围绕个人使用需求，持续推进一些与上游维护方向不同的改进。

包括：

- 更易扩展的主题支持
  - 尽可能减少主题设置在代码中的硬编码，以便于后续的主题定制
  - 增加`macos`，由AI辅助设计的MacOS风格新主题
  - 扩展主题能定制的控件的范围

- 简单且有用的新功能
  - 新增右键图片时粘贴`base64`编码的支持
  - 新增对多个呼出快捷键的支持

- 若干问题修复与可维护性优化
  - 修复`tauri:dev`模式不可用的问题，加快开发速度
  - 修复上游在合并pr时因未删除冗余代码，导致拖拽功能偶现失效的问题

本 fork 主要基于个人需求进行维护，后续更新频率不作保证。由于我目前没有 macOS 设备，macOS 平台的可用性也暂未经过完整验证。

如果有精力继续维护，则暂定的路线图为[路线图](docs/markdown/RoadMap.md)；暂未定的内容为[愿景](docs/markdown/Wish.md)

如需查看上游仓库文档：

- [English README](https://github.com/jimuzhe/tiez-clipboard/blob/master/README.md)
- [中文 README](https://github.com/jimuzhe/tiez-clipboard/blob/master/README.zh-CN.md)
- [上游仓库](https://github.com/jimuzhe/tiez-clipboard)