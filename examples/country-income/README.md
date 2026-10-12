# 月度君主点数

这个自制场景把君主能力、国家焦点和国家修正组成实时收入，再在月初计入点数库存。两份各加 0.6 的修正合计贡献 1 点；切换焦点或修改能力会立即改变收入，库存到结算时才变化。

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/country-income --trigger income.begin --choose 0:0 --save income-waiting.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/country-income --input native --load income-waiting.json --days 20 --choose 1:0 --save income-december.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/country-income --load income-december.json --days 31
```

第一条命令后的库存仍为 10、10、10。12 月 1 日的事件先读取这组库存，再清除焦点；随后月结按 7、7、9 入账，库存变为 17、17、19。选择第二个事件的选项会移除一份修正，收入变为 6、6、8。下一个月库存达到各自的增长上限 20。

`attributes.formulas` 声明收入公式，`monthly_accruals.rules` 指定入账库存及单位换算；`last_settlement` 保存上一次的收入、结算前后库存、实际入账量和缺项。收入每次从当前状态计算，读档不会再次执行已经完成的月结。月结先计算全部账户，再写入结果；计算错误不会留下部分入账。

EU4 写法支持 `monthly_adm`、`monthly_dip`、`monthly_mil` 的大于等于条件，以及 `national_focus = ADM/DIP/MIL`、`change_national_focus = ADM/DIP/MIL/none`。条件中的字面小数向零截断。原生内容使用 `NumericSlot::Attribute`、`NationalFocus` 条件和 `ChangeNationalFocus` 效果。

本场景明确提供了完整公式、初始库存和上限。原版载入的顾问、其他收入来源、开局库存与动态上限仍有缺项，完整收入保持未知；这份演示不能代表原版战役已经跑通。
