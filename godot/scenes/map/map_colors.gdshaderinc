// SPDX-License-Identifier: GPL-3.0-only

// 编号纹理按原始字节读取；显示颜色使用颜色空间转换。
uniform sampler2D province_ids : filter_nearest, repeat_disable;
uniform sampler2D owner_ids : filter_nearest, repeat_disable;
uniform sampler2D political_colors : source_color, filter_nearest, repeat_disable;
uniform uint selected_province = 0u;
uniform uint selected_country = 0u;
uniform bool country_mode = true;
uniform bool wrap_horizontal = false;

vec2 map_uv(vec2 uv) {
    return vec2(wrap_horizontal ? fract(uv.x) : clamp(uv.x, 0.0, 1.0), clamp(uv.y, 0.0, 1.0));
}

uint region_id(vec2 uv) {
    uv = map_uv(uv);
    vec4 encoded = country_mode ? texture(owner_ids, uv) : texture(province_ids, uv);
    uvec4 bytes = uvec4(round(encoded * 255.0));
    return bytes.r | (bytes.g << 8u) | (bytes.b << 16u) | (bytes.a << 24u);
}

vec3 map_color(vec2 uv, vec2 step_uv) {
    uint id = region_id(uv);
    uint selected = country_mode ? selected_country : selected_province;
    vec3 color = texture(political_colors, map_uv(uv)).rgb;
    if (id != 0u && id == selected) {
        color = mix(color, vec3(1.0, 0.78, 0.24), 0.7);
        // 球面经线首尾相连；轮廓按选择单位计算，国家内部不绘制省界。
        bool border = (!wrap_horizontal && (uv.x < step_uv.x || uv.y < step_uv.y
            || uv.x > 1.0 - step_uv.x || uv.y > 1.0 - step_uv.y))
            || region_id(uv - vec2(step_uv.x, 0.0)) != id
            || region_id(uv + vec2(step_uv.x, 0.0)) != id
            || region_id(uv - vec2(0.0, step_uv.y)) != id
            || region_id(uv + vec2(0.0, step_uv.y)) != id;
        if (border) {
            color = vec3(1.0, 0.9, 0.5);
        }
    }
    return color;
}
