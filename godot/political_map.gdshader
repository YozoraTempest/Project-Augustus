// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

shader_type canvas_item;
uniform sampler2D province_palette : filter_nearest, repeat_disable;
uniform sampler2D highlight_mask : filter_nearest, repeat_disable;
uniform int palette_size = 0;
uniform float palette_width = 1.0;
uniform bool highlight_enabled = false;
uniform vec3 highlight_color : source_color = vec3(1.0, 0.82, 0.18);
uniform int selected_province_key = -1;
uniform vec3 selection_color : source_color = vec3(0.55, 0.95, 1.0);

int color_key(vec3 color) {
    ivec3 rgb = ivec3(round(color * 255.0));
    return rgb.r * 65536 + rgb.g * 256 + rgb.b;
}

void fragment() {
    // Keep the inner border in screen pixels, even on magnified low-resolution maps.
    vec2 outline_step = fwidth(UV) * 2.0;
    int wanted = color_key(texture(TEXTURE, UV).rgb);
    int low = 0;
    int high = palette_size - 1;
    vec3 result = vec3(0.12);
    for (int iteration = 0; iteration < 24; iteration++) {
        if (low > high) { break; }
        int middle = (low + high) / 2;
        float x = (float(middle) + 0.5) / palette_width;
        int candidate = color_key(texture(province_palette, vec2(x, 0.25)).rgb);
        if (candidate == wanted) {
            result = texture(province_palette, vec2(x, 0.75)).rgb;
            if (highlight_enabled) {
                float mask = texture(highlight_mask, vec2(x, 0.5)).r;
                result = mix(result, highlight_color, mask);
            }
            break;
        }
        if (candidate < wanted) { low = middle + 1; }
        else { high = middle - 1; }
    }
    if (wanted == selected_province_key) {
        bool border = UV.x < outline_step.x || UV.y < outline_step.y
            || UV.x > 1.0 - outline_step.x || UV.y > 1.0 - outline_step.y
            || color_key(texture(TEXTURE, UV - vec2(outline_step.x, 0.0)).rgb) != wanted
            || color_key(texture(TEXTURE, UV + vec2(outline_step.x, 0.0)).rgb) != wanted
            || color_key(texture(TEXTURE, UV - vec2(0.0, outline_step.y)).rgb) != wanted
            || color_key(texture(TEXTURE, UV + vec2(0.0, outline_step.y)).rgb) != wanted;
        if (border) { result = selection_color; }
    }
    COLOR = vec4(result, 1.0);
}
