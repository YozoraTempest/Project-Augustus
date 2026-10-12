# AI 事件选项

这个自制场景用同一个核心执行两种内容。AAA 由玩家控制，BBB 由 AI 控制；`scene.json` 的 `event_decisions` 显式启用加权选择，`empty` 指定没有正权重时的处理。已有场景省略这一设置时继续由调用者选择选项。

运行 BBB 的事件，再从另一种输入恢复并推进两天：

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/event-decisions --entity 1 --trigger decisions.start --save decisions_started.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/event-decisions --input native --load decisions_started.json --days 2
```

第一次的日志依次为 `queued`、`gate_clear`、`captured_0`、`child_after`、`parent`。子事件在即时效果设置 gate 后选中选项 `0`；父事件随后清除 gate，执行已接受的选项时读取到清除后的状态。两天后，另一事件按 `1:9` 权重选择一个选项，并执行 `after`。

EU4 写法中的 `ai_chance` 是选项元数据，目前支持整数 `factor` 和按顺序执行的整数条件 `modifier`。原生写法在 `option_weights.json` 中按事件、选项标识声明 `base`、`unit`、`modifiers` 和 `factor`；乘法按指定单位缩放，每一步向零截断。原生内容可以通过 `unit` 表达小数权重。条件读取共享世界状态，筛选保留原始选项编号。

`World::queue_event_choice` 接受稳定选项命令，`process_event_commands` 按接受顺序执行选项效果和 `after`。日期推进会在开始时和同一天的任务执行完毕后处理队列。已接受的选项不会重新检查显示条件；命令和随机状态一起保存，恢复时须提供相同的事件内容。`EventCatalog::with_decision_system` 可以替换整个选择执行者，新的执行者须提供标识完整配置的稳定内容键。

`empty = "first_declared"` 在无正总权重时使用第一个声明选项，即使显示条件为假；`keep_pending` 保留待选事件。玩家事件始终保留给玩家操作。使用 `--trigger decisions.zero --entity 1` 与 `--trigger decisions.zero --entity 0` 可以比较这两种情况。

加权抽签使用 Augustus 的可保存随机流，不保证与原版 EU4 的抽取序列相同。兼容输入拒绝小数因子，也会拒绝超过受支持整数幅度的修正链；诊断带原文件位置。修正阶段的可能绝对幅度上限为 300000，最终权重上限为 1000000。正总权重中含负权重、算术溢出会报错。原版小数权重的转换、极端值的整数回绕、浮点舍入、超时和完整事件上下文仍未接入。
