# 政体转换场景

这是自制的命令行测试场景。两种输入都使用核心的政体状态、执行计划和保存格式。

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/government-transitions --trigger transition.start --choose 0:0 --save transition_waiting.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/government-transitions --input native --load transition_waiting.json --days 1 --choose 1:0
```

场景先在君主制内调用转换，再切换为共和国；选项返回君主制，一天后的事件经过原住民、神权和部落君主制。同类型调用保留改革和数值，仍执行回调；跨类型调用在旧状态下执行回调，然后采用目标基础改革和活动指标。普通改革被清除。共和国传统和奉献值停用后保留为可查询值，最终活动正统性为 50。

原生输入在 `Governments.forms` 声明目标状态，通过 `ChangeGovernment` 请求转换；`before` 是在目标国家执行的回调计划。未列入目标活动集合的指标转为停用存储，目标显式声明的停用值覆盖旧值。同类型请求只执行回调。直接执行计划时，回调失败保留此前写入、停止模拟，不安装目标状态，也不能继续保存；事件分发继续遵循已有的事务规则。

兼容输入从本场景的 `common/on_actions` 读取普通和原住民回调。回调依据旧状态选择，编译为同一条核心计划。模糊、递归或含未支持脚本的回调会报错；不能用空回调代替编译失败。

本场景核对改革集合、指标存储、回调、事件和恢复。阶层重建、等级调整、自然增长、改革效果及其他政体生命周期仍未接入；本场景不能作为完整原版战役已经可运行的证明。
