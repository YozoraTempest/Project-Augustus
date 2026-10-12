# 宣战理由记录演示

这是自制的三个国家和两个省份。AAA、BBB 各拥有一个省份，CCC 已登记但没有领土。兼容脚本与原生执行计划共用核心的有向记录、条件查询和存读档。

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/justifications --trigger grounds.start --choose 0:0 --save grounds_waiting.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/justifications --input native --load grounds_waiting.json --days 51 --choose 1:1 --save grounds_boundary.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/justifications --load grounds_boundary.json --days 1 --choose 2:0
```

初始事件向 BBB 授予三个月的 `restoration`，同时给 BBB 一条针对 AAA 的 `honour`。选项 `0` 将前者替换为一个月；选项 `1` 移除这两条记录。再次授予会替换期限，可以缩短，也可以延长。`grounds.dormant` 演示针对 CCC 的记录：授予后立即可查，次日资格检查将其清除。

兼容输入支持 `add_casus_belli`、`reverse_add_casus_belli`、`remove_casus_belli`、`reverse_remove_casus_belli`。目标可以是字面国家标签、`emperor` 或已声明为国家类型的 `event_target:name`。类型由 `common/cb_types` 注册；省略 `months`，或显式给出零、负数时，采用类型默认值。类型未声明期限时默认为十二个月；类型默认值可以为零。

兼容期限从实际授予日期直接增加月份，取目标月份中有效的日期。每日资格检查先于延期计划执行；月初到期清理在延期计划之后，清除到期日早于当前日的记录。因此，本例第 51 天的延期日志仍包含 `boundary_present`，该日结束后的选项查询只提供已到期选项 `1`。这些处理由核心执行，运行时不重新解释脚本。

原生内容使用 `GrantJustification`、`RemoveJustification`、`HasJustification`，通过 `EntityRef` 指定授予方与目标。`GrantTerm` 可选择永久、固定天数或日历月份；固定天数在到期当天的延期计划前清除，月历期限可选择每日或月初清理。定义的 `issuer`、`target` 条件决定已有记录是否继续保留；`null` 表示规则未知，`Always` 为真表示明确无额外限制。

兼容的 `has_casus_belli` 目前只查询显式授予类型；`has_casus_belli_against` 要求注册类型全部属于显式授予类型，避免把尚未实现的自动理由判成不存在。未编译的保留条件在每日检查后产生未知状态。自动生成、宣战权限、附庸限制、战争目标、战争和和约尚未接入；已有记录不能证明允许宣战。动态 ROOT／FROM／PREV 目标仍未支持。
