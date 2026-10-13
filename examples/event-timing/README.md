# 显式事件与平均时间声明

这是自制场景，包含一个虚构国家和四个只由调用触发的事件，分别没有平均时间声明、声明一天、声明一万年，以及声明带条件修正的十个月。

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/event-timing --trigger timing.3 --save timing_pending.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/event-timing --input native --load timing_pending.json --choose 0:0 --days 60
```

四个事件都会在调用时执行，平均时间不会延后本次调用，也不会安排自动重试。`country_event` 的显式 `days` 参数仍由原有延期计划处理，事件的资格和一次性记录仍按原有规则检查。

兼容层对 `is_triggered_only=yes` 事件中的 `mean_time_to_happen` 校验后消除，原生事件无需保存这份不参与执行的声明。支持单一 `days`、`months` 或 `years` 非负整数，以及 `modifier` 的字面因子和已支持条件；条件只校验，不在显式调用时求值。示例中的国库未初始化，因此实际读取国库会报错，但它位于不参与执行的平均时间修正中。

重复字段、未知字段、无因子的修正和不支持的条件仍提供来源诊断。复合时间单位、动态时间、自动事件、自然事件的MTTH概率、错峰调度与完整随机批次尚未接通。此示例不表示自然事件调度已经实现。
