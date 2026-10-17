# 奥古斯都计划 · Project Augustus

Project Augustus 是重新实现《欧陆风云 IV》（EU4）游戏引擎的开源项目，由社区维护。项目独立实现游戏逻辑，玩家提供自己合法取得的本地游戏文件，原版程序、脚本和资源不随项目分发。

项目仍处于早期开发阶段。目前可以载入 EU4 1.37.5 的固定地图、国家和省份的部分开局状态，在 Godot 中查看领土、人物、外交、建筑、商品、贸易节点、科技与政体。已接入日历、税收及点数月结和项目自身存档；收入或库存存在缺项时保留未知，不将部分计算结果入账。兼容层还可以从未压缩的 EU4txt 存档导入已支持的启动状态，但不能续跑完整原版存档。项目以终版及全部 DLC 机制为基线，默认玩家具备全部 DLC；完整经济、战争、AI 和事件尚未接入，当前尚不能运行完整战役或 MEIOU & Taxes。运行方法与实际范围见 [原版世界载入与时间推进](docs/WORLD_INITIALIZATION.md)。

另有一个不需要原版安装的自制建造演示，支持费用扣除、建造队列、完工回调、日期推进、存档和系统替换。EU4 写法的回调与原生规则都由同一套 Rust 核心执行，见 [建造演示](docs/CONSTRUCTION_DEMO.md)。

自制事件演示支持显式触发、等待选项、选项与收尾效果、保存恢复；两种输入使用相同核心，见 [事件演示](docs/EVENT_DEMO.md)。学派选择与切换也使用这一核心，属性查询读取当前选择，见 [学派变更演示](examples/school-effects/README.md)。

条件可以进入首都、所有者、职位和命名对象，读取共享世界中的地理成员关系；自制样本与操作见 [作用域条件演示](examples/scoped-conditions/README.md)。

国家是否拥有领土也由共享核心查询，已登记标签与存活国家分别处理，见 [国家领土条件演示](examples/country-existence/README.md)。[拥有省份查询](examples/owned-provinces/README.md) 可进一步检查当前成员的条件，并在归属或省份状态变化后读取新结果。[遍历与随机选择](examples/owned-iteration/README.md) 共用核心计划，保留取得顺序、实时筛选和读档后的随机状态。

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
