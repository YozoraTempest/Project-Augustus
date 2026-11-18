# 界面占位

占位值在 `crates/augustus-eu4/src/interface/mock.rs`。界面只读这张表，不自己写死数字。国库和人力不在表里，它们走现有的国家财政和人力接口。

| 控件 | 占位 | 以后换成 |
| --- | --- | --- |
| text_ADM | 0 | 国家行政点库存。模拟里还没有这项。 |
| text_DIP | 0 | 国家外交点库存。模拟里还没有这项。 |
| text_MIL | 0 | 国家军事点库存。模拟里还没有这项。 |
| text_merchants | 0 | 商人数量。 |
| text_settlers | 0 | 殖民者数量。 |
| text_diplomats | 0 | 外交官数量。 |
| text_missionaries | 0 | 传教士数量。 |
| text_sailors | 0 | 水手库存。 |
| text_stability | 0 | 稳定度。 |
| text_prestige | 0 | 威望。 |
| text_dev | 0 | 奉献度。 |
| text_meritocracy | 0 | 贤能值。 |
| text_leg | 0 | 正统性。 |
| text_rep | 0 | 共和传统。 |
| text_unity | 0 | 游牧统一。 |
| text_powerprojection | 0 | 力量投射。 |
| text_corruption | 0 | 腐败度。 |
| icon_gold 的帧 | 国库为负用第 2 帧，否则第 1 帧 | Kaiser 顶栏刷新里那组还没对上具体账目的差额。现在暂用 `get_country_finances` 的 `treasury_negative`。 |
