# 脚本宏与事件演示

这是自制场景。国家 AAA 的启动候选通过嵌套条件宏筛选，即时效果展开嵌套效果宏；选择选项后设置国家标记并安排次日事件，收尾和次日查询读取变化后的状态。BBB 不符合候选条件。

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/scripted-events --input eu4 --candidate scripts.start --save scripts_first.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/scripted-events --input native --load scripts_first.json --choose 0:0 --save scripts_waiting.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/scripted-events --input eu4 --load scripts_waiting.json --days 1 --choose 1:0
```

输出依次记录 first、upper、lower、after、later、end。原生定义直接提供宏展开后的核心计划，两种输入可以交叉读档。

宏文件按原文件名的字节顺序读取，同名定义保留先读到的一项。本例的 Z 文件先于 a 文件，其中重复的 begin、ready 定义不会覆盖第一项；Word 和 word 分别调用自己的定义。加载视图只读取类别目录的直接 `.txt` 子文件。

效果调用当前支持 `名称 = yes`，条件调用支持 `名称 = yes/no`。库中暂不使用的模板可以登记；调用到未支持的列表、运算符、参数或作用域时会按定义位置报错，常量不执行分支也会检查。循环调用和超过调用深度的内容会被拒绝。原版启动入口使用相同编译器，并继续绑定宏中引用的事件；改变已展开的计划会改变有效内容标识。

当前仍未支持参数替换、动态脚本名、条件选项和完整事件上下文。没有独立的宏运行时，效果和查询由已有核心执行。
