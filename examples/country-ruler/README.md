# 当前君主能力

这个自制场景演示 `adm`、`dip`、`mil` 条件，以及 `change_adm`、`change_dip`、`change_mil` 效果。条件读取当前君主的能力；变更也写入这个人物，不影响继承人和配偶。

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/country-ruler --trigger skill.begin --choose 0:0 --save ruler-waiting.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/country-ruler --input native --load ruler-waiting.json --days 1 --choose 1:0
```

场景明确将变更范围设为 2～4，君主初始能力为 5 / 3 / 1。载入时保留这些初值；执行第一组变更后为 4 / 4 / 2，选择选项后为 2 / 4 / 2，次日完成事件后为 4 / 2 / 4。继承人的能力保持 1 / 2 / 3。

原生内容在 `politics.ruler_skill_bounds` 中提供 `minimum` 和 `maximum`，在 `politics.countries` 中提供人物状态。`ChangeRulerSkill` 使用这组范围；`NumericSlot::RulerSkill` 用于只读查询。缺少规则、当前君主或所查询的能力时报告未知，不自动创建人物或填入零。

EU4 输入中的小数向零截断，目前支持最多三位小数的字面数值。原版加载从 `MONARCH_MIN_SKILL`、`MONARCH_MAX_SKILL` 读取变更范围，不为缺少的定义猜测默认值。跨国能力比较、人物生成和继位、顾问、点数收入及其初始化尚未接入。
