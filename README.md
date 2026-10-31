# 奥古斯都计划 · Project Augustus

Project Augustus 是重新实现《欧陆风云 IV》（EU4）游戏引擎的开源项目，由社区维护。项目独立实现游戏逻辑，玩家提供自己合法取得的本地游戏文件，原版程序、脚本和资源不随项目分发。

项目仍处于早期开发阶段。目前可以载入 EU4 1.37.5 的固定地图、国家和省份的部分开局状态，在 Godot 中查看领土、人物、外交、建筑、商品、贸易节点、科技与政体。已接入日历、税收及点数月结和项目自身存档；收入或库存存在缺项时保留未知，不将部分计算结果入账。兼容层还可以从未压缩的 EU4txt 存档导入已支持的启动状态，并在同一份存档基线上叠加外部状态包后继续推进。项目以终版及全部 DLC 机制为基线，默认玩家具备全部 DLC；完整经济、战争、AI 和事件尚未接入，当前尚不能运行完整战役或 MEIOU & Taxes。运行方法与实际范围见 [原版世界载入与时间推进](docs/WORLD_INITIALIZATION.md)。

另有一个不需要原版安装的自制建造演示，支持费用扣除、建造队列、完工回调、日期推进、存档和系统替换。EU4 写法的回调与原生规则都由同一套 Rust 核心执行，见 [建造演示](docs/CONSTRUCTION_DEMO.md)。

EU4 存档导入还支持已支付的普通舰船订单：受支持的单订单自动续建；显式添加 `--import-naval-construction-queues` 可按存档顺序推进多订单 FIFO 队列，并读取等待项省略零进度的保存形式。保存恢复后可完成并生成舰船，不会再次扣费。`--show-naval-constructions` 可查看当前待完成订单、排期和 FIFO 位置。调用者也可提供明确费用和完成日提交新订单，或用 `--naval-construction-queues` 在空港口提交多笔 FIFO 请求；国库调整与入队作为一次原子操作完成，并可显式登记 `EXPENSEFLEET` 分类账费用。`--cancel-naval-constructions` 可按稳定 ID 原子取消待完成订单，不退款或逆转账本。`--naval-construction-time-queries` 可查询普通 galley 工期，`--naval-construction-cost-queries` 可查询显式限额内的普通 galley 成本，`--naval-force-limit-queries` 和 `--land-force-limit-queries` 可调用已有核心算术计算调用者完整解析的海军或陆军力限，`--vanilla-production-queries` 和 `--vanilla-gold-production-queries` 可汇总调用者按省份提供的普通生产与黄金生产输入，`--vanilla-production-value-queries` 可查询使用已加载商品基础价格的省份普通生产 value stage，`--vanilla-content-production-queries` 可预览使用 goods-size、已加载商品价格和国家行政科技的省份普通生产绑定，`--vanilla-gold-stage-queries` 可查询当前黄金省份的单省 base 或 production stage，`--estate-loyalty-queries` 可预览国家阶层下一次日更忠诚度，`--vanilla-trade-good-price-queries` 可预览调用者明确提供的 ordered 商品价格影响链，`--fort-maintenance-queries` 可查询直接或按已加载建筑派生的省份要塞维护阶段，`--state-maintenance-queries` 可查询调用者解析的省份州维护贡献；这些入口结合有效内容定义与调用者解析的完整输入，缺项时保留未决。完整原版报价、暂停及取消退款规则仍待接入；多订单路径已通过自制用例与受控海军队列的原版实机保存重载对照，自然生产命令创建多订单的完整链路仍待核对。范围与操作见 [原版世界载入与时间推进](docs/WORLD_INITIALIZATION.md)。

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
