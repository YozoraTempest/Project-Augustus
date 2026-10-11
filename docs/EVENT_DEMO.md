# 事件演示

这个自制场景不需要 EU4 安装。事件的即时效果、待选记录、选项效果和收尾效果由 Rust 核心执行；EU4 写法先编译成核心计划，原生 JSON 直接提供同一种计划。

触发议会事件并查看待选项：

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --trigger augustus_council.1
```

触发后等待五天，再选择第一个选项：

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --trigger augustus_council.1 --days 5 --choose 0:0
```

输出中，国库由 10000 变为 7500，首都的 `variable/phase` 变为 3000，待选列表清空。场景字段使用千分位整数。日志依次为 `council_ready`、`harbor_funded`、`council_closed`。`--choose` 接收“待选记录编号:选项标识”；本例两个选项的标识为 `0` 和 `1`。

在等待期间保存，然后从原生输入恢复并选择：

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --trigger augustus_council.1 --days 2 --save waiting.json
cargo run --locked -p augustus-run --bin augustus-events -- --input native --load waiting.json --days 3 --choose 0:0
```

保存目标必须不存在。两种输入具有相同的有效内容，因此可以互相恢复；修改场景或执行计划后，旧存档会拒绝加载。`--content examples/events` 从目录加载演示，默认使用内嵌文件。选项在日期推进后执行。

## 运行规则

事件先执行 `immediate`，成功后保存当前实体和触发日期。等待期间的数值及实体关系保持实时更新；选择时执行选项效果，再执行 `after`，成功后移除对应记录。多个同类事件各有独立编号。读档不重复即时效果，重复选择、未知选项和错误实体类型会在写入前被拒绝。效果运行错误保留已经发生的写入并停止模拟，不能继续推进或保存。

核心通过 `EventCatalog` 接收不可变定义，使用 `World::trigger_event` 和 `World::choose_event` 执行，使用 `World::restore_with_events` 重新绑定内容并恢复待选状态。事件定义共享存储，执行时直接使用编译后的计划。`World::events` 保持原有日志接口；待选记录通过 `World::pending_events` 查询。

## 当前支持范围

脚本输入支持显式触发的 `country_event`、`province_event`，要求 `id`、`is_triggered_only = yes` 和至少一个具备标量 `name` 的选项。标量 `title`、`desc`、`picture` 可读取，但演示不展示本地化文字。事件标识按输入字符串保留；命名空间的数字登记和原版选项命令格式未实现。

三个效果阶段复用已有编译器：`log`、绑定字段的国库及变量操作、`owner`、`capital_scope`、条件分支和无参数脚本调用。EU4 选项按声明顺序获得字符串标识 `0`、`1` 等；原生定义可以使用自己的选项标识。

自动候选、MTTH、AI 权重、一次性事件、过期处理、条件选项、事件内触发其他事件及完整 `ROOT`／`FROM`／`PREV` 上下文尚未支持。相关字段或不支持的效果会报告来源位置，整份输入编译失败。核心命令拒绝非法选择；原版某些异常选项编号会移除待选记录的行为不在本接口的支持范围内。此演示尚未接入原版战役的事件目录或 Godot 事件界面。
