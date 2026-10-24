# EU4 文件工具

`eu4-inspect` 只读取本地文件，不修改游戏目录，也不运行游戏。以下命令在仓库根目录执行。

## 检查单个目录

```sh
cargo run -p eu4-inspect -- catalog "<内容目录>"
cargo run -p eu4-inspect -- descriptor "<描述文件路径>"
cargo run -p eu4-inspect -- parse "<内容目录>" "common/buildings/example.txt"
```

`catalog --list` 列出文件；`descriptor` 显示 Mod 元数据，但不挂载声明的 `path` 或 `archive`；`parse` 检查语法并显示前二十个顶层字段。

可以单独求值有效的 Lua 定义文件，输出类型、最终值、执行文件顺序和来源：

```sh
cargo run -p eu4-inspect -- defines "<原版目录>" --mod "<Mod 目录>"
cargo run -p eu4-inspect -- defines "<原版目录>" --difficulty hard
```

工具先执行基础文件，再按有效文件清单顺序执行普通覆盖，最后执行选中的难度文件；只处理 `common/defines` 的直接子文件。难度选项为 `very-easy`、`easy`、`normal`、`hard`、`very-hard`，默认正常。该选项用于定义检查，不表示世界入口已经实现全部难度机制。文件挂载复用下文的来源规则。

求值使用 Lua 5.1.5，提供基础运算、字符串、表和数学函数；不提供文件、系统、模块加载、随机数、协程和错误捕获函数。仅接受文本脚本，整批限制为 1000 万条 Lua 指令、64 MiB Lua 内存、每文件 8 MiB、最多五万项定义。超过限制或语法、执行错误会终止本次加载并报告来源。数值输出是 Lua 求值结果，尚未使用的定义不会因此获得对应游戏机制的实现。

## 查询覆盖与来源

```sh
cargo run -p eu4-inspect -- resolve "<原版目录>" --mod "<Mod 目录>" --mod "<子 Mod 目录>"
cargo run -p eu4-inspect -- resolve "<原版目录>" --mod "<Mod 目录>" --file "common/buildings/example.txt" --parse
```

Mod 目录须包含 `descriptor.mod`。加载器根据描述文件依赖排列候选；依赖 Mod 位于依赖它的 Mod 后面，同优先级按描述名称的字节顺序排列。文件查找从输出列表的首项开始，首个匹配文件生效。

可以传入启动器配置，按其中 `enabled_mods` 的名称筛选已提供的 Mod 目录：

```sh
cargo run -p eu4-inspect -- resolve "<原版目录>" --mod "<Mod 目录>" --launcher-config "<EU4 用户目录>/dlc_load.json"
```

配置中启用的每个 Mod 都须通过 `--mod` 提供目录；工具不会搜索 Steam Workshop 或启动器注册表。筛选后仍按描述依赖决定搜索优先级。此项目固定使用全部本地 DLC，`disabled_dlcs` 不改变 DLC 加载。

高优先级来源覆盖同路径文件，`replace_path` 排除声明来源之后的低优先级文件。报告保留覆盖与排除原因；被排除且没有替代文件的路径仍可查询。`--list` 列出全部追踪路径，`--parse` 只解析有效文件。

文件路径按 ASCII 大小写不敏感匹配，保留物理文件的原有拼写。查询接受两种路径分隔符；描述中的 `replace_path` 会先将 `\` 转为 `/`，再校验相对路径。绝对路径、上级跳转、来源内的大小写歧义和循环依赖会报错；缺失的依赖不会增加优先级。

## 查询定义

```sh
cargo run -p eu4-inspect -- definitions "<原版目录>" buildings --mod "<Mod 目录>" --name "example_building"
cargo run -p eu4-inspect -- definitions "<原版目录>" scripted-effects --mod "<Mod 目录>" --duplicates
```

类别支持 `buildings`、`scripted-effects` 和 `scripted-triggers`。索引有效 `.txt` 文件中的顶层赋值块，跳过 `@` 常量，报告声明的来源、文件和行列。定义名保留大小写；同名声明全部列出，当前不判定其生效优先级。`--list` 列出所有定义。所有接受来源的命令都可使用 `--launcher-config`。

## 查看脚本加载列表

```sh
cargo run -p eu4-inspect -- script-files "<原版目录>" scripted-effects --mod "<Mod 目录>"
```

这个列表用于脚本编译，只选择类别目录直接包含的 `.txt` 文件，并按物理文件名的原有字节顺序排列。支持 `scripted-effects` 和 `scripted-triggers`。同名脚本定义保留先读到的一项，标识符区分大小写。定义索引仍保留全部声明，两者用途不同。

当前编译器只支持建造演示使用的一小部分效果、触发器与作用域，未知操作会报告来源和行列，具体范围见 [建造演示](CONSTRUCTION_DEMO.md)。建筑定义可以索引和解析；建筑效果自动接入游戏规则尚未实现。

## 编码与限制

描述文件默认 UTF-8，普通文本默认 Windows-1252。分别用 `--descriptor-encoding` 和 `--encoding` 显式选择 `utf8` 或 `windows1252`；单独的 `descriptor` 命令使用 `--encoding`。错误中的列号按字节计算。

单独的 `catalog`、`parse` 和 `descriptor` 命令针对目录；`resolve`、`definitions` 与 `script-files` 支持本地 DLC ZIP 和目录 Mod，不支持压缩 Mod 或 EU4 存档。文件改变后需要重新生成清单。成功解析与建立索引不代表已经兼容相应游戏机制。
