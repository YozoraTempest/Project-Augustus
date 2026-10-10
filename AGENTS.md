# 开发约定

修改前阅读 `CONTRIBUTING.md` 和相关代码。`crates/augustus-eu4` 负责 EU4 文件与加载处理，`tools/eu4-inspect` 是其命令行调用方。改动保持在实际需要的模块内，不引入没有消费者的接口或依赖。

文档面向使用者和贡献者，介绍实际功能、操作方法及验证要求。代码与测试不得包含未经许可的原版文件、完整第三方脚本或反编译结果。

Rust 代码改动须通过：

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```
