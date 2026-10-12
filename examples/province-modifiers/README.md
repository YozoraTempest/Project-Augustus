# 省份修正

这个自制场景演示省份修正的查询和移除，不需要 EU4 安装。国家与首都的初始修正由场景提供；两者可以各自附着同名修正，移除其中一处不会影响另一处。

```sh
cargo run --locked -p augustus-run --bin augustus-events -- \
  --content examples/province-modifiers --input eu4 \
  --trigger province.start --choose 0:0 --save province.json

cargo run --locked -p augustus-run --bin augustus-events -- \
  --content examples/province-modifiers --input native \
  --load province.json --days 2 --choose 1:0
```

第一条命令查询首都修正，再通过选项移除 `boost`，保留 `guard`。第二条命令从原生入口读档，两天后的事件读取当前附着记录。事件移除首都的 `guard` 后，国家仍能查询到自己的同名记录，随后完成清理。最终活动修正集合为空。

兼容层将 `remove_province_modifier` 和 `has_province_modifier` 翻译为共享的 `RemoveModifier` 和 `HasModifier`。两种省份指令要求省份作用域，国家指令要求国家作用域；未知状态处理见 [事件演示](../../docs/EVENT_DEMO.md)。移除后，地方税收系数立即不再贡献属性。`add_province_modifier`、省份续期和到期翻译尚未支持；原生内容已有的通用修正操作继续可用。本例不包含完整的财政结算。
