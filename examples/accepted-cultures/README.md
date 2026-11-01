<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 Project Augustus contributors -->

# 运行时文化接纳

本目录是叠加在 `examples/world` 上的自制 Mod。月度脉冲只向 BBB 提供开始事件；选择选项后，国家接纳字面文化 `alpha`，并在次日收到后续事件。后续选项只在 BBB 接纳 `alpha` 时可用，选择后移除这项显式接纳。

使用 Godot 4.7.2，从仓库根目录的 Git Bash 启动：

```sh
world_root="$(pwd)/examples/world"
culture_mod="$(pwd)/examples/accepted-cultures"
godot --path "$(pwd)/godot" -- --game-root "$world_root" --date 1444.11.11 --player BBB --mod "$culture_mod"
```

选择 BBB 和省份 1，按“至下月”进入 1444.12.01，再选择事件选项。查看省份税收基数中的文化来源，保存项目快照，然后推进一天并选择后续选项。读取快照可恢复到已接纳文化、后续事件尚未执行的状态。

EU4 兼容层与 Godot 集成检查使用同一基础场景和 Mod：

```sh
cargo test -p augustus-eu4 --test accepted_culture_world --locked
cargo build -p augustus-godot --locked
godot --headless --path godot --script res://tests/accepted_cultures.gd
```

省份 1 属于 BBB，文化为 `alpha`；BBB 的主文化为同组的 `beta`。接纳前，税收基数包含 `same_culture_group` 的 −15% 修正，已知小计为 0.212。接纳后该项惩罚消失，小计为 0.531；移除接纳后恢复。其他税收来源仍有未决项，因此这些小计不代表完整收入。

效果只增删已知显式名单中的文化，不改变主文化、文化联盟的组接纳或其他国家的政策。移除显式接纳后，组政策仍可能使该文化继续被接纳。未知名单不会被当作空名单；文化必须已注册。当前支持字面文化标识，不支持国家、省份或动态文化引用，也不执行玩家付费接纳的费用、数量限制或额外回调。

原生计划可使用 `SetAffiliationAcceptance`，指定 `target`、`dimension`、`member` 和 `accepted`。单项修改复用共享接纳政策，后续条件与属性立即读取新状态，延迟计划和状态使用现有项目快照格式保存。

本目录全部文件均为自制，采用 GPL-3.0-only；完整许可见仓库根目录 `LICENSE`。
