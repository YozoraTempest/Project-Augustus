# 国库余额与月度税收

这是自制场景，包含两个虚构国家和一个省份。AAA 初始拥有 0.999 金币，BBB 拥有 0.500 金币；省份提供每年 12 金币的固定税收，仅 AAA 参与月度税收结算。场景没有贷款系统。

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/treasury --trigger cash.begin --save cash_pending.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/treasury --input native --load cash_pending.json --choose 0:0 --save cash_waiting.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/treasury --load cash_waiting.json --days 20 --save cash_month.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/treasury --input native --load cash_month.json --choose 1:0 --days 31 --save cash_second.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/treasury --load cash_second.json --choose 2:0 --days 1 --choose 3:0
```

`scene.json` 显式指定 `treasury_slot = {"Resource": "Treasury"}`，将兼容脚本的国库访问绑定到共享资源余额。原生事件使用同一资源槽。没有指定此项的已有场景继续使用注册的 `treasury` 字段。

兼容输入支持国家作用域的字面 `add_treasury` 和 `treasury` 条件。金额使用千分位，每个六位小数字面量分别截断，`0.0005` 连续执行两次仍不会累计。余额可以为负。条件还可以与字面国家、国家类型的 `ROOT`、`emperor` 或已声明类型的全局事件目标比较；比较读取双方的实时余额。未知余额会报错，不能以零代替。

事件首先检查 0.999 与 0.500 的方向，增加至 1.000 后扣至 −0.500，再安排跨月事件。月初事件先读到 −0.500，随后税收将余额增至 0.500。存档同时保存余额、延期事件、待选项和税收凭据；历史凭据不会随之后的支出修改。

本场景只演示余额操作和已提供年度税收属性的结算。原版初始国库、完整收支、贷款、利息、破产、动态金额及其他财政效果尚未实现。
