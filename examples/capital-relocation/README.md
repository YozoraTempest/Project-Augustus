SPDX-License-Identifier: GPL-3.0-only

Copyright (C) 2026 Project Augustus contributors

# 原生首都重设示例

本目录是自制 native JSON 场景。AAA 持有 Old 和 New，初始首都是 Old。选择事件选项后，计划把当前首都设为 New，并立即通过 `Target::Capital` 修改 New 的 `probe`；一天后，已保存的计划按命名省份引用把首都设回 Old，再修改 Old 的 `probe`。

CLI 查询中 Old 的 `probe` 从 11 变为 110，New 的 `probe` 从 22 变为 220；事件日志记录两个选项阶段和延迟返回阶段。

```sh
cargo run -p augustus-run --bin augustus-events -- --content examples/capital-relocation --input native
cargo run -p augustus-run --bin augustus-events -- --content examples/capital-relocation --input native --trigger capital.relocate --choose 0:0 --save capital-relocated.json
cargo run -p augustus-run --bin augustus-events -- --content examples/capital-relocation --input native --load capital-relocated.json --days 1 --save capital-returned.json
```

`scene.json` 和 `events.json` 使用本 README 声明的 GPL-3.0-only 许可与版权信息；JSON 文件不添加非标准注释。

此示例演示 core 原生计划对 `currentcapital` 关系的重设。它不代表原版付费迁都行为、首都历史记录修改或完整的 EU4 `set_capital` 效果支持。
