import re, sys
path = sys.argv[1]
text = open(path, encoding="utf-8").read()
fn_names = ["fx001_bloom", "fx002_bokeh", "fx003_channel_mixer", "fx004_chromatic_metal", "fx005_color_adjustment",
    "fx006_colored_edges", "fx007_dither", "fx008_filter_preset", "fx009_gooey", "fx010_gradient_map",
    "fx011_halftone", "fx012_hatching", "fx013_lens_distortion", "fx014_outlines", "fx015_pattern_refraction",
    "fx016_pixel_stretch", "fx017_pixelate", "fx018_slice_shift", "fx019_warp", "fx020_clouds", "fx021_concentric",
    "fx022_fractal_noise", "fx023_glowing_wave", "fx024_mesh_gradient", "fx025_moire", "fx026_nebula",
    "fx027_pattern_grid", "fx028_water_caustic",
    "fx100_tone", "fx101_color", "fx102_noise", "fx103_blur", "fx104_glow", "fx105_chromatic", "fx106_vhs",
    "fx107_crt", "fx108_glitch", "fx109_pixel", "fx110_smear", "fx111_geometry", "fx112_glass", "fx113_materials",
    "fx114_texture", "fx115_print", "fx116_technical", "fx117_text_graphic", "fx118_temporal", "fx119_feedback",
    "fx120_keys", "fx121_masks", "fx122_composite", "fx123_transforms",
    "g001_solid", "g002_gradient", "g003_mesh_gradient", "g004_noise", "g005_voronoi", "g006_plasma", "g007_grid",
    "g008_concentric", "g009_moire", "g010_pattern_fill", "g011_topographic", "g012_clouds_nebula",
    "g013_oscilloscope", "g014_caustics", "g015_fractals", "g016_lissajous", "g017_superformula", "g018_truchet",
    "g019_texture_fields", "g020_starfield", "g021_test_signal", "g022_polar_tunnel", "g023_lsystem",
    "g024_attractor", "g025_kaleidoscope", "g026_recursive_tiling", "g027_automata", "g028_reaction_diffusion",
    "g030_svg", "g031_path_editor", "g032_trim_paths", "g033_follow_path", "g034_path_morph", "g035_shape_morph",
    "g036_repeater", "g037_vector_deform", "g038_stroke_fx", "g039_gradient_animator", "g040_vector_mask",
    "g041_vector_particles", "g042_vector_mesh", "g043_lottie", "g044_icon_scene", "g045_handdrawn",
    "g070_text_plate", "g071_kinetic", "g072_typewriter", "g073_text_path", "g074_text_3d", "g075_text_particles",
    "g076_physics_type", "g077_counters", "g078_charts", "g079_overlays",
    "g050_ken_burns", "g051_corner_pin", "g052_mesh_warp", "g053_liquify", "g054_parallax", "g055_heightfield",
    "g056_image_particles", "g057_fragmentation", "g058_slices", "g059_slit_scan", "g060_polar_projection",
    "g061_collage", "g062_sprite", "g063_motion_brush", "g064_optical_flow", "g065_image_morph",
    "g066_field_extraction", "g067_droste", "g068_stop_motion"]

for name in fn_names:
    text = re.sub(r'crate::([\w]+)::' + name + r'(?!\w)',
                  lambda m: 'crate::%s::%s()' % (m.group(1), name), text)

# row-returning functions: sdf_rows() etc. Should be chained.
text = text.replace('for (s, m, b) in crate::generators_d::sdf_rows()', 'for (s, m, b) in crate::generators_d::sdf_rows()')
open(path, "w", encoding="utf-8", newline="").write(text)
print("done")
