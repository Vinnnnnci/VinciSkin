<div align="center">
  <img src="src-tauri/icons/128x128@2x.png" alt="VinciSkin" width="96" height="96">
  <h1>VinciSkin</h1>
  <p>英雄联盟国服 mod 管理器</p>
</div>

<div align="center">

[![Sponsor](https://img.shields.io/badge/Sponsor-%E7%88%B1%E5%8F%91%E7%94%B5-946ce6?style=for-the-badge)](https://ifdian.net/a/Vinnnnnci)
[![Download](https://img.shields.io/badge/Download-v1.23.1-0078D4?style=for-the-badge&logo=github)](https://github.com/Vinnnnnci/VinciSkin/releases/tag/v1.23.1)
[![License: GPL-3.0](https://img.shields.io/badge/License-GPL--3.0-blue?style=for-the-badge)](LICENSE)
[![Windows 10+](https://img.shields.io/badge/Windows-10+-0078D4?style=for-the-badge&logo=windows)](#下载)

</div>

## 求一杯奶茶

VinciSkin 是我一个人在业余时间写的。研究国服客户端的文件格式、跟着每一次游戏更新做适配、修别人报上来的问题，这些加起来是很多个熬夜的晚上。

它免费开源，没有付费功能，不加广告，也不卖授权。如果你用它换到了喜欢的皮肤，玩得开心，能不能请我喝一杯奶茶：

<div align="center">

**[❤️ 去爱发电赞助 →](https://ifdian.net/a/Vinnnnnci)**

</div>

一杯奶茶就是 ¥6，重要的是让我知道这份劳动有人认了。所有赞助都记在爱发电的公开名单里，用在代码签名证书和发布开销上。

不赞助也完全没关系，软件永远免费、功能一分不少。点一个 **Star**、推荐给打游戏的兄弟，同样是在帮我。

另外说清楚：官方版本只在 [GitHub Releases](https://github.com/Vinnnnnci/VinciSkin/releases) 发布。从别处花钱买到的副本，那笔钱没有到我手里。

---

VinciSkin 是一个面向英雄联盟国服的 mod 管理器，只支持 Windows 10 和 11。导入 mod，开关、排序、按一下启动，进游戏就能看到效果。自带的 Workshop 可以在同一个窗口里浏览游戏文件、改 mod、打包。

## 国服说明

项目基于国服客户端开发和测试，目前只针对国服。

个人测试跑了一个月，没有出现异常。但不代表没有风险：它通过补丁修改游戏加载的资源，这类做法不符合 Riot 和腾讯的用户协议，存在封号风险。测试结果只覆盖我自己的环境，不保证你的账号安全。用不用、用哪个号，自己判断。

## 功能

- 导入 `.modpkg` 和 `.fantome`，拖进窗口即可
- mod 库：搜索、文件夹、拖拽排序、单个开关
- 配置档：多套 mod 方案随时切换
- mod 健康检查：游戏更新后自动检查，能修的修，修不了的说明原因
- 国服皮肤：内置国服英雄与皮肤目录，选了就换，不需要额外下载皮肤包
- 启动：只启动补丁器，或者一个按钮完成启动
- Workshop：浏览游戏归档、编辑 bin、改文本、打包 mod

## 下载

**[v1.23.1 安装包（zip）](https://github.com/Vinnnnnci/VinciSkin/releases/tag/v1.23.1)**

解压到任意文件夹，双击 `vinciskin.exe`。没有安装程序，删除文件夹就是卸载。

- 未签名：Windows 会提示“未知发布者”，点“更多信息”后选“仍要运行”
- 需要 Windows 10 / 11 64 位

历史版本在 [Releases](https://github.com/Vinnnnnci/VinciSkin/releases) 页面。

## 参考

- [LeagueToolkit](https://github.com/LeagueToolkit)：mod 格式、哈希表和补丁器的底层库
- [cslol-manager](https://github.com/LeagueToolkit/cslol-manager)：上一代 mod 管理器，注入和启动流程参考了它

## 许可

代码使用 [GPL-3.0-or-later](LICENSE)。名称和图标见 [TRADEMARK.md](TRADEMARK.md)。内置的补丁器二进制遵循 [LTK Patcher License](LTK-PATCHER-LICENSE.md)。

---

问题反馈：[Issues](https://github.com/Vinnnnnci/VinciSkin/issues) · [Discord](https://discord.gg/yhzDVRyQex)
