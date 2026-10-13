# 根作用域与宗属关系

这个自制场景不需要安装 EU4。AAA 是 BBB 的宗主国；AAA 的事件进入 BBB 和首都时保留自己的根对象，调用新事件后，新事件以接收对象为根。

先触发事件并保存，再从原生输入恢复、完成三个选项：

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/root-scopes --trigger roots.start --save roots-opened.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/root-scopes --input native --load roots-opened.json --choose 0:0 --choose 1:0 --choose 2:0 --save roots-waiting.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/root-scopes --load roots-waiting.json --days 1 --choose 3:0
```

最终日志依次为 `parent_nested`、`child_root`、`parent_return`、`province_root`、`child_option`、`province_option`、`parent_option`、`later_root`、`later_option`。它们的实体编号依次为 `0, 1, 0, 3, 1, 3, 0, 1, 1`。

EU4 输入中的 `ROOT = { ... }` 编译为核心 `InScope`，选择器为 `Root`。普通嵌套和遍历保留根对象，国家事件和省份事件分别以自己的接收对象建立新根。事件选项与收尾效果沿用该事件的根对象。

`is_subject_of` 编译为核心 `SubjectOf`，查询当前直接宗属关系。支持字面国家、`ROOT`、皇帝及已声明类型的全局事件目标；不存在的全局目标和省份目标返回假。它不会把省份自动转换为所有者，也不会沿宗属链逐级查询。关系列表不完整、字面国家未注册或目标未声明时，报告错误。

原生程序可用 `ScopeContext` 分别提供当前对象与根对象；普通延迟计划保存两者的身份，执行时读取当前世界状态。事件调用则在进入新事件时建立新根。`FROM`、`PREV` 及政体转换回调中的 `ROOT` 暂未支持。
