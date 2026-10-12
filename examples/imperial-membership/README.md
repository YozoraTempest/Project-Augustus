# 帝国归属查询演示

这是自制场景。AAA 的首都属于帝国，BBB 的首都明确不属于帝国。事件分别查询国家和首都省份，选项安排次日再次查询。

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/imperial-membership --trigger membership.start --choose 0:0 --save membership_waiting.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/imperial-membership --input native --load membership_waiting.json --days 1 --choose 1:0
```

输出依次记录 country_member、province_member、later_member、accepted。使用 `--entity 1` 触发首个事件可以查询 BBB，得到 country_outside 和 province_outside；后续成员事件不会通过条件。两种输入可以交叉读档。

`is_part_of_hre = yes/no` 支持国家和省份作用域。国家查询当前首都的归属，省份查询自身的归属；拥有其他帝国省份不改变首都之外的判断。兼容层使用共享核心的 `AffiliationPresent`、`With` 及取反，状态保存在 `imperial_membership` 维度中，不缓存国家归属。

原版世界入口按选定日期读取省份历史中的 `hre`。已读取且未声明归属的省份按无归属初始化；缺少历史文件、未执行的相关历史效果或失败的开局保留未知。原生场景将明确无归属的省份列入 `absent_members`，缺少条目或成员为 null 表示未知。未知查询及其否定都会报错。

当前只接入历史初始化和归属查询。`set_in_empire`、迁都时的帝国处理、皇帝与诸侯规则及帝国结算尚未实现；原生核心的成员与首都修改操作也不代办这些游戏规则。
