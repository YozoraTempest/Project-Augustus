# 任务选择

这是一个自制国家场景，演示任务分支的选择和切换。`scene.json` 声明核心任务组；两种事件输入分别使用 EU4 脚本和原生执行计划，运行结果相同。

条件变化不会立即改变已选任务。第一条事件先记录任务完成、改变分支条件，再显式刷新任务；次日事件切回原分支，完成记录仍然保留。另一个国家拥有自己的任务状态。

```sh
cargo run --locked -p augustus-run --bin augustus-events -- \
  --content examples/objective-selection --input eu4 \
  --trigger selection.first --choose 0:0 --days 1 --save selection.json

cargo run --locked -p augustus-run --bin augustus-events -- \
  --content examples/objective-selection --input native \
  --load selection.json --choose 1:0
```

原生任务组通过 `condition`、`objectives`、`priority` 和 `claims` 定义选择规则。较高优先级的已选组先占用 `claims`；同一优先级的组可以并存。`objectives.entities` 缺少国家条目时表示可用性未知，空列表表示已知没有可用任务。刷新不检查领取条件，也不执行奖励。
