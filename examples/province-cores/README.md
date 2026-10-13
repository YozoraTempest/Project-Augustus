<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 Project Augustus contributors -->

# 省份核心演示

此场景使用自制国家和省份，展示核心与拥有权的区别。AAA 可以在 BBB 拥有的省份 8 添加完整核心，选择事件选项后改为领土核心，再通过第二个事件转回完整核心或移除。省份事件还展示了为没有省份的 CCC 添加核心。

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/province-cores --input eu4 --trigger cores.seed --entity 0 --choose 0:0 --save cores.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/province-cores --input native --load cores.json --trigger cores.promote --entity 0
```

两种输入进入同一个执行器，可以互相读取存档。视图中的 `administration.provinces` 包含当前核心类型；这些操作不转移省份，不改变州状态或原始自治度。
