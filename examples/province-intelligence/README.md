# 省份情报界面样本

本目录为自制测试 Mod，配合 `examples/world` 使用。一日后事件把省份 1 的贸易中心设为 3 级并让 BBB 发现该省份；选择事件选项后将其设为 1 级，并让 AAA 发现省份 3。样本用于验证省份面板在推进时间、切换国家、选择事件和恢复快照时读取实时状态。

运行 `godot --headless --path godot --script res://tests/province_intelligence.gd` 可执行完整界面回归。
