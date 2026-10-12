# 拥有省份的条件查询

这个自制场景不需要 EU4 安装。`AAA` 拥有省份 2、3，`BBB` 拥有省份 1；省份 3 初始附着 `local` 修正。

```sh
cargo run --locked -p augustus-run --bin augustus-events -- \
  --content examples/owned-provinces --input eu4 \
  --trigger owned.start --choose 0:0 --save owned.json

cargo run --locked -p augustus-run --bin augustus-events -- \
  --content examples/owned-provinces --input native \
  --load owned.json --days 1 --choose 1:0
```

第一步查询当前国家的省份，并通过首都进入所有者后重复查询。选项移除省份修正，随后的条件和 `after` 立即读取新状态。第二步从原生入口读档，延期事件再次检查同一份状态。最终日志依次为 `found`、`foreign`、`owner`、`removed`、`after`、`later`。

兼容层将国家作用域的 `any_owned_province = { ... }` 翻译为核心 `AnyOwnedProvince`，内部条件在各省份上求值；空块与 `always = yes` 都检查是否存在拥有省份。归属索引随 `set_owner` 更新，读档时由关系重建，不固定为开局列表。查询只读，不改变存档或随机状态。

已知成员满足条件即可返回真；没有匹配时，只有归属资料完整才能确定为假。未知归属继续报错，否定条件不能将其当作不存在。拥有省份的效果遍历与随机选择见 [对应场景](../owned-iteration/README.md)，兼容脚本中的完整归属变更尚未支持。
