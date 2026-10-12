# 国家领土条件演示

这是自制的两个国家、一个陆地省份和一个海域。AAA 拥有陆地，BBB 已登记但没有领土。EU4 写法和原生执行计划读取同一核心状态。

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/country-existence --trigger presence.start --choose 0:0 --save presence_waiting.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/country-existence --input native --load presence_waiting.json --trigger presence.bind --entity 1 --choose 1:0 --save presence_rebound.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/country-existence --load presence_rebound.json --days 1 --choose 2:0
```

首次事件绑定 AAA，次日事件执行前重新绑定到 BBB；延期条件读取当前绑定，结束日志为 `start`、`active`、`rebound`、`later`、`done`。用 `--trigger presence.start --entity 1` 查看 BBB 时，只显示原始标识为 `1` 的无领土选项。`presence.province` 在实体 `2` 上演示省份作用域的布尔写法和字面国家查询。

核心的 `HasOwnedProvince` 查询当前国家是否至少拥有一个省份。字面国家查询通过 `ScopeSelector::Named` 选择具有明确类型的已登记实体；原生内容也可直接使用这个查询。实体名称和领土计数索引在加载时建立、归属变化时维护、读档时重建，不写入存档。

兼容输入接受 `exists = AAA` 等三位大写字母或数字标签，以及 `exists = yes/no`。标签查询不取决于当前实体类型；布尔写法在国家作用域查询当前国家，在省份作用域中 `yes` 为假、`no` 为真。它不隐式进入所有者。命名目标可使用已有作用域块 `event_target:holder = { exists = yes }`，需声明目标类型；`exists = event_target:holder`、ROOT、PREV、职位和其他标量写法尚未支持。

尚未登记的名称和同类型重名会报错。已知某国拥有省份即可返回真；没有找到已知领土、且还存在未知省份归属时会报错，取反不能把未知变成已确定的真。`known_absent_targets` 明确声明海域没有所有者。核心已有的所有权接口可更新领土关系；本演示不提供兼容的省份转让效果，也不代表转让的全部游戏后果已实现。
