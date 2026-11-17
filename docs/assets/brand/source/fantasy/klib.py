"""Shared bits for the kingdom reel (film 9): render setup, the reel's
materials (wet marble, crumpled foil, robes), crumpled-geometry helpers,
logos, candles, stars, and the per-frame render to multilayer EXR.

Style after @ozavry's dark-fantasy reels: saturated blue/violet, red accents,
nothing smooth: every surface is crumpled, torn or bent.

Animated shader values (candle flicker, flame rise, twinkle) read one Value
node per material ("Time") keyed linearly at TIME_RATE units per frame, so no
drivers are needed in background renders.
"""
import math
import os
import random
import sys

import bmesh
import bpy
from mathutils import Matrix, Vector, noise

HERE = os.path.dirname(os.path.abspath(__file__))
LOGOS = os.path.join(HERE, "..", "logos")
W, H_RES, FPS = 1080, 1920, 30

rnd = random.Random(7)
sc = None


def args():
    a = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    return a


# ---------------------------------------------------------------- scene
def setup(samples, pct, n_frames):
    global sc
    bpy.ops.wm.read_factory_settings(use_empty=True)
    sc = bpy.context.scene
    sc.render.engine = "CYCLES"
    prefs = bpy.context.preferences.addons["cycles"].preferences
    prefs.compute_device_type = "METAL"
    prefs.get_devices()
    for d in prefs.devices:
        d.use = d.type == "METAL"
    prefs.metalrt = "OFF"
    sc.cycles.device = os.environ.get("DEV", "GPU")
    sc.cycles.samples = samples
    sc.cycles.use_denoising = True
    sc.cycles.denoising_prefilter = "ACCURATE"
    sc.cycles.max_bounces = 6
    sc.cycles.volume_bounces = 0
    sc.cycles.transparent_max_bounces = 96
    sc.cycles.light_sampling_threshold = 0.0
    sc.cycles.seed = 3
    sc.render.resolution_x, sc.render.resolution_y, sc.render.resolution_percentage = W, H_RES, pct
    sc.render.fps = FPS
    sc.frame_start, sc.frame_end = 1, n_frames
    sc.view_settings.view_transform = "Standard"
    sc.view_settings.look = "None"
    sc.render.film_transparent = False
    sc.render.use_motion_blur = os.environ.get("MBLUR", "1") != "0"
    sc.render.motion_blur_shutter = 0.5
    world = bpy.data.worlds.new("W")
    sc.world = world
    world.use_nodes = True
    bg = world.node_tree.nodes["Background"]
    bg.inputs[0].default_value = (0.0012, 0.0008, 0.005, 1)
    bg.inputs[1].default_value = 1.0
    aov = bpy.context.view_layer.aovs.add()
    aov.name = "stars"
    aov.type = "COLOR"
    return sc


def srgb(h):
    h = h.lstrip("#")
    out = []
    for i in (0, 2, 4):
        c = int(h[i:i + 2], 16) / 255
        out.append(c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4)
    return tuple(out) + (1.0,)


# ---------------------------------------------------------------- shader helpers
TIME_RATE = 0.04      # Time units per frame


def mat(name):
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    nt = m.node_tree
    nt.nodes.clear()
    out = nt.nodes.new("ShaderNodeOutputMaterial")
    return m, nt, out


def time_node(nt, rate=TIME_RATE):
    """A Value node that counts up `rate` per frame (linear keys)."""
    v = nt.nodes.new("ShaderNodeValue")
    v.name = "Time"
    v.outputs[0].default_value = 0.0
    v.outputs[0].keyframe_insert("default_value", frame=0)
    v.outputs[0].default_value = rate * 1000.0
    v.outputs[0].keyframe_insert("default_value", frame=1000)
    for fc in _fcurves(nt):
        for k in fc.keyframe_points:
            k.interpolation = "LINEAR"
    return v.outputs[0]


def _fcurves(idblock):
    ad = idblock.animation_data
    if ad is None or ad.action is None:
        return []
    act = ad.action
    if hasattr(act, "fcurves") and len(getattr(act, "fcurves", [])):
        return list(act.fcurves)
    out = []
    for layer in getattr(act, "layers", []):
        for strip in layer.strips:
            for cb in strip.channelbags:
                out.extend(cb.fcurves)
    return out


def linear_keys(idblock):
    for fc in _fcurves(idblock):
        for k in fc.keyframe_points:
            k.interpolation = "LINEAR"


def smooth_keys(idblock):
    for fc in _fcurves(idblock):
        for k in fc.keyframe_points:
            k.interpolation = "BEZIER"
            k.handle_left_type = k.handle_right_type = "AUTO_CLAMPED"


def math_node(nt, op, a=None, b=None, va=None, vb=None):
    m_ = nt.nodes.new("ShaderNodeMath")
    m_.operation = op
    if a is not None:
        nt.links.new(a, m_.inputs[0])
    elif va is not None:
        m_.inputs[0].default_value = va
    if b is not None:
        nt.links.new(b, m_.inputs[1])
    elif vb is not None:
        m_.inputs[1].default_value = vb
    return m_.outputs[0]


def mix_rgb(nt, blend, a=None, b=None, va=None, vb=None, fac=1.0):
    m_ = nt.nodes.new("ShaderNodeMix")
    m_.data_type = "RGBA"
    m_.blend_type = blend
    if isinstance(fac, (int, float)):
        m_.inputs["Factor"].default_value = fac
    else:
        nt.links.new(fac, m_.inputs["Factor"])
    if a is not None:
        nt.links.new(a, m_.inputs["A"])
    elif va is not None:
        m_.inputs["A"].default_value = va
    if b is not None:
        nt.links.new(b, m_.inputs["B"])
    elif vb is not None:
        m_.inputs["B"].default_value = vb
    return m_.outputs["Result"]


def ramp(nt, stops):
    r = nt.nodes.new("ShaderNodeValToRGB")
    el = r.color_ramp.elements
    el[0].position, el[0].color = stops[0][0], srgb(stops[0][1])
    el[1].position, el[1].color = stops[-1][0], srgb(stops[-1][1])
    for p, c in stops[1:-1]:
        e = el.new(p)
        e.color = srgb(c)
    return r


def noise_tex(nt, scale, detail=8.0, distortion=0.0, rough=0.6, coord=None, dims="3D", w=None):
    n = nt.nodes.new("ShaderNodeTexNoise")
    n.noise_dimensions = dims
    n.inputs["Scale"].default_value = scale
    n.inputs["Detail"].default_value = detail
    n.inputs["Roughness"].default_value = rough
    n.inputs["Distortion"].default_value = distortion
    if coord is not None:
        nt.links.new(coord, n.inputs["Vector"])
    if w is not None:
        nt.links.new(w, n.inputs["W"])
    return n


def emission(name, color, strength):
    m, nt, out = mat(name)
    e = nt.nodes.new("ShaderNodeEmission")
    e.inputs["Color"].default_value = color
    e.inputs["Strength"].default_value = strength
    nt.links.new(e.outputs[0], out.inputs["Surface"])
    return m


def flicker_emission(name, color, strength, speed=1.0, depth=0.55):
    """Emission whose strength flickers per object (Object Info random)."""
    m, nt, out = mat(name)
    t = time_node(nt)
    oi = nt.nodes.new("ShaderNodeObjectInfo")
    w = math_node(nt, "ADD", math_node(nt, "MULTIPLY", t, vb=speed * 6.0), math_node(nt, "MULTIPLY", oi.outputs["Random"], vb=97.0))
    n = noise_tex(nt, 1.0, 2, 0.0, 0.5, dims="1D", w=w)
    k = math_node(nt, "ADD", math_node(nt, "MULTIPLY", math_node(nt, "SUBTRACT", n.outputs["Fac"], vb=0.5), vb=2 * depth), vb=1.0)
    e = nt.nodes.new("ShaderNodeEmission")
    e.inputs["Color"].default_value = color
    nt.links.new(math_node(nt, "MULTIPLY", math_node(nt, "MAXIMUM", k, vb=0.15), vb=strength), e.inputs["Strength"])
    nt.links.new(e.outputs[0], out.inputs["Surface"])
    return m


def marble(name, scale, stops, rough=0.32, glow=0.12, bump=0.25, distortion=2.2, stretch=(1, 1, 1), coord="Object"):
    """The reel's wet blue/violet marble: distorted noise through a hard ramp,
    glossy, a little self-glow so the colours stay loud in the dark."""
    m, nt, out = mat(name)
    tc = nt.nodes.new("ShaderNodeTexCoord")
    mp = nt.nodes.new("ShaderNodeMapping")
    mp.inputs["Scale"].default_value = stretch
    nt.links.new(tc.outputs[coord], mp.inputs["Vector"])
    n = noise_tex(nt, scale, 10, distortion, 0.62, mp.outputs[0])
    r = ramp(nt, stops)
    nt.links.new(n.outputs["Fac"], r.inputs["Fac"])
    b = nt.nodes.new("ShaderNodeBsdfPrincipled")
    nt.links.new(r.outputs["Color"], b.inputs["Base Color"])
    nt.links.new(r.outputs["Color"], b.inputs["Emission Color"])
    b.inputs["Emission Strength"].default_value = glow
    b.inputs["Roughness"].default_value = rough
    bp = nt.nodes.new("ShaderNodeBump")
    bp.inputs["Strength"].default_value = bump
    nt.links.new(n.outputs["Fac"], bp.inputs["Height"])
    nt.links.new(bp.outputs["Normal"], b.inputs["Normal"])
    nt.links.new(b.outputs[0], out.inputs["Surface"])
    return m


GROUND_STOPS = [(0.0, "#020114"), (0.36, "#06044A"), (0.46, "#1510B8"), (0.56, "#3524F5"),
                (0.66, "#6A2EFF"), (0.76, "#B05CFF"), (0.86, "#F0B0FF")]


def foil_mat(name, fire=False, fire_height=9.0, scale=0.16):
    """Crumpled foil crystal: violet to blue with pink creases, glitter, and
    (fire=True) red fire showing through at the foot."""
    m_, nt, out = mat(name)
    tc = nt.nodes.new("ShaderNodeTexCoord")
    co = tc.outputs["Object"]
    cn = noise_tex(nt, scale, 8, 2.4, 0.58, co)
    cr = ramp(nt, [(0.0, "#05031C"), (0.32, "#170D78"), (0.46, "#3A1FD8"), (0.58, "#6A2CFF"),
                   (0.7, "#A548FF"), (0.82, "#F08AF0"), (1.0, "#FFD0FF")])
    nt.links.new(cn.outputs["Fac"], cr.inputs["Fac"])
    geo = nt.nodes.new("ShaderNodeNewGeometry")
    pr = ramp(nt, [(0.0, "#101010"), (0.44, "#454545"), (0.5, "#B0B0B0"), (0.56, "#FFFFFF"), (1.0, "#FFFFFF")])
    nt.links.new(geo.outputs["Pointiness"], pr.inputs["Fac"])
    crease = mix_rgb(nt, "MULTIPLY", cr.outputs["Color"], pr.outputs["Color"])
    b = nt.nodes.new("ShaderNodeBsdfPrincipled")
    nt.links.new(crease, b.inputs["Base Color"])
    b.inputs["Metallic"].default_value = 0.55
    b.inputs["Roughness"].default_value = 0.2
    vo = nt.nodes.new("ShaderNodeTexVoronoi")
    vo.inputs["Scale"].default_value = scale * 4.4
    nt.links.new(co, vo.inputs["Vector"])
    fine = noise_tex(nt, scale * 37, 4, 0.0, 0.5, co)
    b1 = nt.nodes.new("ShaderNodeBump")
    b1.inputs["Strength"].default_value = 1.0
    b1.inputs["Distance"].default_value = 0.2 / scale
    nt.links.new(vo.outputs["Distance"], b1.inputs["Height"])
    b2 = nt.nodes.new("ShaderNodeBump")
    b2.inputs["Strength"].default_value = 0.12
    nt.links.new(fine.outputs["Fac"], b2.inputs["Height"])
    nt.links.new(b1.outputs["Normal"], b2.inputs["Normal"])
    nt.links.new(b2.outputs["Normal"], b.inputs["Normal"])
    gl = nt.nodes.new("ShaderNodeTexVoronoi")
    gl.inputs["Scale"].default_value = scale * 22
    nt.links.new(co, gl.inputs["Vector"])
    gs = nt.nodes.new("ShaderNodeSeparateColor")
    nt.links.new(gl.outputs["Color"], gs.inputs[0])
    spark = math_node(nt, "GREATER_THAN", gs.outputs[0], vb=0.99)
    sp_c = mix_rgb(nt, "MULTIPLY", spark, vb=(1.1, 0.55, 2.2, 1))
    glow = mix_rgb(nt, "MULTIPLY", crease, vb=(0.3, 0.3, 0.3, 1))
    last = mix_rgb(nt, "ADD", glow, sp_c)
    if fire:
        fn = noise_tex(nt, scale * 5.6, 6, 4.0, 0.7, co)
        fr = ramp(nt, [(0.0, "#000000"), (0.55, "#000000"), (0.68, "#B00010"), (0.8, "#FF3A12"), (1.0, "#FFB060")])
        nt.links.new(fn.outputs["Fac"], fr.inputs["Fac"])
        sep = nt.nodes.new("ShaderNodeSeparateXYZ")
        nt.links.new(co, sep.inputs[0])
        foot = nt.nodes.new("ShaderNodeMapRange")
        foot.inputs["From Min"].default_value = 0.0
        foot.inputs["From Max"].default_value = fire_height
        foot.inputs["To Min"].default_value = 6.0
        foot.inputs["To Max"].default_value = 0.0
        nt.links.new(sep.outputs["Z"], foot.inputs["Value"])
        fm = mix_rgb(nt, "MULTIPLY", fr.outputs["Color"], foot.outputs["Result"])
        last = mix_rgb(nt, "ADD", last, fm)
    nt.links.new(last, b.inputs["Emission Color"])
    b.inputs["Emission Strength"].default_value = 1.0
    nt.links.new(b.outputs[0], out.inputs["Surface"])
    return m_


def cloth_bsdf(nt, coord, hem, base=("#04031A", "#0B0A38", "#1C1A6E", "#2C2AA0"), sheen="#5A52D8"):
    sep = nt.nodes.new("ShaderNodeSeparateXYZ")
    nt.links.new(coord, sep.inputs[0])
    ang = math_node(nt, "ARCTAN2", sep.outputs["Y"], sep.outputs["X"])
    wob = noise_tex(nt, 2.5, 4, 0.0, 0.5, coord)
    angw = math_node(nt, "ADD", ang, math_node(nt, "MULTIPLY", wob.outputs["Fac"], vb=1.4))
    folds = math_node(nt, "SINE", math_node(nt, "MULTIPLY", angw, vb=9.0))
    folds01 = math_node(nt, "ADD", math_node(nt, "MULTIPLY", folds, vb=0.5), vb=0.5)
    weave = noise_tex(nt, 260.0, 2, 0.0, 0.5, coord)
    cr = ramp(nt, [(0.0, base[0]), (0.45, base[1]), (0.8, base[2]), (1.0, base[3])])
    nt.links.new(folds01, cr.inputs["Fac"])
    geo = nt.nodes.new("ShaderNodeNewGeometry")
    pr = ramp(nt, [(0.0, "#1A1A1A"), (0.45, "#5A5A5A"), (0.5, "#B8B8B8"), (0.56, "#FFFFFF"), (1.0, "#FFFFFF")])
    nt.links.new(geo.outputs["Pointiness"], pr.inputs["Fac"])
    crease = mix_rgb(nt, "MULTIPLY", cr.outputs["Color"], pr.outputs["Color"])
    inside = mix_rgb(nt, "MIX", crease, vb=(0.002, 0.001, 0.008, 1), fac=geo.outputs["Backfacing"])
    b = nt.nodes.new("ShaderNodeBsdfPrincipled")
    nt.links.new(inside, b.inputs["Base Color"])
    b.inputs["Roughness"].default_value = 0.62
    b.inputs["Sheen Weight"].default_value = 0.75
    b.inputs["Sheen Roughness"].default_value = 0.35
    b.inputs["Sheen Tint"].default_value = srgb(sheen)
    vo = nt.nodes.new("ShaderNodeTexVoronoi")
    vo.inputs["Scale"].default_value = 7.0
    nt.links.new(coord, vo.inputs["Vector"])
    bp0 = nt.nodes.new("ShaderNodeBump")
    bp0.inputs["Strength"].default_value = 0.5
    bp0.inputs["Distance"].default_value = 0.05
    nt.links.new(vo.outputs["Distance"], bp0.inputs["Height"])
    bp = nt.nodes.new("ShaderNodeBump")
    bp.inputs["Strength"].default_value = 0.35
    h = math_node(nt, "ADD", math_node(nt, "MULTIPLY", folds01, vb=0.6), math_node(nt, "MULTIPLY", weave.outputs["Fac"], vb=0.25))
    nt.links.new(h, bp.inputs["Height"])
    nt.links.new(bp0.outputs["Normal"], bp.inputs["Normal"])
    nt.links.new(bp.outputs["Normal"], b.inputs["Normal"])
    if not hem:
        return b.outputs[0]
    z = sep.outputs["Z"]
    lat = math_node(nt, "MULTIPLY",
                    math_node(nt, "SINE", math_node(nt, "ADD", math_node(nt, "MULTIPLY", ang, vb=14.0), math_node(nt, "MULTIPLY", z, vb=55.0))),
                    math_node(nt, "SINE", math_node(nt, "SUBTRACT", math_node(nt, "MULTIPLY", ang, vb=14.0), math_node(nt, "MULTIPLY", z, vb=55.0))))
    gold_l = math_node(nt, "GREATER_THAN", math_node(nt, "ABSOLUTE", lat), vb=0.72)
    edge = math_node(nt, "MAXIMUM",
                     math_node(nt, "LESS_THAN", math_node(nt, "ABSOLUTE", math_node(nt, "SUBTRACT", z, vb=0.08)), vb=0.012),
                     math_node(nt, "LESS_THAN", math_node(nt, "ABSOLUTE", math_node(nt, "SUBTRACT", z, vb=0.27)), vb=0.012))
    gold = math_node(nt, "MAXIMUM", gold_l, edge)
    t = nt.nodes.new("ShaderNodeBsdfPrincipled")
    mc = mix_rgb(nt, "MIX", va=srgb("#5A0012"), vb=srgb("#E0A83A"), fac=gold)
    nt.links.new(mc, t.inputs["Base Color"])
    nt.links.new(gold, t.inputs["Metallic"])
    t.inputs["Roughness"].default_value = 0.35
    nt.links.new(mc, t.inputs["Emission Color"])
    t.inputs["Emission Strength"].default_value = 0.06
    band = math_node(nt, "MULTIPLY", math_node(nt, "GREATER_THAN", z, vb=0.07), math_node(nt, "LESS_THAN", z, vb=0.28))
    ms = nt.nodes.new("ShaderNodeMixShader")
    nt.links.new(band, ms.inputs["Fac"])
    nt.links.new(b.outputs[0], ms.inputs[1])
    nt.links.new(t.outputs[0], ms.inputs[2])
    return ms.outputs[0]


def robe_mat(name, hem, **kw):
    m_, nt, out = mat(name)
    tc = nt.nodes.new("ShaderNodeTexCoord")
    nt.links.new(cloth_bsdf(nt, tc.outputs["Object"], hem, **kw), out.inputs["Surface"])
    return m_


def gold_mat(name="Gold", hexc="#E0A83A", rough=0.3):
    m, nt, out = mat(name)
    tc = nt.nodes.new("ShaderNodeTexCoord")
    n = noise_tex(nt, 9.0, 6, 0.8, 0.6, tc.outputs["Object"])
    b = nt.nodes.new("ShaderNodeBsdfPrincipled")
    b.inputs["Base Color"].default_value = srgb(hexc)
    b.inputs["Metallic"].default_value = 1.0
    rr = math_node(nt, "ADD", math_node(nt, "MULTIPLY", n.outputs["Fac"], vb=0.3), vb=rough - 0.15)
    nt.links.new(rr, b.inputs["Roughness"])
    bp = nt.nodes.new("ShaderNodeBump")
    bp.inputs["Strength"].default_value = 0.3
    nt.links.new(n.outputs["Fac"], bp.inputs["Height"])
    nt.links.new(bp.outputs["Normal"], b.inputs["Normal"])
    nt.links.new(b.outputs[0], out.inputs["Surface"])
    return m


def tablet_mat(name="Tablet"):
    m, nt, out = mat(name)
    tc = nt.nodes.new("ShaderNodeTexCoord")
    n = noise_tex(nt, 18.0, 8, 0.5, 0.7, tc.outputs["Object"])
    r = ramp(nt, [(0.0, "#020107"), (0.5, "#0B0720"), (0.75, "#1C1250"), (1.0, "#3B2A90")])
    nt.links.new(n.outputs["Fac"], r.inputs["Fac"])
    b = nt.nodes.new("ShaderNodeBsdfPrincipled")
    nt.links.new(r.outputs["Color"], b.inputs["Base Color"])
    b.inputs["Roughness"].default_value = 0.4
    bp = nt.nodes.new("ShaderNodeBump")
    bp.inputs["Strength"].default_value = 0.35
    nt.links.new(n.outputs["Fac"], bp.inputs["Height"])
    nt.links.new(bp.outputs["Normal"], b.inputs["Normal"])
    nt.links.new(b.outputs[0], out.inputs["Surface"])
    return m


def star_mat(name, hexc, strength, aov=True):
    """Emission that also writes itself into the 'stars' AOV (for the
    crossed streaks in post). Twinkles per object over time."""
    m, nt, out = mat(name)
    t = time_node(nt)
    oi = nt.nodes.new("ShaderNodeObjectInfo")
    geo = nt.nodes.new("ShaderNodeNewGeometry")
    # per-star random from position so one mesh can hold many stars
    hpos = nt.nodes.new("ShaderNodeTexWhiteNoise")
    hpos.noise_dimensions = "3D"
    vr = nt.nodes.new("ShaderNodeVectorMath")
    vr.operation = "SNAP"
    vr.inputs[1].default_value = (2.0, 2.0, 2.0)
    nt.links.new(geo.outputs["Position"], vr.inputs[0])
    nt.links.new(vr.outputs[0], hpos.inputs["Vector"])
    w = math_node(nt, "ADD", math_node(nt, "MULTIPLY", t, vb=3.0), math_node(nt, "MULTIPLY", hpos.outputs["Value"], vb=50.0))
    n = noise_tex(nt, 1.0, 1, 0.0, 0.5, dims="1D", w=w)
    k = math_node(nt, "ADD", math_node(nt, "MULTIPLY", n.outputs["Fac"], vb=1.3), vb=0.35)
    s = math_node(nt, "MULTIPLY", k, vb=strength)
    e = nt.nodes.new("ShaderNodeEmission")
    e.inputs["Color"].default_value = srgb(hexc)
    nt.links.new(s, e.inputs["Strength"])
    nt.links.new(e.outputs[0], out.inputs["Surface"])
    if aov:
        ao = nt.nodes.new("ShaderNodeOutputAOV")
        ao.aov_name = "stars"
        sv = nt.nodes.new("ShaderNodeCombineColor")
        for i in range(3):
            nt.links.new(s, sv.inputs[i])
        nt.links.new(mix_rgb(nt, "MULTIPLY", va=srgb(hexc), b=sv.outputs[0]), ao.inputs["Color"])
    return m


# ---------------------------------------------------------------- geometry helpers
def link(ob):
    sc.collection.objects.link(ob)
    return ob


def mesh_object(name, bm, material=None, smooth=True):
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    if smooth:
        me.shade_smooth()
    ob = bpy.data.objects.new(name, me)
    if material:
        me.materials.append(material)
    return link(ob)


def fbm(x, y, z=0.0, oct=5):
    return noise.fractal(Vector((x, y, z)), 0.5, 2.0, oct, noise_basis="PERLIN_NEW")


def ss(a, b, t):
    t = min(1.0, max(0.0, (t - a) / (b - a)))
    return t * t * (3 - 2 * t)


def look_rot(direction, up=Vector((0, 0, 1))):
    """Rotation that turns local -Y toward `direction` with local +Z up."""
    f = direction.normalized()
    r = up.cross(f).normalized()
    u = f.cross(r).normalized()
    m = Matrix((r, -f, u)).transposed()
    return m.to_quaternion()


def crumple_verts(bm, amp, scale=1.0, seed=0.0, mask=None):
    """Push every vertex along its normal by turbulence + cells: the crushed-
    foil look. `mask(v) -> 0..1` scales it per vertex."""
    bm.normal_update()
    o = Vector((seed * 31.7, seed * 17.3, seed * 7.1))
    moves = []
    for v in bm.verts:
        p = v.co * scale + o
        d = amp * (0.9 * fbm(p.x * 0.6, p.y * 0.6, p.z * 0.6, 4) + 0.7 * (noise.turbulence(p, 3, True) - 0.4)
                   + 0.4 * (noise.cell(p * 1.7) - 0.5))
        if mask:
            d *= mask(v)
        moves.append((v, v.normal * d))
    for v, mv in moves:
        v.co += mv
    bm.normal_update()


def revolve(profile, segs, fold=0.0, fold_n=9, seed=0.0, crumple=0.0, rag=0.0, bend=0.0,
            step=0.035, keep_front=None, cap=True, squash=0.82):
    """A body of revolution from (radius, z) points with folds, crumpling,
    a torn hem lifted by up to `rag` and a tip bent by `bend`."""
    pts = [profile[0]]
    for (r0, z0), (r1, z1) in zip(profile, profile[1:]):
        n = max(1, int(math.hypot(r1 - r0, z1 - z0) / step))
        for k in range(1, n + 1):
            t = k / n
            pts.append((r0 + (r1 - r0) * t, z0 + (z1 - z0) * t))
    zmin = profile[0][1]
    ztop = max(z for _, z in profile)
    bm = bmesh.new()
    rings = []
    o = Vector((seed * 13.1, seed * 7.7, seed * 3.3))
    for (r, z) in pts:
        ring = []
        for s_ in range(segs):
            a = 2 * math.pi * s_ / segs
            rr = r * (1 + fold * (math.sin(fold_n * a + seed) * 0.6 + fbm(a * 2, z * 3, seed) * 0.8) * min(1, (z - zmin) / 1.2 + 0.1))
            q = Vector((math.cos(a) * 1.6, math.sin(a) * 1.6, z * 2.6)) + o
            c_ = crumple * ((noise.turbulence(q, 3, True) - 0.45) + 0.6 * (noise.cell(q * 1.7) - 0.5))
            if keep_front and keep_front[0] < z < keep_front[1]:
                front = max(0.0, -math.sin(a))
                c_ *= 1 - 0.9 * front ** 2
            rr *= 1 + c_
            zz = z
            if rag and z - zmin < 0.32:
                cut = rag * (0.55 * max(0.0, fbm(a * 2.2, seed, 0.5) + 0.25) + 0.45 * abs(math.sin(a * 6 + seed * 2)) ** 3)
                w = 1 - (z - zmin) / 0.32
                zz = z + cut * w
                rr *= 1 + 0.06 * w
            x, y = rr * math.cos(a), rr * math.sin(a) * squash
            if bend and z > ztop - 0.25:
                k_ = (z - (ztop - 0.25)) / 0.25
                x += bend * 0.35 * k_ * k_
                y += bend * 0.6 * k_ * k_
            ring.append(bm.verts.new((x, y, zz)))
        rings.append(ring)
    for a_, b_ in zip(rings, rings[1:]):
        for s_ in range(segs):
            t = (s_ + 1) % segs
            bm.faces.new((a_[s_], a_[t], b_[t], b_[s_]))
    if cap:
        bm.faces.new(tuple(reversed(rings[0])))
    return bm


def tube(p0, p1, r0, r1, segs=14, crumple=0.0, seed=0.0, rings_n=3, bm=None):
    bm = bm or bmesh.new()
    axis = (p1 - p0)
    rot = axis.to_track_quat("Z", "Y")
    rings = []
    if rings_n == 3:
        steps = ((0.0, r0), (0.55, (r0 + r1) / 2 * 1.05), (1.0, r1))
    else:
        steps = [(k / (rings_n - 1), r0 + (r1 - r0) * (k / (rings_n - 1)) ** 1.6) for k in range(rings_n)]
    for t, r in steps:
        ring = []
        c = p0 + axis * t
        for s_ in range(segs):
            a = 2 * math.pi * s_ / segs
            rr = r
            if crumple:
                q = Vector((math.cos(a) * 1.5, math.sin(a) * 1.5, t * 3.0 + seed))
                rr *= 1 + crumple * ((noise.turbulence(q, 3, True) - 0.45) + 0.6 * (noise.cell(q * 1.7) - 0.5))
            ring.append(bm.verts.new(c + rot @ Vector((rr * math.cos(a), rr * math.sin(a), 0))))
        rings.append(ring)
    for a_, b_ in zip(rings, rings[1:]):
        for s_ in range(segs):
            t = (s_ + 1) % segs
            bm.faces.new((a_[s_], a_[t], b_[t], b_[s_]))
    bm.faces.new(tuple(rings[-1]))
    return bm


def sweep(points, radii, segs=16, crumple=0.0, seed=0.0, bm=None, twist=0.0):
    """A tube along a polyline with per-point radius, crumpled: roots,
    tentacles, cage bars."""
    bm = bm or bmesh.new()
    rings = []
    n = len(points)
    up = Vector((0, 0, 1))
    for i, p in enumerate(points):
        tng = (points[min(i + 1, n - 1)] - points[max(i - 1, 0)]).normalized()
        ref = up if abs(tng.dot(up)) < 0.95 else Vector((1, 0, 0))
        side = tng.cross(ref).normalized()
        nrm = side.cross(tng).normalized()
        ring = []
        for s_ in range(segs):
            a = 2 * math.pi * s_ / segs + twist * i
            rr = radii[i]
            if crumple:
                q = Vector((math.cos(a) * 1.4, math.sin(a) * 1.4, i * 0.37 + seed))
                rr *= 1 + crumple * ((noise.turbulence(q, 3, True) - 0.45) + 0.6 * (noise.cell(q * 1.7) - 0.5))
            ring.append(bm.verts.new(p + (side * math.cos(a) + nrm * math.sin(a)) * rr))
        rings.append(ring)
    for a_, b_ in zip(rings, rings[1:]):
        for s_ in range(segs):
            t = (s_ + 1) % segs
            bm.faces.new((a_[s_], a_[t], b_[t], b_[s_]))
    bm.faces.new(tuple(reversed(rings[0])))
    bm.faces.new(tuple(rings[-1]))
    return bm


def crag(name, cx, cy, z0, half, height, material, sub=7, amp=1.0, lean=1.0, seed=0.0, keep=(), shards=40):
    """A torn, crumpled crystal pyramid with shards breaking out of it."""
    bm = bmesh.new()
    c = [bm.verts.new((cx + dx, cy + dy, z0)) for dx, dy in ((-half, -half), (half, -half), (half, half), (-half, half))]
    apex = bm.verts.new((cx, cy, z0 + height))
    for a, b_ in ((0, 1), (1, 2), (2, 3), (3, 0)):
        bm.faces.new((c[a], c[b_], apex))
    bm.faces.new(tuple(reversed(c)))
    bmesh.ops.subdivide_edges(bm, edges=bm.edges[:], cuts=2 ** sub - 1, use_grid_fill=True)
    bm.normal_update()
    o = Vector((seed * 31.7, seed * 17.3, seed * 7.1))
    moves = []
    for v in bm.verts:
        if v.co.z <= z0 + 1e-4:
            continue
        p = v.co + o
        t = (v.co.z - z0) / height
        big = fbm(p.x * 0.09, p.y * 0.09, p.z * 0.09, 4)
        cr_ = noise.turbulence(p * 0.15, 3, True)
        cell_ = noise.cell(p * 0.22) - 0.5
        strata = ((v.co.z * 0.55 + 2.5 * big) % 1.0) - 0.5
        d = amp * (1.3 * big + 0.8 * (cr_ - 0.4) + 0.45 * cell_ + 0.3 * strata) * (0.3 + 0.7 * (1 - t))
        for (e, r) in keep:
            d *= 0.12 + 0.88 * ss(r * 0.6, r * 1.15, (v.co - e).length)
        shift = Vector((lean * math.sin(t * math.pi) * (1.0 + 0.6 * big), 0, 0))
        moves.append((v, v.normal * d + shift))
    for v, mv in moves:
        v.co += mv
    bm.normal_update()
    faces = [f for f in bm.faces if f.calc_center_median().z > z0 + 0.6 and len(f.verts) >= 3]
    srnd_ = random.Random(int(seed * 1000) + 5)
    made = tries_ = 0
    sh = bmesh.new()
    while made < shards and tries_ < shards * 30:
        tries_ += 1
        f = srnd_.choice(faces)
        cpos = f.calc_center_median()
        if any((cpos - e).length < r * 1.3 for e, r in keep):
            continue
        t = (cpos.z - z0) / height
        if t > 0.85:
            continue
        axis = (f.normal * 1.0 + Vector((0, 0, 0.7)) + Vector((srnd_.uniform(-.4, .4), srnd_.uniform(-.4, .4), 0))).normalized()
        length = srnd_.uniform(1.5, 6.0) * (1.1 - t) * (half / 12.0) ** 0.5
        rad = length * srnd_.uniform(0.14, 0.24)
        g = bmesh.ops.create_cone(sh, cap_ends=True, segments=srnd_.choice((4, 5, 6)), radius1=rad, radius2=0.0, depth=length)
        rot = axis.to_track_quat("Z", "Y").to_matrix().to_4x4() @ Matrix.Rotation(srnd_.uniform(0, 6.28), 4, "Z")
        mtx = Matrix.Translation(cpos - f.normal * 0.2 + axis * (length * 0.5 - 0.3)) @ rot
        bmesh.ops.transform(sh, matrix=mtx, verts=g["verts"])
        made += 1
    ob = mesh_object(name, bm, material, smooth=True)
    mesh_object(name + " shards", sh, material, smooth=False)
    return ob


def grid_terrain(name, Hf, x0, x1, y0, y1, nx, ny, material):
    bm = bmesh.new()
    verts = []
    for j in range(ny + 1):
        row = []
        for i in range(nx + 1):
            x = x0 + (x1 - x0) * i / nx
            y = y0 + (y1 - y0) * j / ny
            row.append(bm.verts.new((x, y, Hf(x, y))))
        verts.append(row)
    for j in range(ny):
        for i in range(nx):
            bm.faces.new((verts[j][i], verts[j][i + 1], verts[j + 1][i + 1], verts[j + 1][i]))
    return mesh_object(name, bm, material)


# ---------------------------------------------------------------- logos
def _grad_emission(name, stops, axis, lo, hi, strength):
    """Emission with a linear gradient along local `axis` (0=x, 1=y) from lo to hi."""
    m, nt, out = mat(name)
    tc = nt.nodes.new("ShaderNodeTexCoord")
    sep = nt.nodes.new("ShaderNodeSeparateXYZ")
    nt.links.new(tc.outputs["Object"], sep.inputs[0])
    mr = nt.nodes.new("ShaderNodeMapRange")
    mr.inputs["From Min"].default_value = lo
    mr.inputs["From Max"].default_value = hi
    nt.links.new(sep.outputs[axis], mr.inputs["Value"])
    r = ramp(nt, stops)
    nt.links.new(mr.outputs["Result"], r.inputs["Fac"])
    e = nt.nodes.new("ShaderNodeEmission")
    e.inputs["Strength"].default_value = strength
    nt.links.new(r.outputs["Color"], e.inputs["Color"])
    nt.links.new(e.outputs[0], out.inputs["Surface"])
    return m


# gradients the island draws (video/src/AgentMarks.tsx); key = placeholder fill
LOGO_GRADS = {
    "codex.svg": {"#7A9DFF": ([(0.0, "#3941FF"), (0.5, "#7A9DFF"), (1.0, "#B1A7FF")], 1)},   # bottom -> top
    "gemini.svg": {"#B381DD": ([(0.0, "#207CFE"), (0.52, "#207CFE"), (0.69, "#B381DD"), (1.0, "#EE4D5D")], 0)},
}


def import_logo(fname, size, strength, thickness=0.012):
    """The SVG as Blender imports it, scaled to `size` on its longest side,
    centred, in XY with +Z as its front. Fills become emissions."""
    before = set(bpy.data.objects)
    bpy.ops.import_curve.svg(filepath=os.path.join(LOGOS, fname))
    new = [o for o in bpy.data.objects if o not in before]
    for o in new:
        for c in o.users_collection:
            c.objects.unlink(o)
        sc.collection.objects.link(o)
    parts = []
    for o in new:
        col = o.data.materials[0].diffuse_color[:] if o.data.materials else (1, 1, 1, 1)
        me = bpy.data.meshes.new_from_object(o)
        bm = bmesh.new()
        bm.from_mesh(me)
        bm.transform(o.matrix_world)
        parts.append((bm, col))
        bpy.data.objects.remove(o)
    lo = Vector((1e9, 1e9, 0))
    hi = Vector((-1e9, -1e9, 0))
    for bm_, _ in parts:
        for v in bm_.verts:
            lo.x, lo.y = min(lo.x, v.co.x), min(lo.y, v.co.y)
            hi.x, hi.y = max(hi.x, v.co.x), max(hi.y, v.co.y)
    centre = (lo + hi) / 2
    k = size / max(hi.x - lo.x, hi.y - lo.y)
    out = bmesh.new()
    mats = []
    for bm_, col in parts:
        key = tuple(round(c, 4) for c in col[:3])
        if key not in mats:
            mats.append(key)
        idx = mats.index(key)
        vmap = {}
        dz = size * 0.004 * parts.index((bm_, col))      # later fills sit in front (no z-fighting)
        for v in bm_.verts:
            vmap[v] = out.verts.new(((v.co.x - centre.x) * k, (v.co.y - centre.y) * k, dz))
        for f in bm_.faces:
            nf = out.faces.new([vmap[v] for v in f.verts])
            nf.material_index = idx
        bm_.free()
    ob = mesh_object(fname.split(".")[0], out, smooth=False)
    w = (hi.x - lo.x) * k
    h = (hi.y - lo.y) * k
    grads = LOGO_GRADS.get(fname, {})
    for key in mats:
        gr = None
        for hx, g in grads.items():
            if all(abs(a - b) < 0.02 for a, b in zip(srgb(hx)[:3], key)):
                gr = g
        if gr:
            stops, axis = gr
            ext = (w if axis == 0 else h) / 2
            ob.data.materials.append(_grad_emission("Logo grad " + fname, stops, axis, -ext, ext, strength))
        else:
            ob.data.materials.append(emission("Logo " + str(key), key + (1.0,), strength))
    for p in ob.data.polygons:
        if p.normal.z < 0:
            p.flip()
    sol = ob.modifiers.new("depth", "SOLIDIFY")
    sol.thickness = size * thickness
    sol.offset = 1.0
    return ob, w, h, mats


# ---------------------------------------------------------------- candles, torches
_M = {}


def _m(key, fn):
    if key not in _M:
        _M[key] = fn()
    return _M[key]


def candle(x, y, z, s=1.0, light=True, rng=rnd):
    """A crooked red wax candle with a flickering flame and its own small
    red light."""
    wax = _m("wax", lambda: flicker_emission("Wax", srgb("#7A0008"), 0.7, speed=0.6, depth=0.25))
    flame = _m("flame", lambda: flicker_emission("Flame", srgb("#FF4A18"), 11.0, speed=1.6, depth=0.45))
    hgt = rng.uniform(0.10, 0.28) * s
    bm = bmesh.new()
    bmesh.ops.create_cone(bm, cap_ends=True, segments=10, radius1=0.042 * s, radius2=0.034 * s, depth=hgt)
    lean = Matrix.Rotation(rng.uniform(-0.12, 0.12), 4, "X") @ Matrix.Rotation(rng.uniform(-0.12, 0.12), 4, "Y")
    bmesh.ops.transform(bm, matrix=lean, verts=bm.verts)
    crumple_verts(bm, 0.006 * s, 18.0, rng.random() * 10)
    bmesh.ops.translate(bm, vec=(x, y, z + hgt / 2 - 0.02), verts=bm.verts)
    mesh_object("candle", bm, wax)
    top = Vector((x, y, z + hgt)) + (lean @ Vector((0, 0, hgt / 2))) - Vector((0, 0, hgt / 2))
    bm = bmesh.new()
    bmesh.ops.create_uvsphere(bm, u_segments=8, v_segments=6, radius=0.024 * s)
    bmesh.ops.scale(bm, vec=(1, 1, 2.3), verts=bm.verts)
    bmesh.ops.translate(bm, vec=top + Vector((0, 0, 0.035 * s)), verts=bm.verts)
    mesh_object("flame", bm, flame, smooth=True)
    if light:
        ld = bpy.data.lights.new("candle", "POINT")
        ld.color = srgb("#FF3A1A")[:3]
        ld.energy = 2.5 * s
        ld.shadow_soft_size = 0.03
        lo = link(bpy.data.objects.new("candle", ld))
        lo.location = top + Vector((0, 0, 0.09 * s))


def torch(x, y, z, hgt=2.4, energy=140):
    pole = _m("pole", lambda: emission("Pole", srgb("#0A0410"), 0.0))
    fire = _m("torchfire", lambda: flicker_emission("TorchFire", srgb("#FF3A12"), 6.0, speed=1.4, depth=0.35))
    bm = bmesh.new()
    bmesh.ops.create_cone(bm, cap_ends=True, segments=8, radius1=0.05, radius2=0.035, depth=hgt)
    crumple_verts(bm, 0.01, 6.0, x)
    bmesh.ops.translate(bm, vec=(x, y, z + hgt / 2), verts=bm.verts)
    mesh_object("pole", bm, pole)
    bm = bmesh.new()
    bmesh.ops.create_uvsphere(bm, u_segments=12, v_segments=10, radius=0.11)
    bmesh.ops.scale(bm, vec=(1, 1, 1.9), verts=bm.verts)
    crumple_verts(bm, 0.03, 8.0, y)
    bmesh.ops.translate(bm, vec=(x, y, z + hgt + 0.16), verts=bm.verts)
    mesh_object("fire", bm, fire)
    ld = bpy.data.lights.new("torch", "POINT")
    ld.color = srgb("#FF5A1E")[:3]
    ld.energy = energy
    ld.shadow_soft_size = 0.12
    lo = link(bpy.data.objects.new("torch", ld))
    lo.location = (x, y, z + hgt + 0.25)


# ---------------------------------------------------------------- lights
def area(name, loc, look_at, size, energy, color):
    ld = bpy.data.lights.new(name, "AREA")
    ld.size = size
    ld.energy = energy
    ld.color = color
    lo = link(bpy.data.objects.new(name, ld))
    lo.location = loc
    lo.rotation_mode = "QUATERNION"
    lo.rotation_quaternion = (Vector(look_at) - Vector(loc)).to_track_quat("-Z", "Y")
    return lo


def point(name, loc, energy, color, soft=0.3):
    ld = bpy.data.lights.new(name, "POINT")
    ld.energy = energy
    ld.color = color
    ld.shadow_soft_size = soft
    lo = link(bpy.data.objects.new(name, ld))
    lo.location = loc
    return lo


# ---------------------------------------------------------------- camera
def camera(name="Cam", lens=24.0):
    cd = bpy.data.cameras.new(name)
    cd.lens = lens
    cd.sensor_fit = "VERTICAL"
    cd.sensor_height = 36.0          # 36 mm tall gate: lens 24 = 74 deg vertical
    cd.clip_end = 2000
    cam = link(bpy.data.objects.new(name, cd))
    cam.rotation_mode = "QUATERNION"
    sc.camera = cam
    return cam


def key_camera(cam, keys):
    """keys = [(frame, location, look_at, roll_deg)], smooth in-between."""
    for f, loc, at, roll in keys:
        cam.location = Vector(loc)
        q = (Vector(at) - Vector(loc)).to_track_quat("-Z", "Y")
        q = q @ Matrix.Rotation(math.radians(roll), 4, "Z").to_quaternion()
        cam.rotation_quaternion = q
        cam.keyframe_insert("location", frame=f)
        cam.keyframe_insert("rotation_quaternion", frame=f)
    smooth_keys(cam)


def sway(cam, n_frames, amp_loc=0.02, amp_rot=0.004, seed=1.0):
    """Handheld-ish drift: small noise added per frame on top of the keys
    (baked as keys on a delta transform)."""
    cam.rotation_mode = "QUATERNION"
    for f in range(1, n_frames + 1):
        t = f / FPS
        dx = amp_loc * fbm(t * 0.6, seed, 0.0, 2)
        dz = amp_loc * fbm(seed, t * 0.6, 1.0, 2)
        cam.delta_location = (dx, 0, dz)
        cam.keyframe_insert("delta_location", frame=f)
        rx = amp_rot * fbm(t * 0.5, seed + 3, 2.0, 2)
        rz = amp_rot * fbm(seed + 5, t * 0.5, 3.0, 2)
        cam.delta_rotation_euler = (rx, 0, rz)
        cam.keyframe_insert("delta_rotation_euler", frame=f)


# ---------------------------------------------------------------- sky
def star_field(cam_pos, fwd, n=1500, R=180.0, bright_p=0.05, cone=0.55, seed=11, avoid=(), big=(0.18, 0.5),
               bright_hex="#F6E6FF", dim_hex="#D8CCFF", bright_strength=40.0, dim_strength=2.5, up_min=-0.1, sep=4.0):
    srnd = random.Random(seed)
    bright = bmesh.new()
    dim = bmesh.new()
    bright_dirs = []
    for _ in range(n):
        d = Vector((srnd.uniform(-1, 1), srnd.uniform(-1, 1), srnd.uniform(up_min, 1)))
        if d.length < 1e-3:
            continue
        d.normalize()
        if d.dot(fwd) < cone or any(d.angle(a) < r for a, r in avoid):
            continue
        p = Vector(cam_pos) + d * R
        if srnd.random() < bright_p:
            if any(d.angle(b_) < math.radians(sep) for b_ in bright_dirs):
                continue
            bright_dirs.append(d)
            g = bmesh.ops.create_icosphere(bright, subdivisions=1, radius=srnd.uniform(*big) * R / 180)
            bmesh.ops.translate(bright, vec=p, verts=g["verts"])
        else:
            g = bmesh.ops.create_icosphere(dim, subdivisions=0, radius=srnd.uniform(0.04, 0.10) * R / 180)
            bmesh.ops.translate(dim, vec=p, verts=g["verts"])
    sb = mesh_object("StarsBright", bright, star_mat("Star", bright_hex, bright_strength), smooth=False)
    sd = mesh_object("StarsDim", dim, star_mat("StarDim", dim_hex, dim_strength, aov=False), smooth=False)
    for s_ in (sb, sd):
        s_.visible_shadow = False
        s_.visible_diffuse = s_.visible_glossy = False
    return sb, sd


def nebula(loc, rot_deg, size, stops=None, strength=2.4, scale=(0.006, 0.0035, 1.0), mask=(0.36, 0.56), drift=0.0):
    m, nt, out = mat("Nebula")
    tc = nt.nodes.new("ShaderNodeTexCoord")
    mp = nt.nodes.new("ShaderNodeMapping")
    mp.inputs["Scale"].default_value = scale
    nt.links.new(tc.outputs["Object"], mp.inputs["Vector"])
    t = time_node(nt)
    n1 = noise_tex(nt, 1.0, 12, 3.5, 0.66, mp.outputs[0], dims="4D", w=math_node(nt, "MULTIPLY", t, vb=drift))
    r1 = ramp(nt, stops or [(0.0, "#000000"), (0.44, "#000000"), (0.54, "#2A0004"), (0.64, "#9A000C"),
                            (0.76, "#FF1E14"), (0.86, "#FF8060"), (1.0, "#FFB0A0")])
    nt.links.new(n1.outputs["Fac"], r1.inputs["Fac"])
    mk = noise_tex(nt, 0.45, 2, 0.0, 0.5, mp.outputs[0])
    mr = ramp(nt, [(0.0, "#000000"), (mask[0], "#000000"), (mask[1], "#FFFFFF"), (1.0, "#FFFFFF")])
    nt.links.new(mk.outputs["Fac"], mr.inputs["Fac"])
    c = mix_rgb(nt, "MULTIPLY", r1.outputs["Color"], mr.outputs["Color"])
    e = nt.nodes.new("ShaderNodeEmission")
    e.inputs["Strength"].default_value = strength
    nt.links.new(c, e.inputs["Color"])
    nt.links.new(e.outputs[0], out.inputs["Surface"])
    bm = bmesh.new()
    bmesh.ops.create_grid(bm, x_segments=2, y_segments=2, size=1.0)
    xs = [v.co.x for v in bm.verts]
    half = (max(xs) - min(xs)) / 2
    bmesh.ops.scale(bm, vec=(size[0] / 2 / half, size[1] / 2 / half, 1), verts=bm.verts)
    ob = mesh_object("Nebula", bm, m, smooth=False)
    ob.location = loc
    ob.rotation_euler = tuple(math.radians(a) for a in rot_deg)
    ob.visible_shadow = False
    ob.visible_diffuse = False
    return ob


# ---------------------------------------------------------------- render
def render(outdir, start, end, step=1):
    """Frames start..end to OUTDIR/f_####.exr (multilayer: Combined + stars AOV)."""
    os.makedirs(outdir, exist_ok=True)
    sc.render.image_settings.media_type = "MULTI_LAYER_IMAGE"
    sc.render.image_settings.file_format = "OPEN_EXR_MULTILAYER"
    sc.render.image_settings.color_depth = "16"
    sc.render.image_settings.exr_codec = "DWAA" if os.environ.get("DWAA") else "ZIP"
    sc.render.use_overwrite = os.environ.get("OVERWRITE", "0") != "0"
    sc.render.use_placeholder = True
    sc.render.filepath = os.path.join(outdir, "f_####")
    sc.frame_start, sc.frame_end, sc.frame_step = start, end, step
    bpy.ops.render.render(animation=True)
    print("DONE", outdir, start, end)


def main(build, n_frames):
    """Common CLI: blender -b -P shot.py -- OUTDIR SPP PCT START END [STEP]"""
    a = args()
    outdir, spp, pct = a[0], int(a[1]), int(a[2])
    start = int(a[3]) if len(a) > 3 else 1
    end = int(a[4]) if len(a) > 4 else n_frames
    step = int(a[5]) if len(a) > 5 else 1
    setup(spp, pct, n_frames)
    build()
    if os.environ.get("SAVE"):
        bpy.ops.wm.save_as_mainfile(filepath=os.environ["SAVE"])
    render(outdir, start, end, step)


# ---------------------------------------------------------------- torn stone tablets (no frames)
def shard_tablet(w, h, depth, seed=0.0, jag=0.12, chips=7, segs=44):
    """A broken slab of stone, centred, face toward -Y: a rough outline (chipped
    corners, bites out of the edges), extruded and crumpled. Returns a bmesh."""
    r = random.Random(int(seed * 977) + 3)
    pts = []
    bites = [(r.uniform(0, 2 * math.pi), r.uniform(0.15, 0.35), r.uniform(0.25, 0.6)) for _ in range(chips)]
    for k in range(segs):
        a = 2 * math.pi * k / segs
        # superellipse: squarish, but never with straight sides
        c, s_ = math.cos(a), math.sin(a)
        e = 3.2
        rr = (abs(c) ** e + abs(s_) ** e) ** (-1 / e)
        rr *= 1 + jag * (fbm(math.cos(a) * 1.8 + seed, math.sin(a) * 1.8, seed, 3)) + jag * 0.5 * (r.random() - 0.5)
        for (ba, bw, bd) in bites:
            d = math.atan2(math.sin(a - ba), math.cos(a - ba))
            if abs(d) < bw:
                rr *= 1 - bd * jag * 2.2 * (1 - abs(d) / bw)
        pts.append((c * rr * w / 2, s_ * rr * h / 2))
    bm = bmesh.new()
    face = bm.faces.new([bm.verts.new((x, 0.0, z)) for x, z in pts])
    ext = bmesh.ops.extrude_face_region(bm, geom=[face])
    bmesh.ops.translate(bm, vec=(0, depth, 0), verts=[g for g in ext["geom"] if isinstance(g, bmesh.types.BMVert)])
    bmesh.ops.triangulate(bm, faces=bm.faces[:])
    bmesh.ops.subdivide_edges(bm, edges=bm.edges[:], cuts=3)
    bmesh.ops.translate(bm, vec=(0, -depth / 2, 0), verts=bm.verts)
    crumple_verts(bm, depth * 0.35, 6.0 / max(w, h), seed)
    return bm


def stone_mat(name, stops, crack_hex="#000000", scale=1.0, glow=0.03):
    """Rough stone: marble colour, dark cracks (voronoi edges), worn edges
    lighter (pointiness), strong bump."""
    m, nt, out = mat(name)
    tc = nt.nodes.new("ShaderNodeTexCoord")
    co = tc.outputs["Object"]
    n = noise_tex(nt, 1.4 * scale, 10, 3.0, 0.65, co)
    r = ramp(nt, stops)
    nt.links.new(n.outputs["Fac"], r.inputs["Fac"])
    geo = nt.nodes.new("ShaderNodeNewGeometry")
    pr = ramp(nt, [(0.0, "#2A2A2A"), (0.46, "#6A6A6A"), (0.53, "#FFFFFF"), (1.0, "#FFFFFF")])
    nt.links.new(geo.outputs["Pointiness"], pr.inputs["Fac"])
    col = mix_rgb(nt, "MULTIPLY", r.outputs["Color"], pr.outputs["Color"])
    vo = nt.nodes.new("ShaderNodeTexVoronoi")
    vo.feature = "DISTANCE_TO_EDGE"
    vo.inputs["Scale"].default_value = 3.5 * scale
    wob = noise_tex(nt, 4.0 * scale, 4, 0.0, 0.5, co)
    nt.links.new(mix_rgb(nt, "ADD", co, wob.outputs["Color"], fac=0.25), vo.inputs["Vector"])
    crack = math_node(nt, "LESS_THAN", vo.outputs["Distance"], vb=0.03)
    col = mix_rgb(nt, "MIX", col, vb=srgb(crack_hex), fac=crack)
    b = nt.nodes.new("ShaderNodeBsdfPrincipled")
    nt.links.new(col, b.inputs["Base Color"])
    nt.links.new(col, b.inputs["Emission Color"])
    b.inputs["Emission Strength"].default_value = glow
    b.inputs["Roughness"].default_value = 0.42
    fine = noise_tex(nt, 18.0 * scale, 8, 0.5, 0.75, co)
    b1 = nt.nodes.new("ShaderNodeBump")
    b1.inputs["Strength"].default_value = 0.8
    nt.links.new(fine.outputs["Fac"], b1.inputs["Height"])
    b2 = nt.nodes.new("ShaderNodeBump")
    b2.inputs["Strength"].default_value = 1.0
    b2.inputs["Distance"].default_value = 0.05
    nt.links.new(math_node(nt, "SUBTRACT", vb=1.0, b=crack), b2.inputs["Height"])
    nt.links.new(b1.outputs["Normal"], b2.inputs["Normal"])
    nt.links.new(b2.outputs["Normal"], b.inputs["Normal"])
    nt.links.new(b.outputs[0], out.inputs["Surface"])
    return m


# grounds that stand apart from the blue castle, root and pyramids
GROUNDS = {
    "plum": [(0.0, "#060006"), (0.4, "#26031E"), (0.5, "#560A3A"), (0.6, "#9A1256"), (0.7, "#D8236E"),
             (0.8, "#FF4E8E"), (0.92, "#FF8AB4")],
    "teal": [(0.0, "#000605"), (0.4, "#01201A"), (0.5, "#03463A"), (0.6, "#067A62"), (0.7, "#0BB088"),
             (0.8, "#2AE0A8"), (0.92, "#7AFFD0")],
}


def ground_stops():
    return GROUNDS[os.environ.get("GROUND", "plum")]


def painted_sky(png, stars_png, loc, size, strength=1.0, star_strength=9.0, star_hex="#F0B0FF", seq=None, seq_len=0):
    """The 'Starry Night' plate (vangogh.py) on a plane facing -Y, its star
    cores written to the 'stars' AOV for the flares."""
    m, nt, out = mat("PaintedSky")
    tc = nt.nodes.new("ShaderNodeTexCoord")
    im = nt.nodes.new("ShaderNodeTexImage")
    if seq:
        # the flowing strokes: one painted frame per film frame (vangogh.py FRAMES)
        im.image = bpy.data.images.load(seq)
        im.image.source = "SEQUENCE"
        im.image_user.frame_duration = seq_len
        im.image_user.frame_start = 1
        im.image_user.frame_offset = 0
        im.image_user.use_auto_refresh = True
    else:
        im.image = bpy.data.images.load(png)
    im.interpolation = "Cubic"
    nt.links.new(tc.outputs["UV"], im.inputs["Vector"])
    e = nt.nodes.new("ShaderNodeEmission")
    nt.links.new(im.outputs["Color"], e.inputs["Color"])
    e.inputs["Strength"].default_value = strength
    nt.links.new(e.outputs[0], out.inputs["Surface"])
    st = nt.nodes.new("ShaderNodeTexImage")
    st.image = bpy.data.images.load(stars_png)
    st.image.colorspace_settings.name = "Non-Color"
    nt.links.new(tc.outputs["UV"], st.inputs["Vector"])
    t = time_node(nt)
    tw = noise_tex(nt, 6.0, 1, 0.0, 0.5, tc.outputs["UV"], dims="4D", w=math_node(nt, "MULTIPLY", t, vb=2.0))
    k = math_node(nt, "ADD", math_node(nt, "MULTIPLY", tw.outputs["Fac"], vb=0.9), vb=0.55)
    ao = nt.nodes.new("ShaderNodeOutputAOV")
    ao.aov_name = "stars"
    c1 = mix_rgb(nt, "MULTIPLY", st.outputs["Color"], vb=tuple(c * star_strength for c in srgb(star_hex)[:3]) + (1,))
    nt.links.new(mix_rgb(nt, "MULTIPLY", c1, k), ao.inputs["Color"])
    bm = bmesh.new()
    uv = bm.loops.layers.uv.new()
    w, h = size
    vs = [bm.verts.new((x, 0, z)) for x, z in ((-w / 2, -h / 2), (w / 2, -h / 2), (w / 2, h / 2), (-w / 2, h / 2))]
    f = bm.faces.new(vs)
    for loop, (a, b) in zip(f.loops, ((0, 0), (1, 0), (1, 1), (0, 1))):
        loop[uv].uv = (a, b)
    ob = mesh_object("PaintedSky", bm, m, smooth=False)
    ob.location = loc
    ob.visible_shadow = False
    ob.visible_diffuse = False
    return ob
