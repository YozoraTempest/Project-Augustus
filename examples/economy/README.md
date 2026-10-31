<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 Project Augustus contributors -->

# 财政月结记录样本

本目录的状态包完全自制，配合 `examples/world` 使用。它为 AAA 提供已知国库、税收、普通生产和负现金流；BBB 则包含未知省份税收、缺失生产输入、未知国库和已知零现金流，用来区分部分金额、完整金额与确认入账。

从仓库根目录推进到首次月结后，查看 AAA：

```sh
cargo run --locked -p augustus-run --bin augustus-world -- --game-root examples/world --external-state examples/economy/external_state.json --country-finances --country AAA --days 20
```

起始日期为 1444.11.11，推进 20 天到 1444.12.01。AAA 的初始国库为 10.000；记录中的月税收为 1.000，普通生产为 0.500，现金流为 −0.250，当前国库为 11.250。将国家改为 BBB，可以查看已知月税收部分 0.500、未知完整收入和缺项原因；它的现金流金额是已知的 0.000，但由于余额未知，阶段确认入账仍未知。

Godot 使用同一个状态包：

```sh
godot --path godot -- --game-root "$(pwd)/examples/world" --external-state "$(pwd)/examples/economy/external_state.json" --player AAA --player BBB
```

载入后展开国家面板的“财政”区，先查看尚无该国月结记录的状态，再按“至下月”并选择税收、生产或现金流。阶段记录附带实际结算日期；继续推进数天，当前日期会改变，历史记录日期保持不变。保存项目快照并读取，可以恢复当时的余额和记录。

当前国库、账本待结余额和各阶段记录分别显示，不相加为一个推测的净收入。阶段次数属于该组件的全局计数；未安装的阶段、已安装但没有该国记录的阶段、未知金额和已知零值分别保留。报告只读，不执行月结或刷新输入，也不预测下月结果。CLI 的 `--economy` 继续查询建筑、商品与贸易网络目录；本样本使用独立的 `--country-finances`。
