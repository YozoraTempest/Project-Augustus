# 停战场景

这个自制场景不需要 EU4 安装。AAA 与 BBB 建立对称停战，AAA 同时与自身建立停战。场景从 1444.12.1 开始，期限为十二个月；周年当天仍然有效，下一次月初在到期事件执行后清理。

先触发事件，选择继续并保存：

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/truces --trigger tr.begin --choose 0:0 --save truce_waiting.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/truces --input native --load truce_waiting.json --days 365 --choose 1:0 --save truce_anniversary.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/truces --load truce_anniversary.json --days 31 --choose 2:0 --save truce_retired.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/truces --input native --load truce_retired.json --days 1 --choose 3:0
```

日志依次为 `symmetric`、`anniversary`、`before_cleanup`、`expired`、`finished`。月初事件中的查询仍为真，随后输出的 `truces` 已清理对应关系；次日查询为假。保存目标必须不存在。

EU4 输入的 `add_truce_with`、`truce_with` 和 `has_truce` 翻译成共享核心操作，原生 JSON 提供相同计划。`scene.json` 的 `truce_months` 声明默认期限，`truces.known_countries` 声明已知完整的国家关系范围；单条写入不会将其他关系自动认定为空。公开场景明确提供初始状态，原版的完整初始停战网络及战争结算尚未接通。
