# EU4 文件工具

`eu4-inspect` 只读取本地文件，不修改游戏目录，也不运行游戏。以下命令在仓库根目录执行。

```sh
cargo run -p eu4-inspect -- catalog "<内容目录>"
cargo run -p eu4-inspect -- descriptor "<描述文件路径>"
cargo run -p eu4-inspect -- parse "<内容目录>" "common/buildings/example.txt"
```

`catalog --list` 列出文件；`descriptor` 显示 Mod 元数据，但不挂载声明的 `path` 或 `archive`；`parse` 检查语法并显示前二十个顶层字段。

描述文件默认 UTF-8，普通文本默认 Windows-1252，用 `--encoding` 显式选择 `utf8` 或 `windows1252`。错误中的列号按字节计算。读取路径须位于选中的目录内，绝对路径和上级跳转会报错。

当前只检查单个内容目录，不支持内容合并、压缩 Mod、DLC 挂载、脚本执行或存档。成功解析不代表已经兼容相应游戏机制。
