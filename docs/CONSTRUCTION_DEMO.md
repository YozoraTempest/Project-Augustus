# 建造演示

演示使用仓库中的自制内容，不需要 EU4 安装。它验证脚本编译、核心结算、系统替换和界面之间的完整调用过程，还不能运行 EU4 或 MEIOU & Taxes。

## 命令行运行

安装 Rust 工具链，在仓库根目录执行：

```sh
cargo run -p augustus-run --locked -- --input eu4 --system capacity --build --days 100
```

程序在两个省份分别建造市场和工坊，然后输出第 100 天的状态。初始国库为 1000，扣除 350 的费用，完工回调奖励 17.5，最终为 667.5。`--input native` 选择原生 JSON 回调；`--system fixed` 选择固定工期。

读取磁盘上的自制场景，并在建造中途保存：

```sh
cargo run -p augustus-run --locked -- --content examples/construction --system capacity --build --days 14 --save construction-save.json
cargo run -p augustus-run --locked -- --content examples/construction --system capacity --load construction-save.json --days 86
```

命令行拒绝覆盖已有存档。恢复时须选择同一内容和建造系统；内容、编译结果或系统变化会报错。这里使用项目自己的 JSON 存档格式。

## Godot 界面

使用 Godot 4.7.2 和 Rust 工具链。先在仓库根目录构建扩展，再启动建造演示：

```sh
cargo build -p augustus-godot --locked
godot --path godot res://main.tscn
```

直接运行项目会进入原版开局查看，也可点击其中的“建造演示”。演示界面初始暂停。选中省份和建筑，点击“开始建造”；可以逐日推进、连续运行或保存、恢复。选择回调格式和建造系统后，点击“重新开始”生效。界面存档写入 Godot 的用户数据目录，不修改游戏安装。

固定工期使用自制的 30 天和 70 天。按建设能力推进时，每天累加省份当前能力；“调整施工安排”会随机改变能力，用于观察后续建造和存档是否一致。这些数值与规则属于演示，不代表 EU4 原版建造机制。

Windows x86_64 已验证。扩展描述中也列出 Linux 与 macOS 的库路径，这两个平台尚未验证。

## 支持范围

自制 EU4 写法回调位于 `examples/construction/eu4`。编译器支持 `log`、`add_treasury`、`set_variable`、`change_variable`、`owner`、`capital_scope`、`if` / `limit` / `else`，以及无参数的 scripted effect 与 scripted trigger。条件支持 `always`、`NOT`、`check_variable` 和 `treasury`；变量必须事先登记，数值最多三位小数，内部用千分位整数。

`NOT` 当前只接受一个直接条件，复杂的组合条件尚未接入。演示使用的数值表示、相对日期和建造规则，不构成对原版所有运算、历法或机制的兼容承诺。

编译时绑定名称与字段，展开受支持的脚本调用，并折叠完全确定的条件。动态条件仍在执行时读取状态，效果按原顺序立即写入。未知操作、错误作用域、递归脚本和超出精度的数值都会报错。

原生 JSON 回调目前支持所有者国库、变量增量和日志。两类输入都生成核心执行计划。`ConstructionSystem` 选择整个建造实现，包含开始操作和后续结算；这是演示接口，尚未发布为稳定 Mod SDK。

Rust 调用方可以实现该 trait，通过 `Demo::embedded_with_system` 或 `Demo::from_directory_with_system` 提供自己的系统，不需要修改演示库。系统的 `id()` 应包含版本，操作语义变化时须更新；存档也会核对规则和编译后的内容。外部系统接入的示例见 `crates/augustus-demo/tests/construction.rs`。

## 验证

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build -p augustus-godot --locked
godot --headless --path godot --editor --import --quit
godot --headless --path godot --script res://tests/session.gd
```

Godot 检查会加载真实 Rust 扩展，验证按钮操作、查询、两类输入、存档和随机状态。测试内容均为自制样本。

可以独立测量常量条件折叠的执行成本：

```sh
cargo run -p augustus-run --bin augustus-bench --release --locked -- --blocks 512 --iterations 2000 --samples 7
```

基准比较已经绑定名称的两份计划，一份保留常量分支，一份折叠分支；每组交替运行并检查完整世界状态相等。输出包括各次耗时、中位数、单次编译耗时和计划自身持有的堆容量。堆容量不包含分配器开销、来源信息或世界状态；这是自制脚本的局部基准，不能据此推算 EU4 或大型 Mod 的整体性能。
