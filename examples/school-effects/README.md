# 学派变更演示

本目录是自制场景，不需要 EU4 安装。兼容脚本和原生定义使用相同的归属状态、属性计算器、事件和存档。

AAA 的国教属于 `family`，初始明确没有学派。触发后选择 `first`，选项切换到 `second`，次日事件重新选择 `first`。场景的 `yield` 基础值为 1，两个学派分别提供 +25% 和 −50% 的比例修正；查询结果依次为 1、1.25、0.5、1.25。查询中的数值以百万分位表示。

```sh
cargo run -p augustus-run --bin augustus-events -- --content examples/school-effects --input eu4 --trigger choice.start --save school-first.json
cargo run -p augustus-run --bin augustus-events -- --content examples/school-effects --input native --load school-first.json --choose 0:0 --save school-waiting.json
cargo run -p augustus-run --bin augustus-events -- --content examples/school-effects --input eu4 --load school-waiting.json --days 1 --choose 1:0
```

BBB 属于其他宗教组。下面的事件尝试选择 `first`，归属仍保持明确无学派，`yield` 为 1：

```sh
cargo run -p augustus-run --bin augustus-events -- --content examples/school-effects --input eu4 --trigger choice.foreign --entity 1 --choose 0:0
```

兼容输入的 `set_religious_school` 要求一个字面 `group` 和一个字面 `school`。编译器检查学派定义，再把宗教组检查和归属修改翻译成核心计划。`else_if` 链只执行首个成立的分支，分支中的学派变化不会触发后续分支重判。

原生 `SetAffiliation` 指令指定 `target`、`dimension` 和 `member`；成员必须注册，`member: null` 表示明确无归属。它不自动限制宗教组，本例在共享计划中显式声明相同条件。场景的可选 `attributes` 声明计算内容，实体查询只在有公式时输出属性。未知归属与明确无归属分别处理，派生结果在读取时计算，不随存档保存缓存。

本示例只验证归属、分支和属性传递，不包含宗教转换、学者邀请或完整宗教系统。
