# 拥有省份的遍历与随机选择

这是自制场景，不需要 EU4 安装。`AAA` 拥有省份 3、1、2，`BBB` 拥有省份 4。兼容入口翻译 EU4 的效果块，原生入口提供共享核心计划。

```sh
cargo run --locked -p augustus-run --bin augustus-events -- \
  --content examples/owned-iteration --input eu4 \
  --trigger traversal.start --choose 0:0 --save traversal.json

cargo run --locked -p augustus-run --bin augustus-events -- \
  --content examples/owned-iteration --input native \
  --load traversal.json --days 1 --choose 1:0 --save traversal_later.json

cargo run --locked -p augustus-run --bin augustus-events -- \
  --content examples/owned-iteration --input eu4 \
  --load traversal_later.json --days 1
```

第一步将两日期限的 `brief` 修正附着到 `AAA`。第二步推进一天并选择延期事件，第三步再推进一天，修正到期移除。存档目标须不存在。

第一轮通过所有者的 `gate` 标记筛选，省份 3 清除标记后，省份 1、2 不再通过。下一轮无筛选，依次记录三个成员。随机块只选择省份 2；恒假筛选不执行正文。嵌套遍历分别记录省份 1、2，然后返回外层省份 3。一天后再遍历，并在省份 1、2 中随机执行一次。选项中的提示文字与期限说明保留在计划中，可由界面查询。

`every_owned_province` 对应核心 `ForEachOwnedProvince`，`random_owned_province` 对应 `RandomOwnedProvince`，两者都接受国家目标、省份条件和正文计划。例如原生遍历为：

```json
{
  "ForEachOwnedProvince": {
    "country": "Current",
    "condition": { "Always": true },
    "plan": [{ "Emit": "member" }]
  }
}
```

成员按取得顺序保存，重新取得的省份追加到末尾，同一所有者的重复赋值不改变顺序。遍历进入时捕获成员列表；正文加入的成员留到下一轮，转出的成员仍留在本轮列表，筛选读取它们的当前状态。嵌套与延期执行分别使用自身上下文，读档保留取得顺序。

随机选择先筛选当前成员，再均匀选择一个；无匹配时不消耗随机状态。两种输入共用现有 ChaCha 状态，保存后续跑一致。项目不承诺与 EU4 产生相同的随机序列。未知省份归属使效果选择报错，不能静默漏掉可能属于该国的成员。

核心 `SetOwner` 只更新省份的所有者关系及成员顺序，供管理其他相关状态的系统使用。兼容层尚不支持 `cede_province`；完整领土转移的控制者、首都、行政和回调仍需接入。
