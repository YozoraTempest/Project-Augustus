# 任务领取样本

本自制 Mod 可叠加在 `examples/world` 上。AAA 的第一项任务要求拥有省份 1，奖励 5 国库并设置一个国家变量，同时弹出一项报告事件。第二项任务以前置任务完成和该变量的实时值为条件，奖励 7 国库。另有一项目标条件尚未确定的任务，以及一项奖励目前无法执行的任务，两者均不能领取。BBB 会显示另一组任务。

`external_state.json` 明确声明了样本的初始国库、变量、任务完成记录和省份归属。从仓库根目录的 Git Bash 启动 Godot 4.7.2：

```sh
world_root="$(pwd)/examples/world"
mission_mod="$(pwd)/examples/mission-claims"
external_state="$(pwd)/examples/mission-claims/external_state.json"
godot --path "$(pwd)/godot" -- --game-root "$world_root" --date 1444.11.11 --player AAA --player BBB --mod "$mission_mod" --external-state "$external_state"
```

在国家选择器中选择 AAA 查看任务，再选择 BBB 查看另一组任务。Godot 集成测试使用进程号独立的 `user://` 文件：

```sh
cargo build -p augustus-godot --locked
godot --headless --path godot --script res://tests/mission_claims.gd
```
