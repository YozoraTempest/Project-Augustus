# SPDX-License-Identifier: GPL-3.0-only
class_name MapMode
extends RefCounted

# 模式决定地图的选择单位；国家标签和省份编号仍使用世界中的原始标识。
enum Type {
	COUNTRY,
	PROVINCE,
}
