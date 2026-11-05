// SPDX-License-Identifier: GPL-3.0-only

shader_type canvas_item;
render_mode unshaded;

#include "res://scenes/map/map_colors.gdshaderinc"

void fragment() {
    COLOR = vec4(map_color(UV, fwidth(UV) * 2.0), 1.0);
}
