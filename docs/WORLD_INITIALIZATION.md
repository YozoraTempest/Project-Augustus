# 原版开局查看

当前可以从本地 EU4 1.37.5 安装目录载入地图、国家和省份的部分开局状态。兼容模块处理文件与历史日期，将受支持的数据交给 Rust 核心；命令行和 Godot 从同一世界查询结果。

项目以终版及全部 DLC 机制为开发基线，默认玩家具备全部 DLC，不根据启动器开关选择机制。这个基线说明目标规则，不代表当前已经实现全部玩法。开局查看尚不能推进原版世界、运行战役或加载 MT，也未支持原版存档导入。

## 运行

安装 Rust 工具链，使用自己的游戏安装目录：

```sh
cargo run -p augustus-run --bin augustus-world --locked -- --game-root "/path/to/Europa Universalis IV" --date 1444.11.11 --country HAB --province 134
```

输出 JSON 包含载入报告，以及可选的国家、省份查询和对应文件来源。省份基础税收、生产、人力用千分位整数表示，例如 `6000` 表示 `6`。未知国家或省份、错误日期、无效引用和不匹配的地图尺寸会报错。加载器要求安装目录版本为 1.37.5；仅修改版本声明不能使其他版本兼容。

Godot 界面使用 Godot 4.7.2：

```sh
cargo build -p augustus-godot --locked
godot --path godot
```

选择安装目录和开局日期，点击“载入开局”。地图显示当前已载入的领土颜色；国家下拉框可查看领土、首都、文化、宗教和政体，点击地图可查看省份。也可以在启动时提供目录：

```sh
godot --path godot -- --game-root "/path/to/Europa Universalis IV" --date 1444.11.11
```

未载入的值显示为“未载入”。界面列出未应用历史字段的数量和存在历史文件歧义的国家数；这些内容仍需实现或验证。重新载入失败时保留原来的世界。

## 当前范围

载入固定地图的省份定义和位图，区分陆地、海域、湖泊、荒地与固定地图未使用的定义。国家定义提供标识和颜色；国家历史处理首都、主文化、宗教、政体和科技组。省份历史处理所有者、控制者、核心、文化、宗教、商品、基础税收、生产、人力、城市状态和帝国归属。

先处理无日期的基础字段，再按时间顺序处理不晚于所选日期的条目；同日条目保持文件中的顺序。历史派生字段缺失时保留未知状态。同一国家有多个历史文件时，报告所有候选文件，暂不选择其中一个；首都必须属于该国才能绑定核心关系。

未支持的历史字段记录次数和一个实际文件位置，不执行其效果。因此，当前显示的国家数量和领土信息不能作为完整原版开局已经一致的证明。尚未接入君主、外交、军队、经济、事件和各项 DLC 专属系统。当前入口只读取原版目录，Mod 组合仍使用独立的[文件检查工具](EU4_CONTENT.md)。

位图、脚本和生成报告均来自本地安装，不随项目分发。需要完整资源的 DLC 档案读取尚未接入；当前地图与所列开局字段从安装目录的普通文件读取。

## 验证

仓库的 `examples/world` 是自制小世界，不需要 EU4 安装。可以将它作为 `--game-root` 使用。测试覆盖日期排序、未来条目、数值精度、国家关系、来源追踪、歧义文件和错误输入，并检查兼容输入与原生输入得到相同的核心状态。

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build -p augustus-godot --locked
godot --headless --path godot --editor --import --quit
godot --headless --path godot --script res://tests/world.gd
godot --path godot --script res://tests/world.gd
godot --headless --path godot --script res://tests/session.gd
```

最后两项界面检查分别验证真实渲染中的国家颜色，以及原有建造演示。Windows x86_64 已验证，其他平台尚未验证。实际游戏安装的检查可运行同一界面测试并提供 `--game-root`；使用 `--country`、`--province` 选择查看对象，`--capture` 后提供 PNG 路径可保存画面。报告和截图只保存在本地，不提交原版资源或其生成数据。
