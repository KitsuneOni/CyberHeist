// Full-screen CRT pass for the terminal screens: scanlines, a faint flicker
// and a darkened edge. Drawn by a ColorRect laid over the whole screen, so the
// content underneath stays ordinary Controls.
shader_type canvas_item;

uniform sampler2D screen_texture : hint_screen_texture, filter_linear;
// Fraction of brightness each dark scanline removes.
uniform float scanline_strength : hint_range(0.0, 1.0) = 0.18;
// Screen pixels per scanline pair.
uniform float scanline_spacing : hint_range(1.0, 8.0) = 3.0;
uniform float vignette_strength : hint_range(0.0, 1.0) = 0.35;
uniform float flicker_strength : hint_range(0.0, 0.1) = 0.015;

void fragment() {
	vec3 colour = texture(screen_texture, SCREEN_UV).rgb;

	float line = step(0.5, fract(FRAGCOORD.y / scanline_spacing));
	colour *= 1.0 - scanline_strength * line;

	colour *= 1.0 - flicker_strength * sin(TIME * 60.0);

	vec2 centred = SCREEN_UV - 0.5;
	colour *= 1.0 - vignette_strength * dot(centred, centred) * 2.0;

	COLOR = vec4(colour, 1.0);
}
