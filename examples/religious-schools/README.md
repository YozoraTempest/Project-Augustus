# 宗教学派查询演示

这是自制场景。国家 AAA 选择了 family 组的 first 学派，BBB 确定没有学派。两个国家的事件候选分别验证学派查询和无学派状态，AAA 的选项安排次日查询。

```sh
cargo run -p augustus-run --bin augustus-events -- --content examples/religious-schools --input eu4 --candidate school.selected --candidate school.absent --save school_first.json
cargo run -p augustus-run --bin augustus-events -- --content examples/religious-schools --input native --load school_first.json --entity 1 --candidate school.selected --candidate school.absent --choose 0:0 --choose 1:0 --save school_waiting.json
cargo run -p augustus-run --bin augustus-events -- --content examples/religious-schools --input eu4 --load school_waiting.json --days 1 --choose 2:0
```

输出依次记录 selected、absent、later。两种输入可以交叉读档。

国家条件 `has_religious_school = yes/no` 编译为共享核心 `AffiliationPresent` 及其取反；`religious_school = { group = family school = first }` 编译为成员与分组条件。原生内容使用 `religious_school` 归属维度，学派名称及其宗教组来自 `groups`。明确没有归属的实体列在 `absent_members`，不得同时出现在 `members` 中；缺少条目或成员为 null 仍表示未知，查询报错。

原版世界入口读取宗教组中的 `religious_schools` 定义和国家历史中的学派选择，按历史日期及声明顺序处理。学派只在当前宗教组适用时选中；同组改宗保留选择，跨组改宗清除。重名定义、未登记名称、缺失或未支持的条件历史保留诊断和未知状态。

当前支持学派登记、初值和上述字面查询。学派属性、学者邀请、学派关系、动态参数、比较另一国家的学派，以及 `set_religious_school` 运行时效果尚未接入。
