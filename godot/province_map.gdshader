// SPDX-License-Identifier: GPL-3.0-only

shader_type canvas_item;
render_mode unshaded;

// 编号纹理是原始数据，不使用颜色空间转换、线性过滤或重复采样。
uniform sampler2D province_ids : filter_nearest, repeat_disable;
uniform uint selected_province = 0u;

uint province_id(vec2 uv) {
    uvec4 bytes = uvec4(round(texture(province_ids, uv) * 255.0));
    return bytes.r | (bytes.g << 8u) | (bytes.b << 16u) | (bytes.a << 24u);
}

vec3 province_color(uint id) {
    // 根据编号生成稳定的区分色；当前显示省份，不表示国家归属或地形。
    uvec3 mixed = uvec3(id * 1664525u + 1013904223u,
        id * 22695477u + 1u, id * 1103515245u + 12345u);
    return vec3(0.24) + vec3(mixed % uvec3(131u)) / 131.0 * 0.48;
}

void fragment() {
    uint id = province_id(UV);
    vec3 color = id == 0u ? vec3(0.06, 0.08, 0.10) : province_color(id);
    if (id != 0u && id == selected_province) {
        color = mix(color, vec3(1.0, 0.78, 0.24), 0.7);
        // 轮廓宽度使用屏幕像素，窗口缩放后仍能看清选中的省份。
        vec2 step_uv = fwidth(UV) * 2.0;
        bool border = UV.x < step_uv.x || UV.y < step_uv.y
            || UV.x > 1.0 - step_uv.x || UV.y > 1.0 - step_uv.y
            || province_id(UV - vec2(step_uv.x, 0.0)) != id
            || province_id(UV + vec2(step_uv.x, 0.0)) != id
            || province_id(UV - vec2(0.0, step_uv.y)) != id
            || province_id(UV + vec2(0.0, step_uv.y)) != id;
        if (border) {
            color = vec3(1.0, 0.9, 0.5);
        }
    }
    COLOR = vec4(color, 1.0);
}
