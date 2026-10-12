# 地区效果与省份数值

这是自制场景，不需要 EU4 安装。兼容入口把地区块翻译成有序遍历计划，原生入口直接提供同一种计划。`test_area` 的成员顺序为 3、1、2；第一轮执行清除所有者标记，因此只有省份 3 通过筛选。

触发事件并保存：

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/area-effects --trigger area.start --save area_waiting.json
```

所有成员的荒废度变为 50，人力属性为 500000。每次增减单独限制在 0—100，EU4 写法的增量先向零截断到整数；`-50.999` 因此减少 50。查询条件保留三位小数精度。

选择修复选项，再从原生入口续跑：

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/area-effects --load area_waiting.json --choose 0:0 --save area_repaired.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/area-effects --input native --load area_repaired.json --days 2 --choose 1:0
```

荒废度变为 38，人力属性为 620000，延迟事件按 3、1、2 记录 `settled`。存档目标必须不存在。两种入口的有效内容一致，可以互相读档。

场景的 `area_scopes` 提供兼容翻译所需的成员表；实际原版入口从玩家的地区文件读取。核心 `ForEach` 使用有序实体选择器，每轮执行前读取筛选条件。核心 `Metrics` 由内容声明实体类型、整数单位、上下限和属性缩放；本例用千分位整数保存荒废度，其他原生内容可选自己的单位。未知数值不能作为零参与增减或属性计算。

当前支持已声明地区的效果块、可选 `limit`、字面省份作用域、`province_id` 和直接荒废度增减与查询。荒废度按比例接入现有省份属性。月度恢复、繁荣度、区域或大区的效果遍历尚未实现。
