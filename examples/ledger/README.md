<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 Project Augustus contributors -->

# 当前国家分类账样本

本目录的状态包完全自制，配合 `examples/world` 使用。它为 AAA 提供初始国库 10.000，以及月税收 1.000、普通生产 0.500、贸易收入 0.500 和要塞维护支出 0.250。首次月结后，国库为 11.750。

CLI 已有的国家查询会输出当前 `country.ledger`，其中金额使用千分位整数：

```sh
cargo run --locked -p augustus-run --bin augustus-world -- --game-root examples/world --external-state examples/ledger/external_state.json --country AAA --days 20
```

Godot 使用相同输入：

```sh
godot --path godot -- --game-root "$(pwd)/examples/world" --external-state "$(pwd)/examples/ledger/external_state.json" --player AAA --player BBB
```

选择 AAA，展开“分类账”，然后按“至下月”。“累计分类记录”分别显示收入与支出分类；例如收入分类 2 是贸易，支出分类 2 是州维护，它们使用独立编号。尚未登记的分类显示“无分类记录”，明确登记的零金额显示 `0.000`。目前确认含义的分类显示中文名称，其余显示方向与编号，原始内部键可在提示中查看。

下拉框还可以查看待归集来源、归集快照、待结转累计、结转快照和辅助支出记录。当前标准月结追加分类账并清算待结余额，不自动执行分类账归集或年度结转；本样本的来源和累计向量有金额，但归集与结转快照仍为零。这些零值不证明已执行过归集或结转。向量尚未载入时则显示“未载入此向量”。累计分类记录没有月份或年度范围，辅助支出记录也不构成额外费用，不能把不同视图相加。

面板显示查询时的当前日期；已有分类账没有归集或结转时间戳，因此界面不推断快照日期。待结余额单独显示，不合并为国库。查询只读，不触发归集、结转或清算。推进、事件、保存恢复后会刷新当前账本，并保留仍有效的行选择；切换国家或载入新世界会清除行选择。

Godot 绑定 `query_country_ledger(country)` 将分类账金额转换为原始千分位整数的十进制字符串，避免 JSON 浮点解析丢失精度；显示时才转换为三位小数。核心与 CLI 的整数格式保持不变。
