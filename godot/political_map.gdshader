// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

shader_type canvas_item;
uniform sampler2D province_palette : filter_nearest, repeat_disable;
uniform int palette_size = 0;
uniform float palette_width = 1.0;

int color_key(vec3 color) {
    ivec3 rgb = ivec3(round(color * 255.0));
    return rgb.r * 65536 + rgb.g * 256 + rgb.b;
}

void fragment() {
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
            break;
        }
        if (candidate < wanted) { low = middle + 1; }
        else { high = middle - 1; }
    }
    COLOR = vec4(result, 1.0);
}
