# 事件演示

这个自制场景不需要 EU4 安装。事件的即时效果、待选记录、选项效果和收尾效果由 Rust 核心执行；EU4 写法先编译成核心计划，原生 JSON 直接提供同一种计划。

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

## 运行规则

事件先执行 `immediate`，成功后保存当前实体和触发日期。等待期间的数值及实体关系保持实时更新；选择时执行选项效果，再执行 `after`，成功后移除对应记录。多个同类事件各有独立编号。读档不重复即时效果，重复选择、未知选项和错误实体类型会在写入前被拒绝。效果运行错误保留已经发生的写入并停止模拟，不能继续推进或保存。

效果中可以调用事件。省略 `days` 或写 `days = 0` 时，先检查被调用事件的触发条件；满足条件才执行即时效果，完成后调用方继续。正整数 `days` 使用任务队列，到期时检查条件。当前实体身份随任务保存。嵌套调用共用执行预算和深度限制；待选编号按记录建立顺序分配。定义绑定、任务登记和恢复时会检查事件引用。

`EventCandidates` 先筛选完整列表，再按输入顺序执行入选事件，本批次不会因即时效果重新筛选触发条件。`TryEvent` 检查调用时的条件；`World::trigger_event`、`TriggerEvent` 和命令行 `--trigger` 可以跳过这些条件。触发条件属于不可变内容，恢复时重新绑定，内容校验也包含这些条件。

所有入口都遵守一次性记录。候选事件真正执行前也会检查记录，因此重复候选或同批次中的嵌套调用不会在登记后再次执行。即时效果成功执行并建立待选记录后才登记；即时效果中的有限自调用可以先完成内层事件，外层随后继续。待选期间已不能重复触发，条件不满足的调用不登记。计划中的重复调用会跳过并继续；直接调用 `World::trigger_event` 或 `--trigger` 会报告事件已经发生，保持世界可用。`World::fired_events` 可查询记录。

职位作用域读取当前持有人，全局目标保存实体身份。`InScope` 使用 `ScopeSelector::Office` 或 `ScopeSelector::Global` 解析目标；`BindGlobalScope` 可以覆盖同名引用，`ClearGlobalScope` 清除引用。缺少目标时跳过效果体，已存在但类型错误时停止模拟。延迟任务在执行时解析全局引用，保存恢复会保留这些引用，读档不会重新登记。

核心通过 `EventCatalog` 接收不可变定义，使用 `World::trigger_event` 和 `World::choose_event` 执行，使用 `World::restore_with_events` 重新绑定内容并恢复待选状态。事件定义共享存储，执行时直接使用编译后的计划。`World::events` 保持原有日志接口；待选记录通过 `World::pending_events` 查询。

## 当前支持范围

脚本输入支持显式触发的 `country_event`、`province_event`，要求 `id`、`is_triggered_only = yes` 和至少一个具备标量 `name` 的选项。`fire_only_once` 接受 `yes`、`no`，省略时允许重复触发。标量 `title`、`desc`、`picture` 可读取，但演示不展示本地化文字。事件标识按输入字符串保留；命名空间的数字登记和原版选项命令格式未实现。

三个效果阶段复用已有编译器：`log`、绑定字段的国库操作、命名变量操作、`owner`、`capital_scope`、条件分支和无参数脚本调用。EU4 选项按声明顺序获得字符串标识 `0`、`1` 等；原生定义可以使用自己的选项标识。

变量支持 `set_variable`、`change_variable`、`subtract_variable`、`multiply_variable`、`divide_variable`，以及 `check_variable` 的大于等于判断和 `is_variable_equal` 的相等判断。当前接受恰好一个 `which` 和一个字面 `value`，最多三位小数；动态名称、变量作为操作数和其他参数形式尚未支持。未显式绑定到状态字段的名称编译为命名状态；已有 `variable/名称` 字段绑定保持原执行计划与存档内容标识。除零、整数溢出和非法缩放会报错。

原生数值操作使用 `UpdateNumeric`、`NumericCompare` 和 `NumericSlot::Named`，可通过 `NumericSlot::Field` 操作已有字段。乘除指令显式提供正整数 `scale`，结果向零截断。`NamedValues.entities` 的空对象声明该实体的变量空间已知为空，未赋值的名称读为零；缺少实体条目表示未知。`unknown_entities` 可保留其他名称未知，同时保存个别已确定值。`Set` 能写入一个已知值，其他运算需要先取得已知旧值。场景通过可选的 `named_values` 提供初始状态，保存恢复不会重复初始化。

国家修正支持 `add_country_modifier`、`remove_country_modifier` 和 `has_country_modifier`。添加接受 `name`、字面整数 `duration` 和可选的 `hidden`，每个参数最多一次；`-1` 表示永久，正数为有效天数，零或省略期限在次日清理。重复添加保留较晚的有限到期日，有限期限与永久期限混用时采用有限期限。未知字面名称的兼容查询为假，添加与移除要求名称已注册；动态名称、说明和消息参数、其他数值形式以及省份效果尚未支持。独立使用编译器时通过 `with_modifier_ids` 提供名称集合，演示场景从 `modifiers.definitions` 提供。

原生指令为 `AttachModifier`、`RemoveModifier` 和 `HasModifier`，可以操作核心的不同实体。添加的 `duration` 为正整数或空值；`renewal: Replace` 采用新期限，`KeepLatestDated` 提供上述兼容续期行为。`expires_on` 是首次失效的模拟日；到期清理先于同日任务和月结，属性计算从当前附着记录取值。日期索引在读档时重建，已移除或续期的旧期限不会误删当前记录。

修正的 `unresolved` 保存按实体区分的未知名称，`unknown_entities` 表示整个附着状态未知，`unresolved_properties` 保存定义中未确定的属性。未知状态不能按缺少修正处理；需要保留旧期限的续期也不能覆盖未知旧期限。原生替换和明确移除可以解决单个未知名称，整个实体未知时须先提供完整状态。

事件调用支持匹配当前类型的 `country_event = { id = ... days = ... }` 和 `province_event = { id = ... days = ... }`。`days` 只接受非负整数；负值、小数、随机延迟及其他参数会被拒绝。原生条件调用使用 `TryEvent`，延迟执行使用 `Schedule`。已绑定的无条件调用可以编译成直接执行计划。

事件 `trigger` 支持已有的常量、变量、国库和无参数脚本条件，以及国家作用域的 `is_emperor`、`is_emperor_of_china`；职位条件读取当前持有人。`tag`／`TAG` 比较当前国家的字面标识，要求三位大写字母或数字；动态 `ROOT`、`FROM`、`PREV` 标识及国家标签变更尚未支持。`is_year` 表示当前年份大于或等于给定正整数，要求场景已提供日历。原生条件分别使用 `EntityNamed`、`YearAtLeast`。

`AND`、`OR`、`NOT` 及其小写写法可以嵌套，条件按声明顺序短路求值。多项 `NOT` 要求内部每项都不成立；未知触发器即使出现在常量分支中，也会报告编译错误。原生组合使用 `All`、`Any` 和 `Not`。

`has_dlc` 使用固定终版、全部 DLC 基线，识别 22 个玩法资料片以及 American Dream、Purple Phoenix、Star and Crescent、Women in History、Dharma 的两个单位包。已识别名称编译为真，名称大小写或其他名称不匹配会报错。这只表示所选规则基线的内容可用性，各机制与效果的支持范围仍分别校验。

国家作用域支持 `complete_mission = 标识` 和 `mission_completed = 标识`，分别编译为核心 `RecordCompletion` 和 `CompletionRecorded`。登记只修改当前国家的记录，不检查任务条件、不执行奖励；条件读取执行时的状态。原生场景通过 `completions.identifiers` 注册标识，通过 `completions.entities` 提供已知记录；缺少实体条目表示状态未知，空列表表示已知未完成。标识要求已注册且非空，动态名称和错误作用域会报错；本入口对不存在的任务名称使用严格诊断，不沿用原版的假值查询。直接使用 `ScriptCompiler` 时，通过 `with_mission_ids` 提供注册标识。任务树、可用性、玩家领取奖励和分支切换尚未实现。

原版世界入口可以读取 `on_startup` 的 `events` 列表和普通国家事件调用，只绑定直接及间接引用的受支持事件，先处理候选列表，再运行启动效果。该入口中的事件脚本宏、其他事件字段及未注册状态字段仍会报告阻塞来源。启动处理未完成时，候选事件和其他启动写入一起撤销，避免留下部分初始化结果。

命名作用域支持 `emperor = { ... }`、`save_global_event_target_as`、`clear_global_event_target`，以及已经声明类型的 `event_target:name = { ... }`。本入口仅处理全局目标；局部事件目标、动态目标名及完整原版目录中的类型推导尚未支持。原版启动入口也能使用皇帝作用域和全局国家目标，其他未支持效果仍会报告阻塞位置。

周期自动候选、其他 `on_action` 调度、MTTH、AI 权重、过期处理、条件选项及完整 `ROOT`／`FROM`／`PREV` 上下文尚未支持。相关字段或不支持的效果会报告来源位置，整份输入编译失败。核心命令拒绝非法选择；原版某些异常选项编号会移除待选记录的行为不在本接口的支持范围内。完整原版战役事件和 Godot 事件界面仍未跑通。
