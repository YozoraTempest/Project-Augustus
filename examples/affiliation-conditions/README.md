# 宗教归属条件演示

这是自制场景。国家 AAA 属于 alpha 宗教、north 分组；首都 Port 属于 gamma 宗教、south 分组。事件候选、效果分支和延期调用都从共享核心读取归属。

```sh
cargo run -p augustus-run --bin augustus-events -- --content examples/affiliation-conditions --input eu4 --candidate faith.control --save faith_pending.json
cargo run -p augustus-run --bin augustus-events -- --content examples/affiliation-conditions --input native --load faith_pending.json --choose 0:0 --save faith_waiting.json
cargo run -p augustus-run --bin augustus-events -- --content examples/affiliation-conditions --input eu4 --load faith_waiting.json --days 1 --choose 1:0
```

首个事件记录 country、province、owner，次日记录 later。两种输入生成相同执行计划，可以交叉读档。输出的可选字段 `affiliations` 显示当前归属和分组；没有这些数据的旧场景仍省略该字段。

EU4 字面条件 `religion = alpha` 和 `religion_group = north` 分别编译为核心 `AffiliationMember` 和 `AffiliationGroup`。原生条件声明 `target`、`dimension` 和 `member` 或 `group`，可以查询任何已提供的归属维度；`target` 使用 `Current`、`Owner` 或 `Capital`。

成员变化后，分组条件按当前成员重新查询。接受某宗教或某个组不改变实体本身的归属。缺少归属或分组是未知状态，查询报错；已确定的不同值才返回假。场景的 `affiliations` 使用共享核心已有的归属结构，读档时保留保存的状态。

兼容入口本轮支持国家和省份作用域的字面标识。国家标签、省份编号、作用域引用、次要宗教、dominant 等特殊值和动态表达式尚未支持，会报告来源位置。转换宗教的脚本效果和宗教专属机制尚未接入。
