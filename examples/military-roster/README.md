<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 Project Augustus contributors -->

# 实时军力检视样本

本样本把自制的陆军、编队、团和舰船状态叠加到 `examples/world`。报告反映当前 core 中已经载入的编队及其成员；它不保证包含国家的全部军力，也不计算战斗或移动。未提供的成员状态保持未知。报告中的编队和成员 ID 是 Augustus core 稳定标识，不是 EU4 存档原始 ID。

从仓库根目录查看 AAA 在省份 1 的当前编队：

```sh
cargo run --locked -p augustus-run --bin augustus-world -- --game-root examples/world --external-state examples/military-roster/external_state.json --military --country AAA --province 1
```

Godot 使用相同的世界与状态包：

```sh
world_root="$(pwd)/examples/world"
external_state="$(pwd)/examples/military-roster/external_state.json"
godot --path "$(pwd)/godot" -- --game-root "$world_root" --date 1444.11.11 --player AAA --player BBB --external-state "$external_state"
```
