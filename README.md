# 奥古斯都计划 · Project Augustus

Project Augustus 是使用 Rust 重新实现《欧陆风云 IV》（EU4）游戏引擎的开源项目。

项目正在从零建设，目前完成工程初始化，命令行支持帮助和版本查询。`demo/` 保存此前的实现，供开发参考。

## 运行

安装 Rust（rustup）及本机构建工具后，在仓库根目录执行：

```bash
cargo build --workspace --locked
cargo run --locked -- --help
cargo run --locked -- --version
```

## 许可

项目代码采用 [GNU GPLv3](LICENSE)（`GPL-3.0-only`）。EU4 游戏文件由玩家自行提供，不随项目分发。

本项目与 Paradox Interactive 无隶属关系。
