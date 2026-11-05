// SPDX-License-Identifier: GPL-3.0-only

shader_type spatial;
render_mode specular_disabled;

#include "res://scenes/map/map_colors.gdshaderinc"

varying vec3 sphere_position;

void vertex() {
    sphere_position = VERTEX;
}

void fragment() {
    // 与鼠标拾取共用同一球面坐标约定，不依赖网格自身的 UV 排列。
    vec3 direction = normalize(sphere_position);
    vec2 uv = vec2(atan(direction.x, direction.z) / TAU + 0.5,
        acos(clamp(direction.y, -1.0, 1.0)) / PI);
    vec2 dx = dFdx(uv);
    vec2 dy = dFdy(uv);
    // 跨经线接缝时，导数取相邻的经度差，避免出现整片选中轮廓。
    dx.x -= round(dx.x);
    dy.x -= round(dy.x);
    ALBEDO = map_color(uv, (abs(dx) + abs(dy)) * 2.0);
    ROUGHNESS = 1.0;
    SPECULAR = 0.0;
}
