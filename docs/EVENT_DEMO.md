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

## 运行规则

事件先执行 `immediate`，成功后保存当前实体和触发日期。等待期间的数值及实体关系保持实时更新；选择时执行选项效果，再执行 `after`，成功后移除对应记录。多个同类事件各有独立编号。读档不重复即时效果，重复选择、未知选项和错误实体类型会在写入前被拒绝。效果运行错误保留已经发生的写入并停止模拟，不能继续推进或保存。

效果中可以调用事件。省略 `days` 或写 `days = 0` 时，被调用事件的即时效果执行完，调用方才继续；正整数 `days` 使用现有任务队列，在指定天数后触发。当前实体身份随任务保存。嵌套调用共用执行预算和深度限制；待选编号按记录建立顺序分配。定义绑定、任务登记和恢复时会检查事件引用。

职位作用域读取当前持有人，全局目标保存实体身份。`InScope` 使用 `ScopeSelector::Office` 或 `ScopeSelector::Global` 解析目标；`BindGlobalScope` 可以覆盖同名引用，`ClearGlobalScope` 清除引用。缺少目标时跳过效果体，已存在但类型错误时停止模拟。延迟任务在执行时解析全局引用，保存恢复会保留这些引用，读档不会重新登记。

核心通过 `EventCatalog` 接收不可变定义，使用 `World::trigger_event` 和 `World::choose_event` 执行，使用 `World::restore_with_events` 重新绑定内容并恢复待选状态。事件定义共享存储，执行时直接使用编译后的计划。`World::events` 保持原有日志接口；待选记录通过 `World::pending_events` 查询。

## 当前支持范围

脚本输入支持显式触发的 `country_event`、`province_event`，要求 `id`、`is_triggered_only = yes` 和至少一个具备标量 `name` 的选项。标量 `title`、`desc`、`picture` 可读取，但演示不展示本地化文字。事件标识按输入字符串保留；命名空间的数字登记和原版选项命令格式未实现。

三个效果阶段复用已有编译器：`log`、绑定字段的国库及变量操作、`owner`、`capital_scope`、条件分支和无参数脚本调用。EU4 选项按声明顺序获得字符串标识 `0`、`1` 等；原生定义可以使用自己的选项标识。

事件调用支持匹配当前类型的 `country_event = { id = ... days = ... }` 和 `province_event = { id = ... days = ... }`。`days` 只接受非负整数；负值、小数、随机延迟及其他参数会被拒绝。原生计划使用 `TriggerEvent`，延迟执行使用 `Schedule`。

命名作用域支持 `emperor = { ... }`、`save_global_event_target_as`、`clear_global_event_target`，以及已经声明类型的 `event_target:name = { ... }`。本入口仅处理全局目标；局部事件目标、动态目标名及完整原版目录中的类型推导尚未支持。原版启动入口也能使用皇帝作用域和全局国家目标，其他未支持效果仍会报告阻塞位置。

自动候选、MTTH、AI 权重、一次性事件、过期处理、条件选项及完整 `ROOT`／`FROM`／`PREV` 上下文尚未支持。相关字段或不支持的效果会报告来源位置，整份输入编译失败。核心命令拒绝非法选择；原版某些异常选项编号会移除待选记录的行为不在本接口的支持范围内。此演示尚未接入原版战役的事件目录或 Godot 事件界面。
