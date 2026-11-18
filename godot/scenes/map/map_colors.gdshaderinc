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
uniform sampler2D country_border : source_color, filter_linear, repeat_enable;
uniform sampler2D province_border : source_color, filter_linear, repeat_enable;
uniform bool use_terrain = false;
uniform bool use_water = false;
uniform bool terrain_ready = false;
uniform bool water_ready = false;
uniform bool use_borders = false;
uniform vec2 terrain_map_size = vec2(5632.0, 2048.0);
uniform uint selected_province = 0u;
uniform uint selected_country = 0u;
uniform bool country_mode = true;
uniform bool wrap_horizontal = false;

vec2 map_uv(vec2 uv) {
    return vec2(wrap_horizontal ? fract(uv.x) : clamp(uv.x, 0.0, 1.0), clamp(uv.y, 0.0, 1.0));
}

uint unpack_id(vec4 encoded) {
    uvec4 bytes = uvec4(round(encoded * 255.0));
    return bytes.r | (bytes.g << 8u) | (bytes.b << 16u) | (bytes.a << 24u);
}

uint owner_at(vec2 uv) {
    return unpack_id(texture(owner_ids, map_uv(uv)));
}

uint province_at(vec2 uv) {
    return unpack_id(texture(province_ids, map_uv(uv)));
}

uint region_id(vec2 uv) {
    return country_mode ? owner_at(uv) : province_at(uv);
}

float overlay_channel(float base, float over) {
    if (over < 0.5) {
        return 2.0 * over * base;
    }
    return 1.0 - 2.0 * (1.0 - over) * (1.0 - base);
}

// gfx/FX/pdxmap.shader：图集 4×4，序号来自 terrain.bmp，颜色来自季节色图。
vec3 terrain_surface(vec2 uv, float overlay_amount, bool water) {
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
    vec3 land = mix(detail, vec3(overlay_channel(detail.r, tint.r), overlay_channel(detail.g, tint.g), overlay_channel(detail.b, tint.b)), overlay_amount);
    // terrain.txt：ocean 是 15，inland_ocean 是 17。水下着色把贴图按地图尺寸 / 32 重复。
    if (water && (abs(id - 15.0) < 0.5 || abs(id - 17.0) < 0.5)) {
        vec3 bed = texture(water_detail, uv * terrain_map_size / 32.0).rgb;
        vec3 sea = texture(water_color, uv).rgb;
        return mix(bed, sea, 0.65);
    }
    return land;
}

vec3 terrain_rgb(vec2 uv) {
    return terrain_surface(uv, 0.75, use_water);
}

// pdxmap.shader 的 COLOR_SHADER：季节色叠 0.5，灰度乘 0.4，国家色乘 0.45。
// 颜色 alpha 低于 0.82 时不进这道混合，改用海面的水色图和海底贴图。
// defines.lua 的 BORDER_WIDTH 是 1.5 个地图单位。国界取 border0 线芯，省界取 border1，放大后才画省界。
vec3 political_rgb(vec2 uv, vec2 step_uv) {
    uv = map_uv(uv);
    vec4 country_sample = texture(political_colors, uv);
    vec3 color = country_sample.rgb;
    if (country_sample.a < 0.82) {
        if (water_ready) {
            vec3 bed = texture(water_detail, uv * terrain_map_size / 32.0).rgb;
            vec3 sea = texture(water_color, uv).rgb;
            color = mix(bed, sea, 0.65);
        }
    } else if (terrain_ready) {
        vec3 paper = terrain_surface(uv, 0.5, false);
        float grey = dot(paper, vec3(0.212671, 0.715160, 0.072169));
        color = grey * 0.4 + country_sample.rgb * 0.45;
    }
    if (!use_borders) {
        return color;
    }
    uint owner = owner_at(uv);
    uint province = province_at(uv);
    bool sea = false;
    if (terrain_ready) {
        float terrain_id = texture(terrain_ids, uv).r * 255.0;
        sea = abs(terrain_id - 15.0) < 0.5 || abs(terrain_id - 17.0) < 0.5;
    }
    vec2 texel = vec2(1.0) / max(terrain_map_size, vec2(1.0));
    float pixels_per_texel = length(texel) / max(length(step_uv * 0.5), 0.00001);
    vec2 axis[4] = vec2[](vec2(1.0, 0.0), vec2(-1.0, 0.0), vec2(0.0, 1.0), vec2(0.0, -1.0));
    bool country_edge = false;
    bool province_edge = false;
    for (int direction = 0; direction < 4; direction++) {
        vec2 at = uv + axis[direction] * texel * 1.5;
        if (owner_at(at) != owner) {
            country_edge = true;
        } else if (province_at(at) != province) {
            province_edge = true;
        }
    }
    if (country_edge) {
        vec4 ink = texture(country_border, vec2(0.5, 0.5));
        return mix(color, ink.rgb, ink.a);
    }
    // 缩到全国时一个省往往不到两个像素，省界会把国家涂成格子。
    if (!sea && province_edge && pixels_per_texel > 1.6) {
        vec4 ink = texture(province_border, vec2(0.5, 0.5));
        return mix(color, ink.rgb, ink.a);
    }
    return color;
}

vec3 map_color(vec2 uv, vec2 step_uv) {
    uint id = region_id(uv);
    uint selected = country_mode ? selected_country : selected_province;
    vec3 color = use_terrain ? terrain_rgb(uv) : political_rgb(uv, step_uv);
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
