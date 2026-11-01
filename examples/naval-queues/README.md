<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 Project Augustus contributors -->

# 舰船队列检视样本

本目录的 EU4txt 输入完全自制，配合 `examples/world` 使用。AAA 在省份 2 的港口有两笔同型舰船订单，第二笔省略了等待项的零进度；BBB 在省份 1 的记录用于展示未接入诊断，因为该省份不是已知港口。当前国库已经包含存档保存时的付款结果，导入与完工不会再次扣费。

从仓库根目录查看显式接入的 FIFO：

```sh
cargo run --locked -p augustus-run --bin augustus-world -- --game-root examples/world --eu4-save examples/naval-queues/save.eu4 --import-naval-construction-queues --show-naval-constructions --military --country AAA
```

Godot 使用相同的输入：

```sh
godot --path godot -- --game-root "$(pwd)/examples/world" --eu4-save "$(pwd)/examples/naval-queues/save.eu4" --import-naval-construction-queues
```

也可以在页面选择这份存档，勾选“载入舰船队列”后载入。“造船”区显示当前排期和 FIFO 位置；选择订单后可查看其港口。推进后队首订单完成，舰船出现在“军力”区；等待项的当前排期会顺延。保存项目快照再读取可恢复队列进度，不需要重新提交请求。

选择一笔订单后点击“取消此订单（不退款）”，确认框会显示订单详情并暂停时间推进。选择“保留订单”不改变队列；选择“取消订单”移除该订单，并刷新当前待造数量和 FIFO 位置。关闭确认框后仍保持暂停，需要手动运行。取消不会退款或撤回已有分类账支出，也不改动其余订单当前保存的排期。取消后的状态可保存恢复，剩余订单继续建造。切换国家、重新载入世界或恢复快照会关闭旧确认框，避免继续执行先前的操作。

取消操作的集成检查及图形截图：

```sh
godot --headless --path godot --script res://tests/naval_cancellation.gd
godot --path godot --script res://tests/naval_cancellation.gd -- --capture /absolute/path/naval-cancellation
```

默认关闭队列导入时，多订单港口保持未接入并报告原因。面板的输入诊断是载入基线时的全局观察，独立于当前国家与待造数量；恢复项目快照不会重计该观察。样本不包含自动报价、暂停、取消退款、移动或战斗规则。
