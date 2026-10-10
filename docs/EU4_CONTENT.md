# EU4 文件工具

`eu4-inspect` 只读取本地文件，不修改游戏目录，也不运行游戏。以下命令在仓库根目录执行。

## 检查单个目录

```sh
cargo run -p eu4-inspect -- catalog "<内容目录>"
cargo run -p eu4-inspect -- descriptor "<描述文件路径>"
cargo run -p eu4-inspect -- parse "<内容目录>" "common/buildings/example.txt"
```

`catalog --list` 列出文件；`descriptor` 显示 Mod 元数据，但不挂载声明的 `path` 或 `archive`；`parse` 检查语法并显示前二十个顶层字段。

## 查询覆盖与来源

```sh
cargo run -p eu4-inspect -- resolve "<原版目录>" --mod "<Mod 目录>" --mod "<子 Mod 目录>"
cargo run -p eu4-inspect -- resolve "<原版目录>" --mod "<Mod 目录>" --file "common/buildings/example.txt" --parse
```

Mod 目录须包含 `descriptor.mod`。原版最先加载，多个 `--mod` 按提供顺序加载；依赖须存在并排在依赖方之前。工具不自动调整顺序。

后续来源覆盖同路径文件，`replace_path` 排除先前来源中的对应目录。报告保留覆盖与排除原因；被排除且没有替代文件的路径仍可查询。`--list` 列出全部追踪路径，`--parse` 只解析有效文件。

文件路径按 ASCII 大小写不敏感匹配，保留物理文件的原有拼写。查询接受两种路径分隔符，描述中的 `replace_path` 须使用 `/`。绝对路径、上级跳转、来源内的大小写歧义，以及缺失或顺序错误的依赖都会报错。

## 查询定义

```sh
cargo run -p eu4-inspect -- definitions "<原版目录>" buildings --mod "<Mod 目录>" --name "example_building"
cargo run -p eu4-inspect -- definitions "<原版目录>" scripted-effects --mod "<Mod 目录>" --duplicates
```

类别支持 `buildings`、`scripted-effects` 和 `scripted-triggers`。索引有效 `.txt` 文件中的顶层赋值块，跳过 `@` 常量，报告声明的来源、文件和行列。定义名保留大小写；同名声明全部列出，当前不判定其生效优先级。`--list` 列出所有定义。

## 编码与限制

描述文件默认 UTF-8，普通文本默认 Windows-1252。分别用 `--descriptor-encoding` 和 `--encoding` 显式选择 `utf8` 或 `windows1252`；单独的 `descriptor` 命令使用 `--encoding`。错误中的列号按字节计算。

当前只支持目录中的文件，不支持压缩 Mod、DLC 挂载、脚本执行或存档。文件改变后需要重新生成清单。成功解析与建立索引不代表已经兼容相应游戏机制。
