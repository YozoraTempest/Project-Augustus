# 条件事件图片

这是自制场景，不需要 EU4 安装。演示输出图片资源键，尚不渲染图片。

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/event-pictures --trigger pictures.start --save picture_waiting.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/event-pictures --input native --load picture_waiting.json --days 1 --save picture_next_day.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/event-pictures --load picture_next_day.json --trigger pictures.later
```

`pictures.start` 的即时效果设置标记，因此选中 `council_ready`。次日标记被清除，已有待选事件仍保留这张图片；新触发的 `pictures.later` 选中 `council_waiting`。存档恢复不会重新选择已有事件的图片。

EU4 声明按原有顺序编译，选择第一项满足条件的图片；标量图片始终满足条件。没有条件满足时，使用第一项声明的图片。`pictures.fallback` 和 `pictures.mixed` 分别演示这两种情况。

原生定义使用可选的 `pictures.json`：每个事件提供有序的 `variants`，每项包含资源 `key` 和核心 `condition`；`fallback` 指定无匹配时的资源键，省略时不显示图片。图片定义参与内容标识，修改后旧存档会拒绝加载。
