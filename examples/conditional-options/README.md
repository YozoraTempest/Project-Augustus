# 条件选项演示

`scene.json` 明确指定 AAA 由玩家控制，BBB 由 AI 控制。两种内容入口使用同一核心，原生定义在 `events.json` 和 `option_conditions.json`，EU4 写法在 `eu4/events/demo.txt`。

先触发 AAA 的事件并保存：

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/conditional-options --trigger choices.start --save choices_pending.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/conditional-options --input native --load choices_pending.json --choose 0:1 --save choices_waiting.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/conditional-options --load choices_waiting.json --days 1 --choose 1:0
```

首次查询只有选项 `1`。`immediate` 已清除 gate 标记，因此选项 `0` 不可显示；选项 `2` 要求 AI 身份，选项 `3` 的条件恒假。选择 `1` 后，日志为 `start`、`human`、`after`，第二天的事件提供选项 `0` 和 `2`。完成 `1:0` 后追加 `open`。

换成 BBB 可检查另一种身份：

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/conditional-options --trigger choices.start --entity 1
```

这次只有选项 `2`。这里的 AI 身份用于条件求值，演示没有自动选择选项。

EU4 选项的 `trigger` 翻译成核心条件，不作为效果执行。选项标识保留原始声明次序，不因筛选重新编号。`World::available_event_options` 每次读取当前状态，不执行效果、不消耗随机数；条件全部为假时返回空列表，待选事件仍保留。未声明选项条件时，该选项始终可用。

`World::choose_event` 执行已经接受的选项命令，不重新检查显示条件。界面应在打开窗口时查询并保留所显示的选项；这个命令入口也允许工具明确指定一个有效的原始选项标识。无效事件或选项标识仍在写入前报错。

原生条件文件按“事件标识 → 选项标识 → 条件”组织，例如：

```json
{
  "choices.start": {
    "1": {
      "All": [
        {"CountryControlled": {"target": "Current", "controller": "human"}}
      ]
    }
  }
}
```

`scene.json` 的 `country_controllers` 按实体编号记录 `human` 或 `ai`。控制身份随存档保存，可用 `World::set_country_controller` 更改；它不修改国家本身或待选事件标识。未指定时身份未知，读取身份条件会报错，取反也不能把未知变成确定结果。原版世界命令行可以通过 `--player 国家标签` 或 `--ai-only` 在启动效果执行前提供完整控制名单，操作见 [世界加载说明](../../docs/WORLD_INITIALIZATION.md)。
