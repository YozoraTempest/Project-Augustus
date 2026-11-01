# 奥古斯都计划 · Project Augustus

Project Augustus 是重新实现《欧陆风云 IV》（EU4）游戏引擎的开源项目，由社区维护。项目独立实现游戏逻辑，玩家提供自己合法取得的本地游戏文件，原版程序、脚本和资源不随项目分发。

项目仍处于早期开发阶段。目前可以载入 EU4 1.37.5 的固定地图、国家和省份的部分开局状态，在 Godot 中查看领土、人物、外交、建筑、商品、贸易节点、科技与政体。已接入日历、税收及点数月结和项目自身存档；收入或库存存在缺项时保留未知，不将部分计算结果入账。兼容层还可以从未压缩的 EU4txt 存档导入已支持的启动状态，并在同一份存档基线上叠加外部状态包后继续推进。项目以终版及全部 DLC 机制为基线，默认玩家具备全部 DLC；完整经济、战争、AI 和事件尚未接入，当前尚不能运行完整战役或 MEIOU & Taxes。省份面板可查看当前贸易中心等级和所选国家的发现状态，并在切换国家、推进时间、选择事件及读档后刷新。运行方法与实际范围见 [原版世界载入与时间推进](docs/WORLD_INITIALIZATION.md)。

另有一个不需要原版安装的自制建造演示，支持费用扣除、建造队列、完工回调、日期推进、存档和系统替换。EU4 写法的回调与原生规则都由同一套 Rust 核心执行，见 [建造演示](docs/CONSTRUCTION_DEMO.md)。

Godot 国家面板的“任务”区可以检查已选任务、前置任务与当前目标条件，并领取已支持的奖励。声明了省份目标的任务还提供“目标”按钮，在地图上高亮当前匹配的省份；未决省份单独提示，切换国家或重新载入会清除活动目标，推进、事件及恢复后会重新求值。领取会重新检查实时状态，奖励和完成记录作为一次事务提交；已完成任务不能重复领取。未支持的条件或奖励保留未决并禁用领取。这个入口与仅登记完成状态的 `complete_mission` 分开；自制样本见 [任务领取演示](examples/mission-claims/README.md)与[任务省份高亮演示](examples/mission-highlights/README.md)。

EU4 存档入口会读取各国保存的任务系列选择，中途存档可保留当时的任务树；当前条件发生变化时，仍由显式任务刷新更新选择。缺失系列会保留未知并给出诊断，导入选择不会执行奖励。CLI 与 Godot 使用同一条导入路径，选择和完成记录随项目快照保存恢复。

中途存档的当前宗主关系也会从顶层外交数据导入，替换历史中的属国关系。宗主、属国类型及直接属国数量条件和任务领取使用同一张图，项目快照保存恢复这些关系。缺少外交数据时保持未知，显式空图表示已知没有属国关系；无效关系会被拒绝，不将不完整图视为独立国家。

国家面板的“军力”区可展开当前已载入的陆军与舰队，查看团、舰船的名称、类型、已知状态和驻地，并打开驻地省份详情。切换国家、推进、事件及读取快照后会刷新；CLI 使用 `--military` 查看同一份明细，可结合 `--country` 与 `--province` 筛选。该列表反映共享世界已有的编队，空列表不代表已完整确认没有军力。自制样本见[实时军力检视](examples/military-roster/README.md)。

“财政”区显示当前国库、账本待结余额及最近的税收、普通生产、现金流月结记录。记录标明实际日期、已知部分、完整金额、阶段确认入账和缺项原因，保留未知值与已知零值的区别；CLI 使用 `--country-finances --country AAA` 查看同一份报告。历史阶段余额独立于当前国库，不作为下月预测。自制输入与操作见[财政月结记录样本](examples/economy/README.md)。

“分类账”区可分别查看当前国家的累计收入与支出分类、待归集来源、归集快照、待结转累计、结转快照及辅助支出记录。没有账本、未载入向量和已知零值分别显示；查询只读，不执行归集或结转，也不推断快照日期。CLI 已有 `--country AAA` 输出相同核心账本，自制输入与操作见[当前国家分类账样本](examples/ledger/README.md)。

“造船”区可查看所选国家当前已接入的待造订单、港口、FIFO 位置和当前排期。载入 EU4 存档时可勾选“载入舰船队列”，按已有 CLI 路径接入多订单续建；默认保持关闭。推进和读取项目快照会刷新待造列表，完成的舰船进入“军力”区。等待项的排期会顺延，不能当作最终完工预测；输入存档的未接入记录单独显示为全局导入诊断。自制输入与操作见[舰船队列检视样本](examples/naval-queues/README.md)。

EU4 存档导入还支持已支付的普通舰船订单：受支持的单订单自动续建；显式添加 `--import-naval-construction-queues` 可按存档顺序推进多订单 FIFO 队列，并读取等待项省略零进度的保存形式。保存恢复后可完成并生成舰船，不会再次扣费。`--show-naval-constructions` 可查看当前待完成订单、排期和 FIFO 位置。调用者也可提供明确费用和完成日提交新订单，或用 `--naval-construction-queues` 在空港口提交多笔 FIFO 请求；国库调整与入队作为一次原子操作完成，并可显式登记 `EXPENSEFLEET` 分类账费用。`--cancel-naval-constructions` 可按稳定 ID 原子取消待完成订单，不退款或逆转账本。`--naval-construction-time-queries` 可查询普通 galley 工期，`--naval-construction-cost-queries` 可查询显式限额内的普通 galley 成本，`--naval-force-limit-queries` 和 `--land-force-limit-queries` 可调用已有核心算术计算调用者完整解析的海军或陆军力限，`--vanilla-production-queries` 和 `--vanilla-gold-production-queries` 可汇总调用者按省份提供的普通生产与黄金生产输入，`--vanilla-production-value-queries` 可查询使用已加载商品基础价格的省份普通生产 value stage，`--vanilla-content-production-queries` 可预览使用 goods-size、已加载商品价格和国家行政科技的省份普通生产绑定，`--vanilla-gold-stage-queries` 可查询当前黄金省份的单省 base 或 production stage，`--estate-loyalty-queries` 可预览国家阶层下一次日更忠诚度，`--vanilla-trade-good-price-queries` 可预览调用者明确提供的 ordered 商品价格影响链，`--vanilla-monthly-autonomy-queries` 可预览省份下一次月度 raw autonomy，`--vanilla-monthly-accrual-queries` 可预览下一次月度君主点数和其他 accrual 入账，`--vanilla-monthly-convergence-queries` 可预览下一次月度 convergence/temporary liberty desire 结算，`--vanilla-monthly-tax-queries` 可预览下一次月度税收结算，`--vanilla-monthly-production-queries` 可预览下一次普通生产结算，`--vanilla-monthly-cashflow-queries` 可预览下一次税收与生产之后的现金流结算，`--vanilla-monthly-ledger-queries` 可预览下一次月度国家分类账追加，`--vanilla-monthly-settlement-queries` 可预览标准月度执行器完成 treasury flush 后的最终国家状态，`--vanilla-pending-event-queries` 可检查待选事件及其本地化展示信息，`--country-runtime-queries` 可读取国家已绑定的任务完成/可用集合与存档命名变量，`--vanilla-province-discovery-queries` 可检查省份按国家的发现三态与完整地图知识集合，`--vanilla-province-center-of-trade-queries` 可读取省份当前贸易中心等级，`--vanilla-annual-ledger-rollover-queries` 可预览显式年度国家分类账 rollover 阶段，`--vanilla-monthly-event-queries` 可检查已翻译的 monthly pulse 事件配置，`--vanilla-yearly-event-queries` 可检查已翻译的 yearly pulse 事件配置，`--fort-maintenance-queries` 可查询直接或按已加载建筑派生的省份要塞维护阶段，`--state-maintenance-queries` 可查询调用者解析的省份州维护贡献；这些入口结合有效内容定义与调用者解析的完整输入，缺项时保留未决。完整原版报价、暂停及取消退款规则仍待接入；多订单路径已通过自制用例与受控海军队列的原版实机保存重载对照，自然生产命令创建多订单的完整链路仍待核对。范围与操作见 [原版世界载入与时间推进](docs/WORLD_INITIALIZATION.md)。

自制事件演示支持显式触发、等待选项、选项与收尾效果、保存恢复；两种输入使用相同核心，见 [事件演示](docs/EVENT_DEMO.md)。学派选择与切换也使用这一核心，属性查询读取当前选择，见 [学派变更演示](examples/school-effects/README.md)。

Godot 事件弹窗显示已加载的英文文件中的标题、正文和选项文本；缺失翻译时显示原键。弹窗随窗口大小限制边界，长正文与选项可滚动阅读，选项文字自动换行，键盘焦点会带动滚动。自制[长事件弹窗样本](examples/event-dialog/README.md)包含多段中文文本和八个长选项，用于检查本地化文本的布局与选择。

静态事件文本保留 UTF-8 与 Windows-1252 字符，并正确解码换行、制表符、引号和反斜杠；双反斜杠后的 `n` 保持字面文本，不会重复解码为换行。查询和 Godot 弹窗共享这套文本解析，样本与准确输出见[事件文本转义](examples/event-text-escapes/README.md)。

原生事件计划支持把当前首都设为国家拥有的省份；后续首都作用域立即更新，延迟变更随项目存档保存和恢复。自制[当前首都变更场景](examples/capital-relocation/README.md)演示迁移、保存和次日迁回；原首都与日期历史独立保存，EU4 `set_capital` 兼容编译尚未接入。

条件可以进入首都、所有者、职位和命名对象，读取共享世界中的地理成员关系；自制样本与操作见 [作用域条件演示](examples/scoped-conditions/README.md)。

国家是否拥有领土也由共享核心查询，已登记标签与存活国家分别处理，见 [国家领土条件演示](examples/country-existence/README.md)。[拥有省份查询](examples/owned-provinces/README.md) 可进一步检查当前成员的条件，并在归属或省份状态变化后读取新结果。[遍历与随机选择](examples/owned-iteration/README.md) 共用核心计划，保留取得顺序、实时筛选和读档后的随机状态。

事件和任务奖励可用字面文化标识执行 `add_accepted_culture` 与 `remove_accepted_culture`，修改国家已知的显式接纳名单。文化待遇、相关条件和省份属性立即读取新状态，保存恢复后保留；文化联盟的组接纳独立于显式名单。自制事件链见[运行时文化接纳](examples/accepted-cultures/README.md)。

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
