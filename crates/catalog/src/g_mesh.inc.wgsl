// Animated four-point bilinear mesh gradient with moving points and turbulence.
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let t = time_seconds() * p_speed() * 6.2831853;
  let m = clamp(p_turbulence(), 0.0, 1.0);
  var p = uv;
  let n = fbm(uv * 4.0 + vec2<f32>(t), 4, 2.0, 0.5) - 0.5;
  p = uv + vec2<f32>(n) * m * 0.2;
  let c00 = mix(vec3<f32>(0.9, 0.2, 0.4), vec3<f32>(0.2, 0.4, 0.9), 0.5 + 0.5 * sin(t));
  let c10 = mix(vec3<f32>(0.2, 0.4, 0.9), vec3<f32>(0.9, 0.8, 0.2), 0.5 + 0.5 * cos(t * 0.8));
  let c01 = mix(vec3<f32>(0.2, 0.9, 0.5), vec3<f32>(0.9, 0.2, 0.4), 0.5 + 0.5 * sin(t * 1.1));
  let c11 = mix(vec3<f32>(0.9, 0.8, 0.2), vec3<f32>(0.2, 0.9, 0.5), 0.5 + 0.5 * cos(t * 1.2));
  let wx = smoothstep(0.0, 1.0, clamp(p.x, 0.0, 1.0));
  let wy = smoothstep(0.0, 1.0, clamp(p.y, 0.0, 1.0));
  let col = mix(mix(c00, c10, wx), mix(c01, c11, wx), wy);
  return premul(col, 1.0);
}
