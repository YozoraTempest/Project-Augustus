# 事件定位

这个自制场景演示事件和选项各自声明的省份位置。EU4 格式使用 `goto = 8`；原生格式在 `locations.json` 中用省份名称引用世界里的对象。两种入口运行同一套核心，可以互相读取存档。

先打开事件，再从另一入口恢复：

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/event-locations --trigger locations.visit --save locations_waiting.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/event-locations --input native --load locations_waiting.json --days 3 --save locations_later.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/event-locations --load locations_later.json --choose 0:0
```

JSON 视图中的 `location` 是事件声明的位置，`option_locations` 记录当前可用选项声明的位置。这里事件指向省份 `8`，第一个选项指向省份 `7`；不可用的第二个选项不出现在视图中，第三个选项没有定位。字段值是世界对象的 `EntityId`，本例分别为 `2` 和 `1`，不是脚本里的省份编号。

读取位置不会执行选项、推进日期或消耗随机数。执行第一个选项后，国家的 `treasury` 增加 `1000`。定位属于不可变内容；修改定位声明后，旧存档会拒绝加载。不含定位声明的已有内容保留原有内容标识和输出格式。

EU4 入口目前接受事件及选项上的正整数省份编号，包括对前导零的规范化。省份必须已登记且名称唯一。命名事件目标、`ROOT`、`FROM`、阶层名称等定位形式仍会报告带源文件位置的错误。原生入口允许引用自制省份名称。当前事件演示提供定位数据查询；Godot 世界界面的事件弹窗和地图定位操作尚未接入。
