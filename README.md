# 奥古斯都计划 · Project Augustus

Project Augustus 是重新实现《欧陆风云 IV》（EU4）游戏引擎的开源项目，由社区维护。项目独立实现游戏逻辑，玩家提供自己合法取得的本地游戏文件，原版程序、脚本和资源不随项目分发。

项目仍处于早期开发阶段。目前提供 EU4 文件检查工具，可以读取目录、Mod 描述和文本；尚不能运行一局游戏。使用方法见 [EU4 文件工具](docs/EU4_CONTENT.md)。

## 参与开发

开发工作在 `develop` 分支进行。开始前请阅读 [贡献指南](CONTRIBUTING.md)。安装 Rust 工具链后，可以运行：

```sh
cargo build --workspace --locked
```

## 游戏文件与许可

运行时所需的 EU4 与 DLC 内容由玩家按自己实际获得的许可提供。文件与发行要求见 [游戏内容说明](docs/DISTRIBUTION.md)。

项目自有代码采用 [GNU GPLv3](LICENSE)（`GPL-3.0-only`），第三方依赖保留各自的许可，见 [第三方声明](docs/THIRD_PARTY.md)。

Project Augustus 是非官方社区项目，与 Paradox Interactive 无隶属关系，也未获其官方认可或支持。
