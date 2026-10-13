# 独立倾向临时值

这个自制场景用同一份国家数值演示临时增减、最终值限制和每月趋向零。BBB 是 AAA 的附庸，基础值由场景定义为 15；CCC 是独立国家。

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/liberty-desire --trigger ld.begin --choose 0:0 --save influence.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/liberty-desire --input native --load influence.json --days 1 --choose 1:0 --save influence.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/liberty-desire --load influence.json --days 20 --choose 2:0 --save influence.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/liberty-desire --input native --load influence.json --days 31 --choose 3:0
```

临时值增加 150 后，最终值限制为 100。减去 50 时仍为 100，再减去 100 才回到 15。之后增加 0.5，经过月份结算变为 15.4；改为负临时值后，下一次结算得到 14.5。独立国家可以保留临时写入；月份结算会清除这笔临时值，其兼容条件查询返回 0。

原生内容通过 `monthly_convergence.rules` 指定数值、目标、步长和条件；`reset_when_inactive` 决定条件不成立时是否直接写入目标。当前数值仍保存在 `metrics` 中，结算记录提供前值、后值和缺项。税收、点数收入和趋近规则共用月度执行器；同一个库存不能同时声明收入和趋近规则。选择完整的月度替代系统时，标准结算组件一起停用。

EU4 入口支持国家作用域的 `add_liberty_desire`、`liberty_desire` 和 `is_subject`。增量使用千分位，当前支持 −1000～1000 的字面数值。最终条件读取共享属性，临时值不按最终值的 0～100 范围截断。未实现的动态操作数会报告来源位置。

场景文件为项目自制，不需要 EU4 安装。原版加载器已经接入临时值与月份处理；完整独立倾向的实力、关系和其他贡献项尚未完成，相关查询保留缺项。
