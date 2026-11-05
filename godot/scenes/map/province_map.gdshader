// SPDX-License-Identifier: GPL-3.0-only

shader_type canvas_item;
render_mode unshaded;

// 编号纹理是原始数据，不使用颜色空间转换、线性过滤或重复采样。
uniform sampler2D province_ids : filter_nearest, repeat_disable;
// 归属编号与显示颜色分离，同色国家不会被当成同一个国家。
uniform sampler2D owner_ids : filter_nearest, repeat_disable;
// 国家颜色是显示数据，使用颜色空间转换；编号纹理仍按原始字节读取。
uniform sampler2D political_colors : source_color, filter_nearest, repeat_disable;
uniform uint selected_province = 0u;
uniform uint selected_country = 0u;
uniform bool country_mode = true;

uint region_id(vec2 uv) {
    vec4 encoded = country_mode ? texture(owner_ids, uv) : texture(province_ids, uv);
    uvec4 bytes = uvec4(round(encoded * 255.0));
    return bytes.r | (bytes.g << 8u) | (bytes.b << 16u) | (bytes.a << 24u);
}

void fragment() {
    uint id = region_id(UV);
    uint selected = country_mode ? selected_country : selected_province;
    vec3 color = texture(political_colors, UV).rgb;
    if (id != 0u && id == selected) {
        color = mix(color, vec3(1.0, 0.78, 0.24), 0.7);
        // 轮廓以当前模式的选择单位计算，国家内部的省界不会成为选中轮廓。
        vec2 step_uv = fwidth(UV) * 2.0;
        bool border = UV.x < step_uv.x || UV.y < step_uv.y
            || UV.x > 1.0 - step_uv.x || UV.y > 1.0 - step_uv.y
            || region_id(UV - vec2(step_uv.x, 0.0)) != id
            || region_id(UV + vec2(step_uv.x, 0.0)) != id
            || region_id(UV - vec2(0.0, step_uv.y)) != id
            || region_id(UV + vec2(0.0, step_uv.y)) != id;
        if (border) {
            color = vec3(1.0, 0.9, 0.5);
        }
    }
    COLOR = vec4(color, 1.0);
}
