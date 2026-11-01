# 建造演示

演示使用仓库中的自制内容，不需要 EU4 安装。它验证脚本编译、核心结算、系统替换和界面之间的完整调用过程，还不能运行 EU4 或 MEIOU & Taxes。

## 命令行运行

安装 Rust 工具链，在仓库根目录执行：

```sh
cargo run -p augustus-run --locked -- --input eu4 --system capacity --build --days 100
```

程序在两个省份分别建造市场和工坊，然后输出第 100 天的状态。初始国库为 1000，扣除 350 的费用，完工回调奖励 17.5，最终为 667.5。`--input native` 选择原生 JSON 回调；`--system fixed` 选择固定工期。

读取磁盘上的自制场景，并在建造中途保存：

```sh
cargo run -p augustus-run --locked -- --content examples/construction --system capacity --build --days 14 --save construction-save.json
cargo run -p augustus-run --locked -- --content examples/construction --system capacity --load construction-save.json --days 86
```

命令行拒绝覆盖已有存档。恢复时须选择同一内容和建造系统；内容、编译结果或系统变化会报错。这里使用项目自己的 JSON 存档格式。

## Godot 界面

使用 Godot 4.7.2 和 Rust 工具链。先在仓库根目录构建扩展，再启动建造演示：

```sh
cargo build -p augustus-godot --locked
godot --path godot res://main.tscn
```

直接运行项目会进入原版开局查看，也可点击其中的“建造演示”。演示界面初始暂停。选中省份和建筑，点击“开始建造”；可以逐日推进、连续运行或保存、恢复。选择回调格式和建造系统后，点击“重新开始”生效。界面存档写入 Godot 的用户数据目录，不修改游戏安装。

固定工期使用自制的 30 天和 70 天。按建设能力推进时，每天累加省份当前能力；“调整施工安排”会随机改变能力，用于观察后续建造和存档是否一致。这些数值与规则属于演示，不代表 EU4 原版建造机制。

Windows x86_64 已验证。扩展描述中也列出 Linux 与 macOS 的库路径，这两个平台尚未验证。

## 支持范围

自制 EU4 写法回调位于 `examples/construction/eu4`。编译器支持 `log`、`add_treasury`、`set_variable`、`change_variable`、`owner`、`capital_scope`、`if` / `limit` / `else`，以及无参数的 scripted effect 与 scripted trigger。条件支持 `always`、`NOT`、`check_variable` 和 `treasury`；变量必须事先登记，数值最多三位小数，内部用千分位整数。

`NOT` 当前只接受一个直接条件，复杂的组合条件尚未接入。演示使用的数值表示、相对日期和建造规则，不构成对原版所有运算、历法或机制的兼容承诺。

编译时绑定名称与字段，展开受支持的脚本调用，并折叠完全确定的条件。动态条件仍在执行时读取状态，效果按原顺序立即写入。未知操作、错误作用域、递归脚本和超出精度的数值都会报错。

原生 JSON 回调目前支持所有者国库、变量增量和日志。两类输入都生成核心执行计划。`ConstructionSystem` 选择整个建造实现，包含开始操作和后续结算；这是演示接口，尚未发布为稳定 Mod SDK。

Rust 调用方可以实现该 trait，通过 `Demo::embedded_with_system` 或 `Demo::from_directory_with_system` 提供自己的系统，不需要修改演示库。系统的 `id()` 应包含版本，操作语义变化时须更新；存档也会核对规则和编译后的内容。外部系统接入的示例见 `crates/augustus-demo/tests/construction.rs`。

## 验证

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build -p augustus-godot --locked
godot --headless --path godot --editor --import --quit
godot --headless --path godot --script res://tests/session.gd
godot --headless --path godot --script res://tests/world.gd
godot --headless --path godot --script res://tests/event_dialog.gd
godot --headless --path godot --script res://tests/event_text_escapes.gd
godot --headless --path godot --script res://tests/localisation_replace.gd
godot --headless --path godot --script res://tests/province_intelligence.gd
godot --headless --path godot --script res://tests/mission_claims.gd
godot --headless --path godot --script res://tests/mission_selection_import.gd
godot --headless --path godot --script res://tests/mission_province_highlights.gd
godot --headless --path godot --script res://tests/saved_subject_relations.gd
godot --headless --path godot --script res://tests/military_roster.gd
godot --headless --path godot --script res://tests/naval_constructions.gd
godot --headless --path godot --script res://tests/naval_cancellation.gd
godot --headless --path godot --script res://tests/economy.gd
godot --headless --path godot --script res://tests/ledger.gd
godot --headless --path godot --script res://tests/accepted_cultures.gd
```

Godot 检查会加载真实 Rust 扩展，验证按钮操作、查询、两类输入、存档和随机状态。测试内容均为自制样本。

`event_dialog.gd` 配合 `examples/event-dialog` 检查 UTF-8 中文标题、使用 Unicode 行分隔符的十二段正文、八个长选项的自动换行与滚动，以及 1100×720 和 800×600 视口中的弹窗边界。它也验证刷新时保留滚动位置、焦点滚动到末项、待选事件阻止时间推进，以及末项选择关闭弹窗、opaque snapshot 恢复后重新显示并再次执行效果。图形模式可附加 `-- --capture <绝对路径前缀>` 保存顶部、底部和窄视口截图。

`event_text_escapes.gd` 配合 `examples/event-text-escapes` 检查 UTF-8 BOM 与 Windows-1252 本地化中的换行、制表符、引号和字面反斜杠，验证查询与弹窗显示一致、缺失翻译保留原键，以及保存恢复待选事件后选择仍执行真实效果。

`localisation_replace.gd` 配合 `examples/localisation-replace` 检查替换目录的事件文本覆盖、替换层冲突时的原键显示，以及查询只读、待选事件保存恢复和实际选项效果。图形模式可附加 `-- --capture <绝对路径前缀>` 保存替换文本弹窗截图。

`accepted_cultures.gd` 验证文化接纳事件、后续移除事件、省份文化待遇和属性刷新，以及接纳后的保存恢复与事件续行。它使用[运行时文化接纳](../examples/accepted-cultures/README.md)中的自制 Mod。

`military_roster.gd` 验证按国家查看当前军队与舰队、团舰详情中的已知值与缺项、驻地跳转，以及推进、事件生成单位、保存恢复和新世界载入后的列表与选择状态；同时检查超出浮点精确整数范围的编号仍可区分和选中。使用图形模式时，也应检查展开列表、长名称和详情换行的实际显示。

`naval_constructions.gd` 验证显式 FIFO 存档导入、当前排期和国家筛选、完工后待造订单与军力联动，以及保存恢复、失败载入和新世界替换后的状态。图形模式还须检查队列开关、订单详情和全局导入诊断的实际显示。

`naval_cancellation.gd` 验证订单取消确认、关闭确认框时不修改世界、取消后队列与军力刷新、不退款和支出保留，以及取消后的保存恢复与剩余订单完成。它也检查国家与订单复检、无效编号拒绝、完整十进制编号传递，以及国家切换、载入和恢复后的旧确认失效。图形模式可附加 `-- --capture <绝对路径前缀>` 检查确认框及取消后的队列。

`economy.gd` 验证财政记录的当前与历史日期、已知和未知金额、月结后刷新、国家与世界切换及快照恢复。图形模式还须检查阶段选择、缺项原因换行和大额正负金额显示。

`ledger.gd` 验证当前分类账只读查询、收入与支出分类编号、六种视图、未载入与零值的区别，以及推进、恢复、国家和世界切换后的刷新与选择。图形模式还须检查折叠区、长分类条目、精确金额及详情在侧栏中的布局。样本及各向量的实际含义见[当前国家分类账样本](../examples/ledger/README.md)。

`province_intelligence.gd` 配合 `examples/province-intelligence` 验证省份详情中的发现三态和贸易中心等级，并检查切换国家、推进时间、选择事件、恢复快照后的即时刷新及滚动位置。

`mission_selection_import.gd` 验证 EU4 中途存档中的任务选择可进入 Godot 会话，并在项目快照中保留。任务选择缓存与完成记录独立，导入不会领取奖励。

`mission_province_highlights.gd` 配合 `examples/mission-highlights` 验证任务的地图目标、已知目标与未决省份并存、归属改变后的刷新、清除和国家切换，并检查目标查询不修改快照。附加 `-- --capture <绝对PNG路径>` 可在图形模式生成渲染检查截图。

可以独立测量常量条件折叠的执行成本：

```sh
cargo run -p augustus-run --bin augustus-bench --release --locked -- --blocks 512 --iterations 2000 --samples 7
```

基准比较已经绑定名称的两份计划，一份保留常量分支，一份折叠分支；每组交替运行并检查完整世界状态相等。输出包括各次耗时、中位数、单次编译耗时和计划自身持有的堆容量。堆容量不包含分配器开销、来源信息或世界状态；这是自制脚本的局部基准，不能据此推算 EU4 或大型 Mod 的整体性能。
