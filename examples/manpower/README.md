# 人力存量场景

这个自制场景不需要 EU4 安装。AAA 从 1501 人开始，月度规则每月恢复 640 人，上限为 3000 人；BBB 没有省份，不执行这条恢复规则。事件可以增加超额存量，月度恢复保留已有超额。

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/manpower --trigger mp.begin --choose 0:0 --save manpower_waiting.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/manpower --input native --load manpower_waiting.json --days 20 --choose 1:0 --save manpower_low.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/manpower --load manpower_low.json --days 31 --save manpower_month.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/manpower --input native --load manpower_month.json --choose 2:0 --save manpower_zero.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/manpower --load manpower_zero.json --days 1 --choose 3:0
```

保存目标必须不存在。第三步的事件读取月结前的 501 人，输出存量已经恢复为 1141 人；最后为零。日志依次为 `per_literal`、`excess`、`excess_preserved`、`before_growth`、`zero`、`finished`。

EU4 输入的 `add_manpower` 和 `manpower` 使用千人为单位，每个字面操作数单独向零截断到整数人数。BBB 的两次 0.0005 增量各自变为零，没有合并成一人。核心的 `country_resources.manpower_milli` 用千分位保存人数，原生计划直接读写同一资源槽，不另建一份人力状态。

`monthly_stocks` 从当前属性取得收入与增长上限，保存结算记录；整项替换月度执行器时停用这套规则。本例的初值、收入和上限由自制内容明确提供。原版的完整开局人力、上限与恢复收入计算、军队补员和征募尚未接通。
