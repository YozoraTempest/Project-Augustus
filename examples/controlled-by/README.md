SPDX-License-Identifier: GPL-3.0-only

Copyright (C) 2026 Project Augustus contributors

# 当前省份控制者条件

这是叠加在 `examples/world` 上的自制 Mod。AAA 拥有省份 1，但由 BBB 控制；BBB 拥有省份 2，但由 AAA 控制。

两国都显示四项任务。第一项要求省份 1 当前由查询国家控制，使用 `controlled_by = ROOT`：AAA 不能领取，BBB 可以领取并获得 3 国库。该任务的“目标”按钮显示查询国家控制的省份，因此 AAA 高亮省份 2，BBB 高亮省份 1。第二项固定要求省份 1 由 BBB 控制，两国都可领取，奖励 1 国库。第三项使用 `controlled_by = owner`，比较当前控制者与该省的当前所有者。

第四项对省份 4 使用 `NOT = { controlled_by = ROOT }`。它的控制者未知，条件和地图目标保持未决，不能领取。外部状态包的 `null` 将该关系留为未知；核心运行时显式记录的无控制者则是已知不匹配，否定后为真。

从仓库根目录的 Git Bash 运行：

```sh
world_root="$(pwd)/examples/world"
controller_mod="$(pwd)/examples/controlled-by"
controller_state="$controller_mod/external_state.json"
godot --path "$(pwd)/godot" -- --game-root "$world_root" --date 1444.11.11 --player AAA --player BBB --mod "$controller_mod" --external-state "$controller_state"
```

展开国家面板的“任务”，切换 AAA 和 BBB 查看条件、地图目标和领取结果。原生 `SetProvinceController` 动作修改当前控制者后，条件会立即读取新关系；Rust 集成测试覆盖这种变更及保存恢复。这个条件只读取控制者，不执行所有权转移或占领副作用。

当前支持省份作用域中的字面国家标签、国家类型的 `ROOT`、`emperor`、`event_target` 和 `owner`。`event_target` 必须由本次有效任务奖励或事件效果（含展开后的 scripted effects）声明为国家作用域；未声明、仅在省份作用域声明或类型冲突的目标会在编译时拒绝。符合类型的目标按运行时绑定读取，未绑定时不匹配。叛军控制者及需要前一作用域的 `FROM` 尚未接入；无效形式会报告脚本来源。国家作用域中的 `controlled_by` 也会被拒绝。

```sh
cargo test -p augustus-eu4 --test controlled_by --locked
godot --headless --path godot --script res://tests/controlled_by.gd
```

`external_state.json` 按本 README 声明的许可和版权分发，保持标准 JSON 格式。
