# 直属属国遍历

自制场景包含五个虚构国家。AAA的直属属国为BBB和CCC，DDD属于BBB。国家执行顺序声明使CCC先于BBB；显式数量观察为9，与当前成员数分别保存。

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/subject-iteration --trigger subject_demo.1 --save subject_pending.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/subject-iteration --input native --load subject_pending.json --choose 0:0 --days 2 --save subject_child.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/subject-iteration --load subject_child.json --choose 1:0 --days 6
```

遍历只访问当前直属关系，进入时捕获成员，每项执行前读取limit。前一项的写入可以使后续项通过或跳过筛选。普通遍历和嵌套块保留调用者ROOT；接收事件建立自己的ROOT。延期计划在执行时选择成员，选出的事件继续使用已有待选和读档流程。

核心的subject_country_order声明国家优先级，不创建成员关系；非空时必须完整列出所有国家且不重复。未声明时使用实体顺序，旧输入和存档保持原行为。新增国家追加到已声明顺序。项目存档保存非默认顺序，恢复时验证实体类型、完整性与唯一性。

兼容入口按国家声明顺序提供优先级；当前原版单文件对照通过，多文件国家登记和Mod覆盖顺序仍需扩大验证。every_subject_country要求国家作用域，limit和正文均检查支持范围；不可达分支也不能包含未知效果。完整宗属关系创建、解除副作用、数量刷新及原版战役仍未完成。
