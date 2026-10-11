# 开发约定

修改前阅读 `CONTRIBUTING.md` 和相关代码。`augustus-core` 负责世界状态和执行计划，不依赖 EU4 或 Godot；`augustus-eu4` 负责文件处理与受支持脚本的编译；`augustus-demo` 提供自制场景和建造系统；`augustus-godot` 连接界面。命令行工具在 `tools/` 下。改动保持在实际需要的模块内，不引入没有消费者的接口或依赖。

文档面向使用者和贡献者，介绍实际功能、操作方法及验证要求。代码与测试不得包含未经许可的原版文件、完整第三方脚本或反编译结果。

Rust 代码改动须通过：

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

涉及 Godot 绑定或界面的改动，还须运行 `docs/CONSTRUCTION_DEMO.md` 中的 Godot 集成检查。
