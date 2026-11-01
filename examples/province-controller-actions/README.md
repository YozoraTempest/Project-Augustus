SPDX-License-Identifier: GPL-3.0-only

Copyright (C) 2026 Project Augustus contributors

# 原生省份控制者示例

这是自制 native JSON 场景：AAA 与 BBB 是国家，Harbor 是 AAA 的首都省份。触发事件时，`SetProvinceController` 把当前控制者设为 AAA。选项 0 改为 BBB，并安排一天后恢复为 AAA；选项 1 将控制者记录为已知不存在。

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/province-controller-actions --input native --trigger province_controller_actions.change --save controller_pending.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/province-controller-actions --input native --load controller_pending.json --choose 0:0 --save controller_bbb.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/province-controller-actions --input native --load controller_bbb.json --days 1 --save controller_aaa.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/province-controller-actions --input native --load controller_pending.json --choose 0:1 --save controller_absent.json
```

CLI 事件视图记录事件日志和待执行任务，不直接列出省份关系。Rust 调用者可查询 core 状态：

```rust
let province = EntityId(2);
let controller = world.province_controller_state(province)?;
let previous_controller = world.province_previous_controller_state(province)?;
```

当前控制者返回的三态分别是未知、已知无控制者和已知国家；`previous_controller` 是独立历史关系。本示例只改当前控制者，不推断或改写 owner 与 previous-controller。

该原语不执行完整 EU4 占领副作用，也不接入 EU4 `change_controller` 编译；兼容层中按开局绑定的占领属性不会因此自动重算。包含这个新动作的延迟任务需要由支持该动作的程序版本读取。

回归测试检查保存快照中的 `province_controllers`、事件日志、延迟计划和连续/恢复执行结果。它同时确认 owner 保持为 AAA，previous-controller 关系保持未知。

`scene.json` 和 `events.json` 是 JSON 数据文件，按本 README 声明的 GPL-3.0-only 许可和版权信息分发；文件本身保持标准 JSON 格式。
