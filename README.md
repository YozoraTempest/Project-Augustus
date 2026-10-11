# 奥古斯都计划 · Project Augustus

Project Augustus 是重新实现《欧陆风云 IV》（EU4）游戏引擎的开源项目，由社区维护。项目独立实现游戏逻辑，玩家提供自己合法取得的本地游戏文件，原版程序、脚本和资源不随项目分发。

项目仍处于早期开发阶段。目前可以载入 EU4 1.37.5 的固定地图、国家和省份的部分开局状态，在 Godot 中查看领土、人物、外交、建筑、商品、贸易节点、科技与政体。已接入日历、税收月结和项目自身存档；税收或国库存在缺项时保留未知，不将部分计算结果入账。项目以终版及全部 DLC 机制为基线，默认玩家具备全部 DLC；完整经济、战争、AI 和事件尚未接入，当前尚不能运行完整战役或 MEIOU & Taxes。运行方法与实际范围见 [原版世界载入与时间推进](docs/WORLD_INITIALIZATION.md)。

另有一个不需要原版安装的自制建造演示，支持费用扣除、建造队列、完工回调、日期推进、存档和系统替换。EU4 写法的回调与原生规则都由同一套 Rust 核心执行，见 [建造演示](docs/CONSTRUCTION_DEMO.md)。

项目也提供本地文件检查工具，用于读取目录、Mod 描述和文本，查询文件覆盖及定义来源。使用方法见 [EU4 文件工具](docs/EU4_CONTENT.md)。

## 参与开发

开发工作在 `develop` 分支进行。开始前请阅读 [贡献指南](CONTRIBUTING.md)。安装 Rust 工具链后，可以运行：

```sh
cargo build --workspace --locked
```

## 游戏文件与许可

运行时所需的 EU4 与 DLC 内容由玩家按自己实际获得的许可提供。文件与发行要求见 [游戏内容说明](docs/DISTRIBUTION.md)。

项目自有代码采用 [GNU GPLv3](LICENSE)（`GPL-3.0-only`），第三方依赖保留各自的许可，见 [第三方声明](docs/THIRD_PARTY.md)。

Project Augustus 是非官方社区项目，与 Paradox Interactive 无隶属关系，也未获其官方认可或支持。
