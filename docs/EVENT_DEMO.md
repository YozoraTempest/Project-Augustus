# 事件演示

[省份核心场景](../examples/province-cores/README.md) 演示完整核心、领土核心、增删和跨入口读档。`add_core`、`add_territorial_core`、`remove_core`、`is_core` 与 `is_territorial_core` 支持国家和省份两种作用域，参数可以是字面标识、类型匹配的 `ROOT` 或已声明类型的全局事件引用。核心与拥有权独立；修改核心后，行政分类和相关属性读取当前关系。未实现的动态参数会报告来源位置。

[事件定位](../examples/event-locations/README.md) 演示事件与选项各自声明的省份位置、可用选项过滤及跨入口读档。EU4 入口支持固定省份编号的 `goto`。

[根作用域与宗属关系](../examples/root-scopes/README.md) 演示嵌套执行、事件接收对象、选项和延期事件的根对象，以及 `is_subject_of` 对共享关系表的实时查询。

[国家作用域场景](../examples/country-scopes/README.md) 演示字面国家选择、无领土国家的旗标读写、实时条件和跨输入读档。启动入口与事件使用同一份国家状态；`exists` 单独查询领土。

[当前君主能力](../examples/country-ruler/README.md) 演示能力条件、增减及延期事件。EU4 写法和原生计划读写同一个当前人物；变更范围由内容定义，保存恢复后继续使用相同规则。

[国家点数库存](../examples/country-power/README.md) 演示 `add_adm_power`、`add_dip_power`、`add_mil_power` 和相应条件。兼容层把字面数值翻译成共享核心操作；实体的增长上限独立于已有库存，降低上限时保留超额点数。原版点数初始化与消费系统尚未接入。

[月度君主点数](../examples/country-income/README.md) 演示焦点切换、实时收入、月初入账和跨输入读档。兼容脚本的 `monthly_adm/dip/mil` 条件读取共享属性，`national_focus` 条件及 `change_national_focus` 效果读取或修改当前焦点。自制内容通过 `monthly_accruals.rules` 选择入账公式和库存；未知收入、库存或必需上限保留缺项。

`examples/country-stability` 演示国家稳定度、正负修正和所属省份的税收属性。核心数值定义中的可选 `below_baseline` 指定低于基准值时的另一组效果；省略时，两侧继续使用同一组效果。EU4 输入把稳定度条件和效果转成同一核心操作，不需要第二套国家状态。

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/country-stability --trigger confidence.begin --choose 0:0 --save confidence-waiting.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/country-stability --input native --load confidence-waiting.json --days 1 --choose 1:0
```

首次运行后稳定度为 −1，省份 `annual_tax` 为 `11100000`；继续一天后为 +1，属性为 `13500000`。属性用百万分位表示，场景的系数由自制内容声明。日志依次为 `minimum`、`maximum`、`negative`、`positive`、`finished`。该演示验证共享状态与事件执行，不包含稳定度购买、叛乱处理或完整经济模拟。

这个自制场景不需要 EU4 安装。事件的即时效果、待选记录、选项效果和收尾效果由 Rust 核心执行；EU4 写法先编译成核心计划，原生 JSON 直接提供同一种计划。

[条件图片场景](../examples/event-pictures/README.md) 演示即时效果后的选图、待选期间保持图片和跨入口读档；当前输出资源键，尚不渲染图片。

[地区效果场景](../examples/area-effects/README.md) 演示有序遍历、每轮筛选、省份数值对属性的影响和跨入口续跑。

[政体查询场景](../examples/government-identity/README.md) 演示政体类型、已选改革、政府属性和跨入口续跑。事件与开局条件读取同一份核心状态。

[政体转换场景](../examples/government-transitions/README.md) 演示转换前回调、同类型保留、跨类型改革替换、指标初始化及停用存储，并核对延期执行与两种输入之间的恢复。

触发议会事件并查看待选项：

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --trigger augustus_council.1
```

触发后等待五天，再选择第一个选项：

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --trigger augustus_council.1 --days 5 --choose 0:0
```

输出中，国库由 10000 变为 7500，首都的 `variable/phase` 变为 3000，待选列表清空。场景字段使用千分位整数。日志依次为 `council_ready`、`harbor_funded`、`council_closed`。`--choose` 接收“待选记录编号:选项标识”；本例两个选项的标识为 `0` 和 `1`。

另一个场景演示事件之间的调用：议会选项当场触发首都事件，两天后再触发货运事件。先选择议会选项并保存：

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/event-chain --trigger augustus_council.2 --choose 0:0 --save chain_waiting.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/event-chain --input native --load chain_waiting.json --days 2 --choose 1:0 --choose 2:0
```

第一步输出一个待选省份事件和一个未来任务；第二步完成两项选择，国库为 9250，首都变量为 4000。查询中的 `pending_tasks` 是未来任务数量。

`examples/named-scopes` 演示职位与全局目标引用。先登记职位持有人和首都，再由另一个国家覆盖持有人目标；次日事件读取新的国家目标，同时仍通过原首都查找当时的所有者：

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/named-scopes --trigger augustus_reference.1 --save references_waiting.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/named-scopes --load references_waiting.json --trigger augustus_reference.2 --entity 1 --save references_rebound.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/named-scopes --input native --load references_rebound.json --days 1 --choose 0:0 --choose 1:0 --choose 2:0
```

结果中两国国库分别为 10500 和 6000，日志为 `current_chair`、`harbor_owner`。场景的 `offices` 记录职位持有人；`global_scope_types` 声明全局目标是国家还是省份，编译时据此检查作用域。空声明不会改变已有场景的内容标识。

单个议会事件可以在等待期间保存，再从原生输入恢复并选择：

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --trigger augustus_council.1 --days 2 --save waiting.json
cargo run --locked -p augustus-run --bin augustus-events -- --input native --load waiting.json --days 3 --choose 0:0
```

保存目标必须不存在。两种输入具有相同的有效内容，因此可以互相恢复；修改场景或执行计划后，旧存档会拒绝加载。`--content examples/events` 从目录加载演示，默认使用内嵌文件。选项在日期推进后执行。

`examples/event-candidates` 演示整批筛选和延迟条件。下面的列表只触发 `candidate.1`：第二个事件虽然在第一个事件执行后满足条件，但本轮筛选已经结束。

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/event-candidates --candidate candidate.1 --candidate candidate.2 --candidate candidate.false
```

延迟调用在到期时检查条件。先安排两项任务并保存，再用原生输入继续：

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/event-candidates --trigger delay.control --save delays_waiting.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/event-candidates --input native --load delays_waiting.json --days 2
```

日志依次为 `delay_start`、`direct_true`、`delay_end`，第二天追加 `was_false_now_true`。安排时满足条件、随后失去条件的另一项事件不会执行。原生条件保存在可选的 `conditions.json` 中；已有无条件场景无需增加这个文件。

`examples/once-events` 演示一次性事件。先安排两个相同的延迟调用并保存，再用原生输入继续：

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/once-events --trigger once.control --save once_waiting.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/once-events --input native --load once_waiting.json --days 2 --choose 1:0 --choose 0:0
```

第二天只出现一个 `delayed` 记录。EU4 定义使用 `fire_only_once = yes`；原生入口在可选的 `once_events.json` 中列出相应事件标识。记录在整局游戏中共享，换实体、完成选项或保存恢复都不会重新开放触发机会。`once.2` 还演示有终止条件的自调用，其日志为 `enter`、`enter`、`inner`、`outer`。

`examples/runtime-predicates` 从 1444.12.31 开始，演示国家标识和跨年条件。当天调用只执行 `before`，次日到期的两个调用只执行 `after`：

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/runtime-predicates --trigger calendar.control --save calendar_waiting.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/runtime-predicates --input native --load calendar_waiting.json --days 1 --choose 0:0 --choose 1:0 --choose 2:0
```

场景的可选 `calendar` 声明起始日期和十二个月的长度；年份条件随核心日期变化，延迟任务不会沿用安排时的判断。另一个国家的候选列表不会触发这些要求 `AAA` 身份的事件。

这个示例的兼容脚本也使用了嵌套 `hidden_effect`，编译后与原生定义得到相同的执行计划和内容标识。它不改变效果的顺序、当前作用域或事件调用；容器中的即时效果、选项、收尾和延迟调用均由已有核心执行。当前演示只处理执行，不生成原版的效果提示。

`examples/completion-records` 演示任务完成记录。先在选项中登记完成并保存，再用原生输入执行次日事件：

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/completion-records --trigger records.control --choose 0:0 --choose 1:0 --save completion_waiting.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/completion-records --input native --load completion_waiting.json --days 1 --choose 2:0
```

日志依次为 `before`、`after`、`gated`。查询中的 `completions` 给出已注册标识和各实体的完成记录；`AAA` 已完成 `sample_first`，`BBB` 仍为空。重复登记不会重复生成记录，读档也不会重放登记动作。

`examples/named-variables` 演示按实体保存的命名变量。国家和首都省份各自保存 `count`，选项修改国家值后，次日事件才满足条件：

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/named-variables --trigger numeric.control --choose 0:0 --choose 1:0 --save numeric_waiting.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/named-variables --input native --load numeric_waiting.json --days 1 --choose 2:0
```

日志依次为 `before`、`owner`、`after`、`delayed`。查询的 `named_values` 中，国家 `AAA` 的 `count` 为 1000，首都省份为 4500；演示中的整数表示千分位数值。

`examples/modifier-lifecycle` 演示国家修正的添加、续期、查询和到期。四天期限随后又添加两天期限，选项再添加永久期限；兼容规则保留四天这一较晚的有限期限：

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/modifier-lifecycle --trigger modifier.control --choose 0:0 --save modifier_waiting.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/modifier-lifecycle --input native --load modifier_waiting.json --days 4 --choose 1:0
```

日志依次为 `before`、`owner`、`after`、`expired`。最后一条在第四天出现；到期条件读取已经清理的修正状态。查询的 `modifiers` 给出定义和附着记录，结束时没有活动修正。

`examples/flag-lifecycle` 演示国家标记的设置、清除、作用域和延期查询。选项清除 `ready` 并设置 `chosen`，次日事件读取修改后的状态：

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/flag-lifecycle --trigger flags.control --choose 0:0 --save flags_waiting.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/flag-lifecycle --input native --load flags_waiting.json --days 1 --choose 1:0
```

日志依次为 `before`、`owner`、`after`、`delayed`。最后一条在次日出现；结束时两国的标记空间均已知为空。查询的 `markers` 给出按实体保存的状态。

国家作用域支持字面名称形式的 `set_country_flag`、`clr_country_flag` 和 `has_country_flag`，分别使用核心 `SetMarker` 和 `HasMarker`。重复设置保持一个标记，清除已知不存在的名称没有副作用。条件读取当前状态，选项和 `after` 的修改立即可见。动态名称、`@ROOT` 等作用域后缀、`had_country_flag` 时间条件，以及省份、人物标记的兼容写法尚未支持；未知形式报告来源位置。

原生场景通过 `markers.entities` 提供初始状态。空对象表示该实体的标记空间已知为空，未记录的名称查询为假；缺少实体条目或列入 `unknown_entities` 表示其他名称未知。明确写入 `true` 或 `false` 可以确定个别名称，同时保留其他名称未知。原生核心可以操作不同实体，状态随存档保存，恢复不会重放设置动作。

`examples/global-flags` 演示全局标记。国家和省份作用域都支持字面名称形式的 `set_global_flag`、`clr_global_flag`、`has_global_flag`，编译为核心 `SetGlobalMarker` 和 `HasGlobalMarker`。各实体读取同一份状态，清除同名国家标记不会影响全局标记。可以先运行国家事件并保存，再从原生输入继续次日的省份事件：

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/global-flags --trigger global.start --choose 0:0 --save global_waiting.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/global-flags --input native --load global_waiting.json --days 1 --choose 1:0
```

原生场景通过 `global_markers` 提供初始状态：`known: true` 表示未记录的名称为假，默认的 `known: false` 表示其他名称未知；`values` 中的明确布尔值可以确定个别名称。状态随存档保存，延期查询读取执行时的值。当前原版加载器遇到尚未执行的历史全局标记操作，或开局脚本执行失败时，保留全局状态未知。

`examples/subject-queries` 演示直接属国与类型条件。国家作用域中的 `any_subject_country` 检查当前直接成员，包括场景中声明的朝贡关系；`is_subject_of_type` 按注册标识精确匹配。下一级属国不会自动成为上一级国家的直接成员。可以先保存，再从原生输入继续延期事件：

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/subject-queries --trigger subject.start --choose 0:0 --save subject_waiting.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/subject-queries --input native --load subject_waiting.json --days 1 --choose 1:0
```

场景在 `politics.relations` 中提供关系，在 `subjects.types` 中声明类型，`subjects.known: true` 表示当前关系列表完整。列表不完整时，查询保留未知；未注册类型和错误作用域会被拒绝。旧场景缺少这项声明时，也不能据此推断国家没有属国。关系索引由当前记录建立，保存恢复后重建；延期条件读取执行时的关系。

兼容编译器通过 `with_subject_type_ids` 接收类型声明。原版加载器从本地 `common/subject_types` 和选定日期的外交历史取得初始数据，供开局条件、任务与事件使用。遇到已识别但尚未执行的历史关系变更、未知外交字段或启动失败时，不把历史记录当作完整的当前状态。当前支持成员、类型、宗主对象和数量条件；创建与解除效果，以及成员遍历效果尚未接入。

`examples/subject-counts` 演示数量条件与延期事件。`num_of_subjects = N` 接受非负整数字面量，判断国家记录的数量是否至少为 N。场景的 `subjects.counts` 以国家实体编号为键，明确提供数量；缺少记录表示未知，包括门槛为零时。数量可以与当前成员列表不同，保存和恢复会保留它，时间推进不会自行刷新。通用关系替换会撤销发生变化的宗主国数量，等待所属系统提供新记录。

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/subject-counts --trigger counts.begin --choose 0:0 --save count_waiting.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/subject-counts --input native --load count_waiting.json --days 1 --choose 1:0
```

原版加载器在外交历史完整时为各国建立初始数量；未执行的开局脚本会撤销这些记录。当前尚未实现原版数量的自动刷新时机，不能用本示例确认创建、解除或读入原版存档的完整行为。

`examples/government-metrics` 演示正统性的增减、范围限制、实时条件和延期事件：

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/government-metrics --trigger government.control --choose 0:0 --save government_waiting.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/government-metrics --input native --load government_waiting.json --days 1 --choose 1:0
```

日志依次为 `fraction`、`owner`、`after`、`delayed`，最终正统性为 `10.125`，查询中的千分位整数为 `10125`。场景通过 `governments` 提供指标定义与初始国家状态；空声明不会改变已有场景的内容标识。

兼容写法支持国家作用域中的字面小数 `add_legitimacy` 和 `legitimacy`，精确到千分位。每次增减后限制在 0 到 100；不适用正统性的国家忽略该效果。阈值条件读取活动值或明确保存的停用值；已知不适用且没有保存值时，沿用零的兼容规则。未知选择或明确未知的数值仍会报告错误。国家标识比较、动态参数、缩放效果及其他政体指标的兼容写法尚未支持。

原生计划使用 `NumericSlot::Government`、`UpdateNumeric`、`NumericCompare` 和 `GovernmentMetricActive`。指标必须先在 `Governments.metrics` 注册，并在国家状态中选为活动指标。原生直接读写不适用的指标会报告错误；`Set` 可以确定活动指标的未知值，其他运算要求已有值。属性和月结读取同一份实时状态，修改无需同步派生值缓存；保存待选事件和延期任务后，恢复不会重放修改。

[停用指标场景](../examples/government-stored-values/README.md) 演示 `inactive_metrics`、`GovernmentMetricStored` 和只读数值槽 `StoredGovernment`。停用值可以被查询，不参与属性计算，也不能通过数值效果修改。正统性条件翻译已经更新，此前版本的正统性场景存档会因内容标识不同而被拒绝，需要重新创建。

`examples/modifier-duration-text` 演示修正效果的期限说明：

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/modifier-duration-text --trigger description.control --choose 0:0 --save description_waiting.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/modifier-duration-text --input native --load description_waiting.json --days 1 --choose 1:0
```

`add_country_modifier` 中的字面本地化键 `desc` 翻译为 `AttachModifier.duration_label`，用于替换效果的期限说明；真实期限仍由 `duration` 决定。待选记录的可选查询字段 `modifier_duration_labels` 按选项给出其直接声明的键，包括嵌套计划，不预判条件是否满足，也不展开被调用事件。查询尚未解析本地化文本。说明保留在内容定义和保存的延期计划中，不写入活动修正，也不影响数值、续期或到期。消息参数与动态说明表达式尚未支持。

## 运行规则

事件先执行 `immediate`，成功后保存当前实体和触发日期。等待期间的数值及实体关系保持实时更新；选择时执行选项效果，再执行 `after`，成功后移除对应记录。多个同类事件各有独立编号。读档不重复即时效果，重复选择、未知选项和错误实体类型会在写入前被拒绝。效果运行错误保留已经发生的写入并停止模拟，不能继续推进或保存。

效果中可以调用事件。省略 `days` 或写 `days = 0` 时，先检查被调用事件的触发条件；满足条件才执行即时效果，完成后调用方继续。正整数 `days` 使用任务队列，到期时检查条件。当前实体身份随任务保存。嵌套调用共用执行预算和深度限制；待选编号按记录建立顺序分配。定义绑定、任务登记和恢复时会检查事件引用。

`EventCandidates` 先筛选完整列表，再按输入顺序执行入选事件，本批次不会因即时效果重新筛选触发条件。`TryEvent` 检查调用时的条件；`World::trigger_event`、`TriggerEvent` 和命令行 `--trigger` 可以跳过这些条件。触发条件属于不可变内容，恢复时重新绑定，内容校验也包含这些条件。

所有入口都遵守一次性记录。候选事件真正执行前也会检查记录，因此重复候选或同批次中的嵌套调用不会在登记后再次执行。即时效果成功执行并建立待选记录后才登记；即时效果中的有限自调用可以先完成内层事件，外层随后继续。待选期间已不能重复触发，条件不满足的调用不登记。计划中的重复调用会跳过并继续；直接调用 `World::trigger_event` 或 `--trigger` 会报告事件已经发生，保持世界可用。`World::fired_events` 可查询记录。

职位作用域读取当前持有人，全局目标保存实体身份。`InScope` 使用 `ScopeSelector::Office` 或 `ScopeSelector::Global` 解析目标；`BindGlobalScope` 可以覆盖同名引用，`ClearGlobalScope` 清除引用。缺少目标时跳过效果体，已存在但类型错误时停止模拟。延迟任务在执行时解析全局引用，保存恢复会保留这些引用，读档不会重新登记。

核心通过 `EventCatalog` 接收不可变定义，使用 `World::trigger_event` 和 `World::choose_event` 执行，使用 `World::restore_with_events` 重新绑定内容并恢复待选状态。事件定义共享存储，执行时直接使用编译后的计划。`World::events` 保持原有日志接口；待选记录通过 `World::pending_events` 查询。

## 效果预览

`examples/effect-previews` 同时提供脚本与原生定义：

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/effect-previews --trigger preview.control --save preview_pending.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/effect-previews --input native --load preview_pending.json --choose 0:0 --save preview_waiting.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/effect-previews --load preview_waiting.json --days 1 --choose 1:0 --save preview_live.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/effect-previews --load preview_live.json --days 2
```

`tooltip = { ... }` 编译为核心 `Preview`。内部计划保留在事件内容或延期任务中，供描述生成使用；执行到这个节点时跳过整个内部计划，不改变数值、修正或标记，不切换作用域、不求值条件、不消耗随机数，也不安排或调用事件。示例中的预览声明变量增加 9、添加修正和触发事件，选择后只有实际的变量增加 1 与延期调用生效。

内部语法、作用域和内容引用仍需有效，未知效果不会因位于预览中而被忽略。启动加载会登记预览引用的事件，但不会因此触发它们。原生内容可以直接使用 `{"Preview": {"plan": [...]}}`，与兼容输入共用执行和存档逻辑。程序可从 `World::event_catalog()` 读取完整计划；演示的期限说明查询也包含预览里直接声明的键，读取查询不会执行预览。当前没有实现自动效果描述、本地化渲染或 Godot 事件界面；事件调用中的标量 `tooltip` 参数仍未支持。

## 自定义提示

`examples/custom-descriptions` 提供带重复提示、隐藏提示及条件提示的两种内容定义：

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/custom-descriptions --trigger text.control --save text_pending.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/custom-descriptions --input native --load text_pending.json --choose 0:0 --save text_waiting.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/custom-descriptions --load text_waiting.json --days 1 --choose 1:0
```

`custom_tooltip = 字面本地化键` 翻译成 `Description`，保留原有顺序和重复项。它不修改模拟状态、不发出日志，也不消耗随机数。在 `hidden_effect` 中声明的提示标记为隐藏；预览容器里的提示仍保留，但预览内部的实际效果不会执行。国家和省份作用域都可声明提示，键必须非空；动态参数、作用域后缀、直接本地化表达式及对象写法尚未支持。

原生写法为 `{"Description": {"key": "提示键"}}`；需要隐藏时添加 `"hidden": true`。提示随事件内容和保存的延期计划保留。待选查询的可选字段 `description_keys` 按可用选项收集非隐藏键，包含嵌套计划并保留重复项；选项的显示条件会求值，效果内部的条件不会求值，也不展开被调用事件，因此返回的是声明集合，不是当前条件下的最终显示文本。没有提示时省略字段，现有查询仍保持原格式。文本渲染与动态本地化尚未接入。

## 当前支持范围

显式宣战理由支持正向及反向授予、按类型移除、重新授予和期限维护。兼容层从 `common/cb_types` 读取定义，将效果和受支持的保留条件翻译成核心记录与计划；自动生成和宣战权限尚未接入。原生写法、跨输入读档和月初查询顺序见 [宣战理由记录演示](../examples/justifications/README.md)。

`exists` 支持字面国家标签和 `yes/no`，翻译为共享核心的领土查询。明确无领土、标签未登记与省份资料不完整分别处理；省份作用域的布尔写法不会隐式查询所有者。原生写法、命名目标块和跨输入保存恢复见 [国家领土条件演示](../examples/country-existence/README.md)。

国家条件中的 `any_owned_province = { ... }` 在当前拥有省份上检查内部条件，翻译为共享核心 `AnyOwnedProvince`；空集合返回假。归属和省份状态变化后立即使用新值，未知归属不能被否定条件当作不存在。原生写法与跨输入读档见 [拥有省份查询](../examples/owned-provinces/README.md)。

`every_owned_province` 与 `random_owned_province` 翻译为共享核心的拥有省份效果。前者进入时捕获取得顺序，逐项执行前读取筛选；后者筛选后执行一个随机成员。两者支持可选 `limit`、嵌套正文和延期调用，存档保留取得顺序与随机状态。用法及完整领土转移的限制见 [遍历与随机选择场景](../examples/owned-iteration/README.md)。

选项可声明独立的 `trigger`，原生内容在可选的 `option_conditions.json` 中提供相同条件。待选查询按当前状态筛选选项，保留原始标识；没有可用选项时返回空列表。选项命令执行有效的原始标识，不重新检查显示条件。国家作用域的 `ai = yes/no` 读取明确的控制身份，未指定时报告未知。自制场景、跨输入读档和查询／命令的职责见 [条件选项演示](../examples/conditional-options/README.md)。

脚本输入支持显式触发的 `country_event`、`province_event`，要求 `id`、`is_triggered_only = yes` 和至少一个具备标量 `name` 的选项。`fire_only_once` 接受 `yes`、`no`，省略时允许重复触发。标量 `title`、`desc`、`picture` 可读取，但演示不展示本地化文字。事件标识按输入字符串保留；命名空间的数字登记和原版选项命令格式未实现。

三个效果阶段复用已有编译器：`log`、绑定字段的国库操作、命名变量操作、`owner`、`capital_scope`、条件分支和无参数脚本调用。EU4 选项按声明顺序获得字符串标识 `0`、`1` 等；原生定义可以使用自己的选项标识。

连续的 `if`、`else_if`、`else` 只执行首个成立的分支。`if` 和 `else_if` 都须以 `limit` 开头，后续分支必须紧邻；空的成立分支也会结束这条分支链。所有分支均须通过语法与引用检查，恒假或不会执行的分支不能藏有未支持内容。

变量支持 `set_variable`、`change_variable`、`subtract_variable`、`multiply_variable`、`divide_variable`，以及 `check_variable` 的大于等于判断和 `is_variable_equal` 的相等判断。当前接受恰好一个 `which` 和一个字面 `value`，最多三位小数；动态名称、变量作为操作数和其他参数形式尚未支持。未显式绑定到状态字段的名称编译为命名状态；已有 `variable/名称` 字段绑定保持原执行计划与存档内容标识。除零、整数溢出和非法缩放会报错。

原生数值操作使用 `UpdateNumeric`、`NumericCompare` 和 `NumericSlot::Named`，可通过 `NumericSlot::Field` 操作已有字段。乘除指令显式提供正整数 `scale`，结果向零截断。`NamedValues.entities` 的空对象声明该实体的变量空间已知为空，未赋值的名称读为零；缺少实体条目表示未知。`unknown_entities` 可保留其他名称未知，同时保存个别已确定值。`Set` 能写入一个已知值，其他运算需要先取得已知旧值。场景通过可选的 `named_values` 提供初始状态，保存恢复不会重复初始化。

国家修正支持 `add_country_modifier`、`remove_country_modifier` 和 `has_country_modifier`。添加接受 `name`、字面整数 `duration`、可选的 `hidden` 和字面期限说明键 `desc`，每个参数最多一次；`-1` 表示永久，正数为有效天数，零或省略期限在次日清理。重复添加保留较晚的有限到期日，有限期限与永久期限混用时采用有限期限。未知字面名称的兼容查询为假，添加与移除要求名称已注册；动态名称、动态说明和消息参数以及其他数值形式尚未支持。独立使用编译器时通过 `with_modifier_ids` 提供名称集合，演示场景从 `modifiers.definitions` 提供。

省份修正支持 `has_province_modifier` 和 `remove_province_modifier`，使用同一核心组件和名称集合。省份指令要求省份作用域，国家指令要求国家作用域；同名修正按实体保存，移除不会影响所有者或首都。未知名称查询、注册检查和未知附着处理沿用上述规则。[省份修正场景](../examples/province-modifiers/README.md) 演示实时查询、移除和两种输入之间的读档。省份添加、续期和到期翻译尚未实现。

原生指令为 `AttachModifier`、`RemoveModifier` 和 `HasModifier`，可以操作核心的不同实体。添加的 `duration` 为正整数或空值；`renewal: Replace` 采用新期限，`KeepLatestDated` 提供上述兼容续期行为。`expires_on` 是首次失效的模拟日；到期清理先于同日任务和月结，属性计算从当前附着记录取值。日期索引在读档时重建，已移除或续期的旧期限不会误删当前记录。

修正的 `unresolved` 保存按实体区分的未知名称，`unknown_entities` 表示整个附着状态未知，`unresolved_properties` 保存定义中未确定的属性。未知状态不能按缺少修正处理；需要保留旧期限的续期也不能覆盖未知旧期限。原生替换和明确移除可以解决单个未知名称，整个实体未知时须先提供完整状态。

事件调用支持匹配当前类型的 `country_event = { id = ... days = ... }` 和 `province_event = { id = ... days = ... }`。`days` 只接受非负整数；负值、小数、随机延迟及其他参数会被拒绝。原生条件调用使用 `TryEvent`，延迟执行使用 `Schedule`。已绑定的无条件调用可以编译成直接执行计划。

事件 `trigger` 支持已有的常量、变量、国库和无参数脚本条件，以及国家作用域的 `is_emperor`、`is_emperor_of_china`；职位条件读取当前持有人。`tag`／`TAG` 比较当前国家的字面标识，要求三位大写字母或数字；动态 `ROOT`、`FROM`、`PREV` 标识及国家标签变更尚未支持。`is_year` 表示当前年份大于或等于给定正整数，要求场景已提供日历。原生条件分别使用 `EntityNamed`、`YearAtLeast`。

`religion = 字面宗教标识`、`religion_group = 字面分组标识` 支持国家和省份作用域，分别编译为核心 `AffiliationMember`、`AffiliationGroup`。查询从当前 `affiliations` 读取成员及其分组，不使用接受政策，也不缓存开局值。未知归属或分组会报错，不能通过取反变成已确定的真值。国家标签、省份编号、作用域引用、次要宗教、dominant 等特殊值和动态表达式尚未支持。自制场景、原生写法和交叉读档命令见 [宗教归属条件演示](../examples/affiliation-conditions/README.md)。

国家作用域的 `has_religious_school = yes/no` 查询当前学派是否存在，`religious_school = { group = 字面宗教组 school = 字面学派 }` 同时比较所选学派及其分组。明确无学派与未知数据分别表示，原生内容复用 `AffiliationPresent`、成员和分组查询。输入格式、演示和当前限制见 [宗教学派查询演示](../examples/religious-schools/README.md)。

`set_religious_school = { group = 字面宗教组 school = 字面学派 }` 支持国家作用域。当前国教属于指定组时设置或替换学派，其他已知宗教组不执行修改；未知国教组报错。学派须已注册且分组相符，重复或未注册定义使用严格诊断。独立编译器通过 `with_religious_school_groups` 提供定义，事件演示从场景的归属数据提供。原生内容使用 `SetAffiliation`，可设置已注册成员或以 `null` 明确清除归属；EU4 的分组限制由兼容编译器翻译成条件。跨输入保存和实时属性查询见 [学派变更演示](../examples/school-effects/README.md)。运行时宗教转换、学者邀请及清除学派的兼容语法尚未接入。

`AND`、`OR`、`NOT` 及其小写写法可以嵌套，条件按声明顺序短路求值。多项 `NOT` 要求内部每项都不成立；未知触发器即使出现在常量分支中，也会报告编译错误。原生组合使用 `All`、`Any` 和 `Not`。

条件块支持 `capital_scope`、`owner`、`emperor` 和已声明类型的 `event_target:name`；省份作用域支持字面 `area`、`region`、`superregion`、`continent`。条件按当前实体关系和地理状态求值，命名目标保留实体身份。缺失对象返回假，未确定的关系或成员数据报告错误；原生条件分别使用 `With`、`InScope`、`GeographicMember`。初始状态、缺失关系声明和跨输入读档见 [作用域与地理条件演示](../examples/scoped-conditions/README.md)。

`has_dlc` 使用固定终版、全部 DLC 基线，识别 22 个玩法资料片以及 American Dream、Purple Phoenix、Star and Crescent、Women in History、Dharma 的两个单位包。已识别名称编译为真，名称大小写或其他名称不匹配会报错。这只表示所选规则基线的内容可用性，各机制与效果的支持范围仍分别校验。

国家作用域支持 `complete_mission = 标识` 和 `mission_completed = 标识`，分别编译为核心 `RecordCompletion` 和 `CompletionRecorded`。登记只修改当前国家的记录，不检查任务条件、不执行奖励；条件读取执行时的状态。原生场景通过 `completions.identifiers` 注册标识，通过 `completions.entities` 提供已知记录；缺少实体条目表示状态未知，空列表表示已知未完成。标识要求已注册且非空，动态名称和错误作用域会报错；本入口对不存在的任务名称使用严格诊断，不沿用原版的假值查询。直接使用 `ScriptCompiler` 时，通过 `with_mission_ids` 提供注册标识。任务可用性由 `objectives` 保存，`has_mission` 读取当前选择，`swap_non_generic_missions` 显式重新选择且保留完成记录。任务完成条件、玩家领取奖励与任务界面尚未实现。自制示例见 [任务选择](../examples/objective-selection/README.md)。

原版世界入口可以读取 `on_startup` 的 `events` 列表和普通国家事件调用，只绑定直接及间接引用的受支持事件，先处理候选列表，再运行启动效果。启动事件使用已有脚本库展开无参数条件和效果宏，宏中的事件引用也会继续绑定。文件次序、重名处理、原生写法和跨输入读档见 [脚本宏演示](../examples/scripted-events/README.md)。其他事件字段及未注册状态字段仍会报告阻塞来源。启动处理未完成时，候选事件和其他启动写入一起撤销，避免留下部分初始化结果。

命名作用域支持 `emperor = { ... }`、`save_global_event_target_as`、`clear_global_event_target`，以及已经声明类型的 `event_target:name = { ... }`。本入口仅处理全局目标；局部事件目标、动态目标名及完整原版目录中的类型推导尚未支持。原版启动入口也能使用皇帝作用域和全局国家目标，其他未支持效果仍会报告阻塞位置。

`is_part_of_hre = yes/no` 支持省份与国家条件，国家条件读取当前首都的帝国归属。历史初始化、未知状态和跨输入保存示例见 [帝国归属查询](../examples/imperial-membership/README.md)。迁都及加入、退出帝国的兼容效果尚未接入。

`ai_chance` 翻译为独立的选项权重，目前支持整数因子与有序条件修正，小数因子会报告来源错误。场景可显式启用可替换的 AI 选择执行者，在触发时生成稳定选项命令，随后执行效果与 `after`；队列和随机状态随存档恢复。用法、原生权重写法及随机序列、数值范围的限制见 [AI 事件选项](../examples/event-decisions/README.md)。原版启动入口在已提供玩家／AI 控制名单时启用选择；没有名单的旧入口保留手动待选行为。

`add_republican_tradition`、`add_devotion` 使用共享政体指标的字面增减，查询可以读取保留的停用值，增减只作用于活动指标。跨类型进入时的数值规则由内容声明；普通共和国保留高于进入下限的传统，神权奉献值使用固定进入值。玩家、AI、延期与读档示例见 [共和传统与奉献值](../examples/republic-metrics/README.md)。原版开局初值、自然增长及完整属性缩放尚未接通。

周期自动候选、其他 `on_action` 调度、MTTH、过期处理及完整 `ROOT`／`FROM`／`PREV` 上下文尚未支持。相关字段或不支持的效果会报告来源位置，整份输入编译失败。核心命令拒绝非法选择；原版某些异常选项编号会移除待选记录的行为不在本接口的支持范围内。完整原版战役事件和 Godot 事件界面仍未跑通。
