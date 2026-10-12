# 政体状态查询

自制场景演示政体类型、已选改革和政府属性查询。两种事件输入分别使用 EU4 脚本和原生计划，共用 `scene.json` 的核心状态。

```sh
cargo run --locked -p augustus-run --bin augustus-events -- \
  --content examples/government-identity --input eu4 \
  --trigger identity.start --choose 0:0 --save identity.json

cargo run --locked -p augustus-run --bin augustus-events -- \
  --content examples/government-identity --input native \
  --load identity.json --days 1 --choose 1:0
```

日志依次为 `country`、`owner`、`later`。国家条件、首都所有者和延期事件都读取当前政体状态；查询不要求国家拥有活动数值指标。

原生计划使用 `GovernmentKind`、`GovernmentReform`、`GovernmentFeature`。改革标识必须在 `Governments.reforms` 注册。未选改革和已知缺少的属性返回假；未知状态、属性冲突和未解析的属性声明会报错。

EU4 入口支持字面值 `government`、`has_reform`、`has_government_attribute`。动态政体引用、特殊政体别名与旧改革别名尚未支持；政体切换和改革增删效果也未接入。
