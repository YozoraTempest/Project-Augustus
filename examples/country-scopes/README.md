# 国家作用域演示

场景包含两个有领土的国家和一个已登记的无领土国家，不需要 EU4 安装。`AAA = { ... }` 一类字面作用域先按国家身份查找，再执行条件或效果。国家没有省份时仍可进入；是否拥有省份由 `exists` 单独判断。

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/country-scopes --trigger scopes.start --choose 0:0 --save scopes_started.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/country-scopes --input native --load scopes_started.json --trigger scopes.reset --choose 1:0 --save scopes_waiting.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/country-scopes --load scopes_waiting.json --days 1 --choose 2:1
```

先在无领土的 CCC 中写入旗标，再从另一个事件清除它。BBB 登记的次日事件读取清除后的状态，因此只提供选项 `1`。完成后 BBB 的 `variable/seen` 为 `5000`，日志依次为 `dormant_written`、`other_country`、`dormant_checked`、`accepted`、`cleared`、`seen_cleared`、`done`。

也可以从省份进入指定国家：

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/country-scopes --trigger scopes.province --entity 3 --choose 0:0
```

这条日志由 BBB 发出，内容为 `from_province`。原生输入使用带实体类型的 `ScopeSelector::Named` 和同一个核心执行计划；保存的数据可以在两种输入之间继续读取。未登记的国家标识会报错。动态国家标识、运行时改标签和完整 `ROOT`／`FROM`／`PREV` 上下文尚未支持。
