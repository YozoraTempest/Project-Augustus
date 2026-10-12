# 作用域与地理条件演示

这个自制场景有两个国家、两个首都和一个海洋省份，不需要 EU4 安装。EU4 条件先翻译成核心查询；原生 JSON 使用同一套条件和世界状态。

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/scoped-conditions --trigger scopes.start --save scopes_first.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/scoped-conditions --input native --load scopes_first.json --choose 0:0 --trigger scopes.rebind --entity 1 --save scopes_waiting.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/scoped-conditions --load scopes_waiting.json --days 1 --choose 1:0 --choose 2:0
```

首个事件只有选项 `0`：AAA 的首都满足地区、区域、大区和大陆条件，所有者也为 AAA。另一个国家将命名目标 `place` 改绑到自己的首都；次日查询读取新的绑定，同时 AAA 的首都关系保持独立。日志依次为 `start`、`accepted`、`changed`、`done`。

海洋省份明确没有所有者，可以单独运行：

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/scoped-conditions --trigger scopes.sea --entity 4 --choose 0:0
```

日志为 `unowned`、`sea`。缺失对象的作用域条件返回假，即使其内部是空条件块；`NOT` 可以取反这个已确定的结果。未确定的关系和地理数据会报错，不能被取反成真。

`scene.json` 的 `administration` 声明地理成员关系，`known_absent_targets` 按实体编号明确记录不存在的 `Owner` 或 `Capital`。核心通过 `World::mark_target_absent` 登记这类状态；绑定实际所有者或首都时自动清除对应声明。缺少声明且没有关系表示未知。

原生条件 `With` 进入所有者或首都，`InScope` 进入职位或全局命名目标，`GeographicMember` 查询省份的成员关系。兼容写法支持 `owner`、`capital_scope`、`emperor`、已声明类型的 `event_target:name`，以及省份作用域的字面 `area`、`region`、`superregion`、`continent`。当前尚未支持数字省份作用域、动态地理标识和完整 `ROOT`／`FROM`／`PREV` 上下文。
