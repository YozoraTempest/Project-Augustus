// SPDX-License-Identifier: GPL-3.0-only

// 编号纹理按原始字节读取；显示颜色使用颜色空间转换。
uniform sampler2D province_ids : filter_nearest, repeat_disable;
uniform sampler2D owner_ids : filter_nearest, repeat_disable;
uniform sampler2D political_colors : source_color, filter_nearest, repeat_disable;
uniform sampler2D terrain_color : source_color, filter_linear, repeat_disable;
uniform sampler2D terrain_atlas : source_color, filter_linear, repeat_enable;
uniform sampler2D terrain_ids : filter_nearest, repeat_disable;
uniform sampler2D water_color : source_color, filter_linear, repeat_disable;
uniform sampler2D water_detail : source_color, filter_linear, repeat_enable;
uniform bool use_terrain = false;
uniform bool use_water = false;
uniform vec2 terrain_map_size = vec2(5632.0, 2048.0);
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

float overlay_channel(float base, float over) {
    if (over < 0.5) {
        return 2.0 * over * base;
    }
    return 1.0 - 2.0 * (1.0 - over) * (1.0 - base);
}

// gfx/FX/pdxmap.shader：图集 4×4，序号来自 terrain.bmp，颜色来自季节色图。
vec3 terrain_rgb(vec2 uv) {
    uv = map_uv(uv);
    float id = texture(terrain_ids, uv).r * 255.0;
    float index_v = floor((id + 0.5) / 4.0);
    float index_u = floor(id - index_v * 4.0 + 0.5);
    index_u = clamp(index_u, 0.0, 3.0);
    index_v = clamp(index_v, 0.0, 3.0);
    vec2 repeated = uv * 128.0;
    repeated.x *= terrain_map_size.x / max(terrain_map_size.y, 1.0);
    repeated = fract(repeated);
    repeated *= 511.0 / 512.0;
    vec2 atlas_uv = (vec2(index_u, index_v) + repeated) / 4.0 + vec2(0.5 / 2048.0);
    vec3 detail = texture(terrain_atlas, atlas_uv).rgb;
    vec3 tint = texture(terrain_color, uv).rgb;
    vec3 land = mix(detail, vec3(overlay_channel(detail.r, tint.r), overlay_channel(detail.g, tint.g), overlay_channel(detail.b, tint.b)), 0.75);
    // terrain.txt：ocean 是 15，inland_ocean 是 17。水下着色把贴图按地图尺寸 / 32 重复。
    if (use_water && (abs(id - 15.0) < 0.5 || abs(id - 17.0) < 0.5)) {
        vec3 bed = texture(water_detail, uv * terrain_map_size / 32.0).rgb;
        vec3 sea = texture(water_color, uv).rgb;
        return mix(bed, sea, 0.65);
    }
    return land;
}

vec3 map_color(vec2 uv, vec2 step_uv) {
    uint id = region_id(uv);
    uint selected = country_mode ? selected_country : selected_province;
    vec3 color = use_terrain ? terrain_rgb(uv) : texture(political_colors, map_uv(uv)).rgb;
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
