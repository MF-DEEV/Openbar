"""Shared bits for the dark-fantasy README (banner, wordmark, buttons, demo set).

Builds on the kingdom reel's klib (materials, crumple helpers, logos, render to
multilayer EXR). Style after @ozavry's reel Dco5xX1vbj6: an old-game CGI night,
violet cloud sky, a pale grinning moon, moss-green stone armour, warm campfire,
blue waterfalls; everything rough and a little crooked.
"""
import math
import os
import random
import sys

import bmesh
import bpy
from mathutils import Matrix, Vector, noise

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, HERE)
import klib as K  # noqa: E402

FONTS = os.path.join(ROOT, "assets", "fonts")
MARKS = os.path.join(ROOT, "assets", "marks")
FONT = {
    "saira": "saira-latin-900-normal.ttf",
    "fraktur": "unifrakturmaguntia-latin-400-normal.ttf",
    "cinzeldeco": "cinzel-decorative-latin-900-normal.ttf",
    "cinzel": "cinzel-latin-700-normal.ttf",
    "pirata": "pirata-one-latin-400-normal.ttf",
    "rocker": "new-rocker-latin-400-normal.ttf",
    "grenze": "grenze-gotisch-latin-900-normal.ttf",
    "metamorph": "metamorphous-latin-400-normal.ttf",
}
srgb = K.srgb


def setup(w, h, spp, n_frames=1, transparent=False, mblur=False):
    K.W, K.H_RES = w, h
    os.environ["MBLUR"] = "1" if mblur else "0"
    sc = K.setup(spp, 100, n_frames)
    sc.render.film_transparent = transparent
    return sc


def camera(loc, at, lens, roll=0.0, ortho=None):
    cam = K.camera("Cam", lens=lens)
    cam.data.sensor_fit = "HORIZONTAL"
    cam.data.sensor_width = 36.0
    if ortho:
        cam.data.type = "ORTHO"
        cam.data.ortho_scale = ortho
    cam.location = Vector(loc)
    q = (Vector(at) - Vector(loc)).to_track_quat("-Z", "Y")
    cam.rotation_quaternion = q @ Matrix.Rotation(math.radians(roll), 4, "Z").to_quaternion()
    return cam


def still(outdir, frame=1):
    K.render(outdir, frame, frame)


# ---------------------------------------------------------------- text
def text(body, font, size, depth=0.12, bevel=0.02, bevel_res=3, spacing=1.0, scale_x=1.0, offset=0.0):
    """Text as a mesh in the XZ plane, front toward -Y, centred on the origin.
    Returns (object, width, height)."""
    cu = bpy.data.curves.new("T_" + body, "FONT")
    cu.body = body
    cu.font = bpy.data.fonts.load(os.path.join(FONTS, FONT.get(font, font)), check_existing=True)
    cu.size = size
    cu.extrude = depth * size
    cu.bevel_depth = bevel * size
    cu.bevel_resolution = bevel_res
    cu.offset = offset * size
    cu.space_character = spacing
    cu.align_x = "CENTER"
    cu.align_y = "CENTER"
    ob = K.link(bpy.data.objects.new("T_" + body, cu))
    dg = bpy.context.evaluated_depsgraph_get()
    me = bpy.data.meshes.new_from_object(ob.evaluated_get(dg))
    bpy.data.objects.remove(ob)
    bm = bmesh.new()
    bm.from_mesh(me)
    bmesh.ops.scale(bm, vec=(scale_x, 1, 1), verts=bm.verts)
    # XY -> XZ, front (+Z of the curve) toward -Y
    bmesh.ops.rotate(bm, verts=bm.verts, cent=(0, 0, 0), matrix=Matrix.Rotation(math.radians(90), 3, "X"))
    xs = [v.co.x for v in bm.verts]
    zs = [v.co.z for v in bm.verts]
    ys = [v.co.y for v in bm.verts]
    c = Vector(((min(xs) + max(xs)) / 2, max(ys), (min(zs) + max(zs)) / 2))
    bmesh.ops.translate(bm, vec=-c, verts=bm.verts)
    w, h = max(xs) - min(xs), max(zs) - min(zs)
    ob = K.mesh_object("T_" + body, bm, smooth=False)
    ob.data.shade_smooth_by_angle(angle=math.radians(35)) if hasattr(ob.data, "shade_smooth_by_angle") else None
    return ob, w, h


def remesh_rough(ob, voxel, amp, scale, seed=0.0):
    """Voxel-remesh then crumple: hand-cut, chipped letters."""
    r = ob.modifiers.new("vox", "REMESH")
    r.mode = "VOXEL"
    r.voxel_size = voxel
    dg = bpy.context.evaluated_depsgraph_get()
    me = bpy.data.meshes.new_from_object(ob.evaluated_get(dg))
    ob.modifiers.clear()
    old = ob.data
    ob.data = me
    bpy.data.meshes.remove(old)
    bm = bmesh.new()
    bm.from_mesh(me)
    K.crumple_verts(bm, amp, scale, seed)
    bm.to_mesh(me)
    bm.free()
    me.shade_smooth()
    return ob


def helmet(size, metal, thickness=0.18, dent=0.012, voxel=0.006):
    """The Openbar helmet as a battered casting, in XZ facing -Y, centred.
    The SVG's ngon front is a fan of sliver triangles (shading streaks), so
    the solid is rebuilt as voxels, smoothed, then dented."""
    hm, w, h, _ = K.import_logo("openbar-helmet.svg", size, 1.0, thickness=thickness)
    dg = bpy.context.evaluated_depsgraph_get()
    me = bpy.data.meshes.new_from_object(hm.evaluated_get(dg))
    hm.modifiers.clear()
    old = hm.data
    hm.data = me
    bpy.data.meshes.remove(old)
    me.materials.clear()
    me.materials.append(metal)
    vx = hm.modifiers.new("vox", "REMESH")
    vx.mode = "VOXEL"
    vx.voxel_size = size * voxel
    sm = hm.modifiers.new("smooth", "SMOOTH")
    sm.factor = 0.6
    sm.iterations = 3
    if dent:
        dsp = hm.modifiers.new("dent", "DISPLACE")
        tex = bpy.data.textures.new("dent", "CLOUDS")
        tex.noise_scale = size * 0.08
        dsp.texture = tex
        dsp.strength = size * dent
    me.shade_smooth()
    hm.rotation_euler = (math.radians(90), 0, 0)
    return hm, w, h


def place(ob, loc, rot_z=0.0):
    ob.location = Vector(loc)
    ob.rotation_euler.z = math.radians(rot_z)
    return ob


# ---------------------------------------------------------------- materials
def _bump_chain(nt, co, fine_scale, strength, extra=None):
    fine = K.noise_tex(nt, fine_scale, 8, 0.5, 0.75, co)
    b1 = nt.nodes.new("ShaderNodeBump")
    b1.inputs["Strength"].default_value = strength
    nt.links.new(fine.outputs["Fac"], b1.inputs["Height"])
    if extra is None:
        return b1.outputs["Normal"]
    b2 = nt.nodes.new("ShaderNodeBump")
    b2.inputs["Strength"].default_value = 1.0
    b2.inputs["Distance"].default_value = 0.04
    nt.links.new(extra, b2.inputs["Height"])
    nt.links.new(b1.outputs["Normal"], b2.inputs["Normal"])
    return b2.outputs["Normal"]


def lichen_stone(name, scale=1.0, base=("#0C120C", "#26331F", "#4E6440", "#8EA07C"), spots="#D8E4CC",
                 glow=0.0, crack=True):
    """The reel's knights: moss-green stone, pale lichen speckle, dark cracks."""
    m, nt, out = K.mat(name)
    tc = nt.nodes.new("ShaderNodeTexCoord")
    co = tc.outputs["Object"]
    n = K.noise_tex(nt, 2.2 * scale, 10, 2.0, 0.66, co)
    r = K.ramp(nt, [(0.0, base[0]), (0.38, base[1]), (0.55, base[2]), (0.75, base[3])])
    nt.links.new(n.outputs["Fac"], r.inputs["Fac"])
    sp = K.noise_tex(nt, 30.0 * scale, 6, 0.0, 0.8, co)
    spot = K.math_node(nt, "GREATER_THAN", sp.outputs["Fac"], vb=0.62)
    col = K.mix_rgb(nt, "MIX", r.outputs["Color"], vb=srgb(spots), fac=K.math_node(nt, "MULTIPLY", spot, vb=0.85))
    geo = nt.nodes.new("ShaderNodeNewGeometry")
    pr = K.ramp(nt, [(0.0, "#303030"), (0.47, "#808080"), (0.53, "#FFFFFF"), (1.0, "#FFFFFF")])
    nt.links.new(geo.outputs["Pointiness"], pr.inputs["Fac"])
    col = K.mix_rgb(nt, "MULTIPLY", col, pr.outputs["Color"])
    cr = None
    if crack:
        vo = nt.nodes.new("ShaderNodeTexVoronoi")
        vo.feature = "DISTANCE_TO_EDGE"
        vo.inputs["Scale"].default_value = 5.0 * scale
        nt.links.new(co, vo.inputs["Vector"])
        cr = K.math_node(nt, "LESS_THAN", vo.outputs["Distance"], vb=0.02)
        part = K.noise_tex(nt, 1.6 * scale, 3, 0.0, 0.5, co)
        cr = K.math_node(nt, "MULTIPLY", cr, K.math_node(nt, "GREATER_THAN", part.outputs["Fac"], vb=0.52))
        col = K.mix_rgb(nt, "MIX", col, vb=srgb("#020302"), fac=cr)
        cr = K.math_node(nt, "SUBTRACT", va=1.0, b=cr)
    b = nt.nodes.new("ShaderNodeBsdfPrincipled")
    nt.links.new(col, b.inputs["Base Color"])
    nt.links.new(col, b.inputs["Emission Color"])
    b.inputs["Emission Strength"].default_value = glow
    b.inputs["Roughness"].default_value = 0.55
    nt.links.new(_bump_chain(nt, co, 40.0 * scale, 0.7, cr), b.inputs["Normal"])
    nt.links.new(b.outputs[0], out.inputs["Surface"])
    return m


def castle_stone(name, scale=1.0, tint=("#05040E", "#100E26", "#221E48", "#3C3672")):
    """Night masonry: blue-violet blocks with dark mortar (brick texture)."""
    m, nt, out = K.mat(name)
    tc = nt.nodes.new("ShaderNodeTexCoord")
    co = tc.outputs["Object"]
    br = nt.nodes.new("ShaderNodeTexBrick")
    br.inputs["Scale"].default_value = 1.6 * scale
    br.inputs["Mortar Size"].default_value = 0.025
    br.inputs["Brick Width"].default_value = 0.9
    br.inputs["Row Height"].default_value = 0.4
    br.inputs["Color1"].default_value = (1, 1, 1, 1)
    br.inputs["Color2"].default_value = (0.6, 0.6, 0.6, 1)
    br.inputs["Mortar"].default_value = (0, 0, 0, 1)
    sw = nt.nodes.new("ShaderNodeSeparateXYZ")
    nt.links.new(co, sw.inputs[0])
    cmb = nt.nodes.new("ShaderNodeCombineXYZ")
    nt.links.new(K.math_node(nt, "ADD", sw.outputs["X"], sw.outputs["Y"]), cmb.inputs[0])
    nt.links.new(sw.outputs["Z"], cmb.inputs[1])
    nt.links.new(cmb.outputs[0], br.inputs["Vector"])
    n = K.noise_tex(nt, 3.0 * scale, 10, 2.0, 0.66, co)
    r = K.ramp(nt, [(0.0, tint[0]), (0.4, tint[1]), (0.6, tint[2]), (0.8, tint[3])])
    nt.links.new(n.outputs["Fac"], r.inputs["Fac"])
    col = K.mix_rgb(nt, "MULTIPLY", r.outputs["Color"], br.outputs["Color"])
    b = nt.nodes.new("ShaderNodeBsdfPrincipled")
    nt.links.new(col, b.inputs["Base Color"])
    b.inputs["Roughness"].default_value = 0.6
    nt.links.new(_bump_chain(nt, co, 25.0 * scale, 0.5, br.outputs["Fac"]), b.inputs["Normal"])
    nt.links.new(b.outputs[0], out.inputs["Surface"])
    return m


def iron(name, base="#2A2E52", rough=0.32, rust=0.0, glow=0.0, edge="#B8C4FF"):
    """Blued, scratched steel with bright worn edges."""
    m, nt, out = K.mat(name)
    tc = nt.nodes.new("ShaderNodeTexCoord")
    co = tc.outputs["Object"]
    n = K.noise_tex(nt, 6.0, 10, 1.0, 0.7, co)
    r = K.ramp(nt, [(0.0, "#06060E"), (0.45, base), (1.0, edge)])
    geo = nt.nodes.new("ShaderNodeNewGeometry")
    pt = K.math_node(nt, "ADD", K.math_node(nt, "MULTIPLY", K.math_node(nt, "SUBTRACT", geo.outputs["Pointiness"], vb=0.5), vb=6.0), vb=0.45)
    nt.links.new(K.math_node(nt, "ADD", pt, K.math_node(nt, "MULTIPLY", K.math_node(nt, "SUBTRACT", n.outputs["Fac"], vb=0.5), vb=0.4)), r.inputs["Fac"])
    b = nt.nodes.new("ShaderNodeBsdfPrincipled")
    nt.links.new(r.outputs["Color"], b.inputs["Base Color"])
    b.inputs["Metallic"].default_value = 1.0
    sc_ = nt.nodes.new("ShaderNodeTexWave")
    sc_.wave_type = "BANDS"
    sc_.inputs["Scale"].default_value = 60.0
    sc_.inputs["Distortion"].default_value = 18.0
    nt.links.new(co, sc_.inputs["Vector"])
    rr = K.math_node(nt, "ADD", K.math_node(nt, "MULTIPLY", sc_.outputs["Fac"], vb=0.25), vb=rough)
    nt.links.new(rr, b.inputs["Roughness"])
    if glow:
        nt.links.new(r.outputs["Color"], b.inputs["Emission Color"])
        b.inputs["Emission Strength"].default_value = glow
    nt.links.new(_bump_chain(nt, co, 30.0, 0.25), b.inputs["Normal"])
    nt.links.new(b.outputs[0], out.inputs["Surface"])
    return m


def parchment(name):
    m, nt, out = K.mat(name)
    tc = nt.nodes.new("ShaderNodeTexCoord")
    co = tc.outputs["Object"]
    n = K.noise_tex(nt, 3.0, 10, 1.5, 0.7, co)
    r = K.ramp(nt, [(0.0, "#2A1606"), (0.35, "#7A5428"), (0.55, "#C79E62"), (0.8, "#E9D2A0")])
    nt.links.new(n.outputs["Fac"], r.inputs["Fac"])
    geo = nt.nodes.new("ShaderNodeNewGeometry")
    pr = K.ramp(nt, [(0.0, "#200E02"), (0.45, "#9A7A50"), (0.52, "#FFFFFF"), (1.0, "#FFFFFF")])
    nt.links.new(geo.outputs["Pointiness"], pr.inputs["Fac"])
    col = K.mix_rgb(nt, "MULTIPLY", r.outputs["Color"], pr.outputs["Color"])
    b = nt.nodes.new("ShaderNodeBsdfPrincipled")
    nt.links.new(col, b.inputs["Base Color"])
    b.inputs["Roughness"].default_value = 0.8
    b.inputs["Subsurface Weight"].default_value = 0.15
    nt.links.new(_bump_chain(nt, co, 60.0, 0.25), b.inputs["Normal"])
    nt.links.new(b.outputs[0], out.inputs["Surface"])
    return m


def velvet(name, base=("#0A0218", "#2A0A52", "#5A1A9A"), sheen="#B080FF"):
    m, nt, out = K.mat(name)
    tc = nt.nodes.new("ShaderNodeTexCoord")
    co = tc.outputs["Object"]
    n = K.noise_tex(nt, 4.0, 8, 1.0, 0.6, co)
    r = K.ramp(nt, [(0.0, base[0]), (0.5, base[1]), (1.0, base[2])])
    nt.links.new(n.outputs["Fac"], r.inputs["Fac"])
    b = nt.nodes.new("ShaderNodeBsdfPrincipled")
    nt.links.new(r.outputs["Color"], b.inputs["Base Color"])
    b.inputs["Roughness"].default_value = 0.7
    b.inputs["Sheen Weight"].default_value = 1.0
    b.inputs["Sheen Tint"].default_value = srgb(sheen)
    nt.links.new(_bump_chain(nt, co, 90.0, 0.15), b.inputs["Normal"])
    nt.links.new(b.outputs[0], out.inputs["Surface"])
    return m


def glow_mat(name, hexc, strength):
    return K.emission(name, srgb(hexc), strength)


def cloud_sky(name="Clouds", stops=None, strength=1.0, scale=(0.05, 0.12, 1.0), drift=0.0, flow=(0.0, 0.0)):
    """The reel's violet cloud cover: billowing noise on a big plane, a
    little self-lit so it reads as moonlit. `flow` scrolls it per frame."""
    m, nt, out = K.mat(name)
    tc = nt.nodes.new("ShaderNodeTexCoord")
    mp = nt.nodes.new("ShaderNodeMapping")
    mp.inputs["Scale"].default_value = scale
    t = K.time_node(nt)
    if flow != (0.0, 0.0):
        cmb = nt.nodes.new("ShaderNodeCombineXYZ")
        nt.links.new(K.math_node(nt, "MULTIPLY", t, vb=flow[0]), cmb.inputs[0])
        nt.links.new(K.math_node(nt, "MULTIPLY", t, vb=flow[1]), cmb.inputs[1])
        nt.links.new(cmb.outputs[0], mp.inputs["Location"])
    nt.links.new(tc.outputs["Object"], mp.inputs["Vector"])
    n1 = K.noise_tex(nt, 1.0, 12, 2.6, 0.62, mp.outputs[0], dims="4D", w=K.math_node(nt, "MULTIPLY", t, vb=drift))
    r1 = K.ramp(nt, stops or [(0.0, "#010006"), (0.36, "#07031E"), (0.48, "#1A0C5A"), (0.6, "#3D24B0"),
                              (0.72, "#7A5CF0"), (0.84, "#B8A4FF"), (1.0, "#E8E0FF")])
    nt.links.new(n1.outputs["Fac"], r1.inputs["Fac"])
    e = nt.nodes.new("ShaderNodeEmission")
    e.inputs["Strength"].default_value = strength
    nt.links.new(r1.outputs["Color"], e.inputs["Color"])
    nt.links.new(e.outputs[0], out.inputs["Surface"])
    return m


def plane(name, loc, size, material, rot=(90, 0, 0)):
    bm = bmesh.new()
    uv = bm.loops.layers.uv.new()
    w, h = size
    vs = [bm.verts.new((x, y, 0)) for x, y in ((-w / 2, -h / 2), (w / 2, -h / 2), (w / 2, h / 2), (-w / 2, h / 2))]
    f = bm.faces.new(vs)
    for loop, (a, b) in zip(f.loops, ((0, 0), (1, 0), (1, 1), (0, 1))):
        loop[uv].uv = (a, b)
    ob = K.mesh_object(name, bm, material, smooth=False)
    ob.location = Vector(loc)
    ob.rotation_euler = tuple(math.radians(a) for a in rot)
    ob.visible_shadow = False
    return ob


def sky(loc, size, **kw):
    ob = plane("Sky", loc, size, cloud_sky(**kw))
    ob.visible_diffuse = False
    ob.visible_glossy = False
    return ob


# ---------------------------------------------------------------- the moon
def moon_mat(name="Moon", glow=0.9):
    m, nt, out = K.mat(name)
    tc = nt.nodes.new("ShaderNodeTexCoord")
    co = tc.outputs["Object"]
    n = K.noise_tex(nt, 3.0, 10, 0.6, 0.7, co)
    vo = nt.nodes.new("ShaderNodeTexVoronoi")
    vo.inputs["Scale"].default_value = 2.2
    vo.inputs["Randomness"].default_value = 1.0
    nt.links.new(K.mix_rgb(nt, "ADD", co, K.noise_tex(nt, 3.0, 4, 0.0, 0.5, co).outputs["Color"], fac=0.35), vo.inputs["Vector"])
    crater = K.ramp(nt, [(0.0, "#6A78C8"), (0.12, "#A8B4F0"), (0.3, "#E0E6FF"), (1.0, "#F0F4FF")])
    nt.links.new(vo.outputs["Distance"], crater.inputs["Fac"])
    r = K.ramp(nt, [(0.0, "#38409A"), (0.45, "#8A9AF0"), (0.7, "#D8E0FF"), (1.0, "#FFFFFF")])
    nt.links.new(n.outputs["Fac"], r.inputs["Fac"])
    col = K.mix_rgb(nt, "MULTIPLY", r.outputs["Color"], crater.outputs["Color"])
    b = nt.nodes.new("ShaderNodeBsdfPrincipled")
    nt.links.new(col, b.inputs["Base Color"])
    nt.links.new(col, b.inputs["Emission Color"])
    b.inputs["Emission Strength"].default_value = glow
    b.inputs["Roughness"].default_value = 0.8
    bp = nt.nodes.new("ShaderNodeBump")
    bp.inputs["Strength"].default_value = 0.6
    nt.links.new(vo.outputs["Distance"], bp.inputs["Height"])
    nt.links.new(bp.outputs["Normal"], b.inputs["Normal"])
    nt.links.new(b.outputs[0], out.inputs["Surface"])
    return m


def moon(loc, radius, face="helmet", glow=0.6, facing=None):
    """A pale cratered moon. face='helmet': the Openbar helmet rises out of it
    as a relief, light pouring through the visor slits (the reel's grin)."""
    m = moon_mat(glow=glow)
    bm = bmesh.new()
    bmesh.ops.create_uvsphere(bm, u_segments=64, v_segments=40, radius=radius)
    for v in bm.verts:
        v.co.y *= 0.45          # a squashed lens: reads as a disc, keeps a rim
    ob = K.mesh_object("Moon", bm, m)
    ob.location = Vector(loc)
    ob.visible_shadow = False
    if face == "helmet":
        # the helmet as a shadowed relief across the face; the bright moon
        # shows through the visor slits like the reel's grin
        shade = moon_mat("MoonFace", glow=glow * 0.18)
        hm, w, h = helmet(radius * 1.3, shade, thickness=0.1, dent=0.006)
        hm.location = Vector(loc) + Vector((radius * 0.03, -radius * 0.47, -radius * 0.05))
        hm.visible_shadow = False
    if facing is not None:
        ob.rotation_mode = "QUATERNION"
        ob.rotation_quaternion = K.look_rot(facing)
    return ob


# ---------------------------------------------------------------- fire, water
def fire_mat(name, seed=0.0, stops=None, strength=2.2):
    """Card of fire (UV 0..1, root at v=0): orange campfire by default."""
    m, nt, out = K.mat(name)
    tc = nt.nodes.new("ShaderNodeTexCoord")
    t = K.time_node(nt)
    sep = nt.nodes.new("ShaderNodeSeparateXYZ")
    nt.links.new(tc.outputs["UV"], sep.inputs[0])
    u, v = sep.outputs["X"], sep.outputs["Y"]
    cm = nt.nodes.new("ShaderNodeCombineXYZ")
    nt.links.new(K.math_node(nt, "MULTIPLY", u, vb=3.2), cm.inputs[0])
    nt.links.new(K.math_node(nt, "SUBTRACT", K.math_node(nt, "MULTIPLY", v, vb=2.2), K.math_node(nt, "MULTIPLY", t, vb=3.0)), cm.inputs[1])
    cm.inputs[2].default_value = seed * 7.3
    tn = K.noise_tex(nt, 2.0, 6, 1.4, 0.62, cm.outputs[0])
    # a teardrop envelope: wide at the root, a point at the top
    du = K.math_node(nt, "ABSOLUTE", K.math_node(nt, "SUBTRACT", u, vb=0.5))
    width = K.math_node(nt, "MULTIPLY", K.math_node(nt, "SUBTRACT", va=1.0, b=v), vb=0.5)
    env = K.math_node(nt, "SUBTRACT", width, du)
    f = K.math_node(nt, "ADD", K.math_node(nt, "MULTIPLY", env, vb=2.6),
                    K.math_node(nt, "MULTIPLY", K.math_node(nt, "SUBTRACT", tn.outputs["Fac"], vb=0.5), vb=1.5))
    f = K.math_node(nt, "SUBTRACT", f, K.math_node(nt, "MULTIPLY", v, vb=0.55))
    r = K.ramp(nt, stops or [(0.0, "#000000"), (0.05, "#300400"), (0.18, "#C02800"), (0.35, "#FF6A10"),
                             (0.55, "#FFB040"), (0.8, "#FFF0C0")])
    nt.links.new(f, r.inputs["Fac"])
    alpha = K.math_node(nt, "MAXIMUM", K.math_node(nt, "MINIMUM", K.math_node(nt, "MULTIPLY", f, vb=4.0), vb=1.0), vb=0.0)
    e = nt.nodes.new("ShaderNodeEmission")
    nt.links.new(r.outputs["Color"], e.inputs["Color"])
    e.inputs["Strength"].default_value = strength
    tr = nt.nodes.new("ShaderNodeBsdfTransparent")
    ms = nt.nodes.new("ShaderNodeMixShader")
    nt.links.new(alpha, ms.inputs["Fac"])
    nt.links.new(tr.outputs[0], ms.inputs[1])
    nt.links.new(e.outputs[0], ms.inputs[2])
    nt.links.new(ms.outputs[0], out.inputs["Surface"])
    return m


def fire(loc, w, h, cam_loc, n=5, seed=0.0, stops=None, strength=2.2):
    """A cluster of crossed fire cards facing the camera."""
    rr = random.Random(int(seed * 100) + 1)
    obs = []
    for i in range(n):
        m = fire_mat(f"Fire{seed}_{i}", seed + i * 1.37, stops, strength)
        ww, hh = w * rr.uniform(0.55, 1.0), h * rr.uniform(0.6, 1.0)
        p = Vector(loc) + Vector((rr.uniform(-w, w) * 0.2, rr.uniform(-w, w) * 0.2, 0))
        ob = plane(f"FireCard{seed}_{i}", p + Vector((0, 0, hh / 2)), (ww, hh), m)
        d = (Vector(cam_loc) - p)
        d.z = 0
        ob.rotation_euler = (math.radians(90), 0, math.atan2(d.y, d.x) + math.radians(90 + rr.uniform(-25, 25)))
        ob.visible_diffuse = ob.visible_glossy = False
        obs.append(ob)
    return obs


def water_mat(name, strength=1.6):
    """Falling water: streaks stretched down the card, scrolling."""
    m, nt, out = K.mat(name)
    tc = nt.nodes.new("ShaderNodeTexCoord")
    t = K.time_node(nt)
    sep = nt.nodes.new("ShaderNodeSeparateXYZ")
    nt.links.new(tc.outputs["UV"], sep.inputs[0])
    u, v = sep.outputs["X"], sep.outputs["Y"]
    cm = nt.nodes.new("ShaderNodeCombineXYZ")
    nt.links.new(K.math_node(nt, "MULTIPLY", u, vb=14.0), cm.inputs[0])
    nt.links.new(K.math_node(nt, "ADD", K.math_node(nt, "MULTIPLY", v, vb=0.8), K.math_node(nt, "MULTIPLY", t, vb=1.4)), cm.inputs[1])
    n = K.noise_tex(nt, 3.0, 8, 0.4, 0.65, cm.outputs[0])
    r = K.ramp(nt, [(0.0, "#000010"), (0.35, "#020A50"), (0.5, "#1030D0"), (0.64, "#3C7CFF"), (0.8, "#BCD8FF")])
    nt.links.new(n.outputs["Fac"], r.inputs["Fac"])
    du = K.math_node(nt, "ABSOLUTE", K.math_node(nt, "SUBTRACT", u, vb=0.5))
    edge = K.math_node(nt, "MULTIPLY", K.math_node(nt, "SUBTRACT", vb=0.5, va=0.5, b=du), vb=5.0)
    alpha = K.math_node(nt, "MINIMUM", K.math_node(nt, "SUBTRACT", va=0.5, b=du), vb=1.0)
    alpha = K.math_node(nt, "MINIMUM", K.math_node(nt, "MULTIPLY", alpha, vb=4.0), vb=1.0)
    streak = K.math_node(nt, "MINIMUM", K.math_node(nt, "MULTIPLY", K.math_node(nt, "SUBTRACT", n.outputs["Fac"], vb=0.32), vb=3.0), vb=1.0)
    alpha = K.math_node(nt, "MULTIPLY", alpha, K.math_node(nt, "MAXIMUM", streak, vb=0.0))
    e = nt.nodes.new("ShaderNodeEmission")
    nt.links.new(r.outputs["Color"], e.inputs["Color"])
    e.inputs["Strength"].default_value = strength
    tr = nt.nodes.new("ShaderNodeBsdfTransparent")
    ms = nt.nodes.new("ShaderNodeMixShader")
    nt.links.new(alpha, ms.inputs["Fac"])
    nt.links.new(tr.outputs[0], ms.inputs[1])
    nt.links.new(e.outputs[0], ms.inputs[2])
    nt.links.new(ms.outputs[0], out.inputs["Surface"])
    _ = edge
    return m


# ---------------------------------------------------------------- set pieces
def castle(origin, s=1.0, seed=3, stone=None, window="#FFB040"):
    """A crooked castle: keep, towers with needle spires, ragged walls, lit
    windows. Built around `origin` (base centre), front toward -Y."""
    rr = random.Random(seed)
    o = Vector(origin)
    stone = stone or castle_stone("Castle", scale=1.0 / s)
    roof = castle_stone("Roof", scale=2.0 / s, tint=("#020108", "#0A0820", "#1A1640", "#2A2460"))
    win = K.flicker_emission("Window", srgb(window), 6.0, speed=1.2, depth=0.3)
    obs = []

    def tower(x, y, r, h, spire, lean=0.0):
        prof = [(r * 1.08, 0), (r, h * 0.15), (r * 0.98, h * 0.85), (r * 1.18, h * 0.88), (r * 1.18, h), (0.02, h)]
        bm = K.revolve(prof, 12, fold=0.0, seed=rr.random() * 9, crumple=0.05, squash=1.0, step=0.25 * s)
        ob = K.mesh_object("Tower", bm, stone)
        ob.location = o + Vector((x, y, 0)) * s
        ob.scale = (s, s, s)
        ob.rotation_euler = (lean * 0.06, lean * 0.04, 0)
        obs.append(ob)
        sp = [(r * 1.28, 0), (r * 0.7, spire * 0.35), (r * 0.25, spire * 0.75), (0.01, spire)]
        bm = K.revolve(sp, 10, seed=rr.random() * 9, crumple=0.06, squash=1.0, step=0.25 * s, bend=lean * 0.6)
        ob2 = K.mesh_object("Spire", bm, roof)
        ob2.location = o + Vector((x, y, h)) * s + Vector((lean * h * 0.04 * s, -lean * h * 0.06 * s, 0))
        ob2.scale = (s, s, s)
        ob2.rotation_euler = ob.rotation_euler
        obs.append(ob2)
        for k in range(rr.randint(1, 3)):
            z = h * rr.uniform(0.35, 0.8)
            a = rr.uniform(-2.4, -0.7)
            p = o + Vector((x + math.cos(a) * r * 1.01, y + math.sin(a) * r * 1.01, z)) * s
            wb = bmesh.new()
            bmesh.ops.create_cube(wb, size=1.0)
            bmesh.ops.scale(wb, vec=(0.22 * s, 0.05 * s, 0.42 * s), verts=wb.verts)
            w_ = K.mesh_object("Win", wb, win, smooth=False)
            w_.location = p
            w_.rotation_euler.z = a + math.pi / 2
    # keep
    kb = bmesh.new()
    bmesh.ops.create_cube(kb, size=1.0)
    bmesh.ops.scale(kb, vec=(5.0, 3.6, 7.5), verts=kb.verts)
    bmesh.ops.translate(kb, vec=(0, 0, 3.75), verts=kb.verts)
    bmesh.ops.subdivide_edges(kb, edges=kb.edges[:], cuts=6, use_grid_fill=True)
    K.crumple_verts(kb, 0.18, 0.5, rr.random())
    kp = K.mesh_object("Keep", kb, stone, smooth=False)
    kp.location = o
    kp.scale = (s, s, s)
    obs.append(kp)
    tower(0, 0, 1.3, 10.5, 6.5, lean=0.3)
    tower(-3.2, -1.2, 1.0, 8.0, 4.5, lean=-0.6)
    tower(3.3, -1.0, 1.1, 9.0, 5.0, lean=0.8)
    tower(-6.0, 0.6, 0.8, 5.5, 3.6, lean=-1.0)
    tower(6.2, 0.8, 0.85, 6.2, 3.2, lean=1.2)
    tower(1.6, 2.0, 0.7, 12.5, 4.0, lean=-0.4)
    # curtain walls with merlons
    for x0, x1 in ((-6.0, -3.2), (3.3, 6.2)):
        wb = bmesh.new()
        bmesh.ops.create_cube(wb, size=1.0)
        bmesh.ops.scale(wb, vec=(abs(x1 - x0), 0.8, 3.2), verts=wb.verts)
        bmesh.ops.translate(wb, vec=((x0 + x1) / 2, -0.5, 1.6), verts=wb.verts)
        for k in range(int(abs(x1 - x0) / 0.55)):
            if rr.random() < 0.25:
                continue
            g = bmesh.ops.create_cube(wb, size=1.0)
            bmesh.ops.scale(wb, vec=(0.3, 0.8, rr.uniform(0.3, 0.6)), verts=g["verts"])
            bmesh.ops.translate(wb, vec=(min(x0, x1) + 0.3 + k * 0.55, -0.5, 3.4), verts=g["verts"])
        ob = K.mesh_object("Wall", wb, stone, smooth=False)
        ob.location = o
        ob.scale = (s, s, s)
        obs.append(ob)
    return obs


def cliff(name, loc, w, d, h, material, seed=0.0, sub=5):
    """A rough rock column for the castle to sit on."""
    bm = bmesh.new()
    bmesh.ops.create_cube(bm, size=1.0)
    bmesh.ops.scale(bm, vec=(w, d, h), verts=bm.verts)
    bmesh.ops.translate(bm, vec=(0, 0, -h / 2), verts=bm.verts)
    bmesh.ops.subdivide_edges(bm, edges=bm.edges[:], cuts=sub * 2, use_grid_fill=True)
    for v in bm.verts:
        t = -v.co.z / h
        v.co.x *= 1.0 + 0.35 * t
        v.co.y *= 1.0 + 0.35 * t
    K.crumple_verts(bm, min(w, h) * 0.12, 1.2 / min(w, h) * 3, seed)
    ob = K.mesh_object(name, bm, material)
    ob.location = Vector(loc)
    return ob


def tree(name, base, height, lean, bark, leaves, seed=0.0, branches=6, crown=1.0):
    """A twisted old tree: swept trunk, a few crooked limbs, clumped crown."""
    rr = random.Random(int(seed * 100) + 9)
    b = Vector(base)
    pts, rad = [], []
    for i in range(10):
        t = i / 9
        pts.append(b + Vector((lean * t * t * height + 0.3 * K.fbm(t * 2, seed, 0), 0.2 * K.fbm(seed, t * 2, 1), t * height)))
        rad.append(height * 0.07 * (1.25 - t * 0.8) * (1.6 if i == 0 else 1.0))
    bm = K.sweep(pts, rad, segs=14, crumple=0.25, seed=seed)
    tips = []
    for k in range(branches):
        i0 = rr.randint(3, 8)
        p0 = pts[i0]
        a = rr.uniform(0, 2 * math.pi)
        dirn = Vector((math.cos(a), math.sin(a) * 0.6, rr.uniform(0.2, 0.9))).normalized()
        L = height * rr.uniform(0.3, 0.55)
        bp = []
        for j in range(7):
            t = j / 6
            bp.append(p0 + dirn * L * t + Vector((0.15 * L * K.fbm(t * 3, k, seed), 0.1 * L * K.fbm(k, t * 3, seed), -0.2 * L * t * t)))
        K.sweep(bp, [rad[i0] * 0.5 * (1 - j / 7) + 0.008 * height for j in range(7)], segs=8, crumple=0.3, seed=seed + k, bm=bm)
        tips.append(bp[-1])
        # twigs: two or three crooked fingers off each limb
        for q in range(rr.randint(2, 3)):
            j0 = rr.randint(2, 5)
            a2 = rr.uniform(0, 2 * math.pi)
            d2 = (dirn + Vector((math.cos(a2), math.sin(a2), rr.uniform(-0.2, 0.6)))).normalized()
            L2 = L * rr.uniform(0.3, 0.5)
            tp = [bp[j0] + d2 * L2 * (m / 4) + Vector((0, 0, -0.1 * L2 * (m / 4) ** 2)) for m in range(5)]
            K.sweep(tp, [rad[i0] * 0.2 * (1 - m / 5) + 0.004 * height for m in range(5)], segs=6, crumple=0.3,
                    seed=seed + k * 7 + q, bm=bm)
    ob = K.mesh_object(name, bm, bark)
    if leaves is not None and crown > 0:
        lb = bmesh.new()
        for tp in tips:
            for _ in range(4):
                g = bmesh.ops.create_icosphere(lb, subdivisions=3, radius=height * rr.uniform(0.12, 0.2) * crown)
                bmesh.ops.translate(lb, vec=tp + Vector((rr.uniform(-1, 1), rr.uniform(-0.6, 0.6), rr.uniform(-0.3, 0.5))) * height * 0.12,
                                    verts=g["verts"])
        K.crumple_verts(lb, height * 0.05, 3.0 / height, seed)
        K.mesh_object(name + " leaves", lb, leaves)
    return ob


def knight(name, loc, face_dir, h=1.9, armour=None, metal=None, sit=0.0, seed=0.0, cape=None):
    """A hulking stone-armoured knight wearing the Openbar helmet. sit=1 drops
    the body as if seated on a log."""
    rr = random.Random(int(seed * 100) + 3)
    armour = armour or lichen_stone("Armour")
    metal = metal or armour
    root = K.link(bpy.data.objects.new(name, None))
    root.location = Vector(loc)
    root.rotation_mode = "QUATERNION"
    root.rotation_quaternion = K.look_rot(Vector(face_dir))
    drop = 0.42 * sit * h
    # torso: a big crumpled barrel
    prof = [(0.2, 0.0), (0.28, 0.12), (0.33, 0.35), (0.36, 0.55), (0.32, 0.72), (0.14, 0.8)]
    bm = K.revolve([(r * h, z * h) for r, z in prof], 18, fold=0.04, seed=seed, crumple=0.09, squash=0.78, step=0.04 * h)
    body = K.mesh_object(name + " body", bm, armour)
    body.parent = root
    body.location = (0, 0, 0.42 * h - drop)
    # legs: thick tubes, bent when seated
    for sx in (-1, 1):
        hip = Vector((sx * 0.14 * h, 0, 0.45 * h - drop))
        knee = hip + (Vector((sx * 0.04 * h, -0.36 * h, 0.02 * h)) if sit else Vector((sx * 0.02 * h, 0, -0.22 * h)))
        foot = knee + Vector((0, -0.04 * h, -(0.45 * h - drop) + 0.02 * h - (knee.z - (0.45 * h - drop)) - 0.02 * h))
        foot.z = 0.04 * h
        lb = K.sweep([hip, knee, foot], [0.09 * h, 0.08 * h, 0.075 * h], segs=12, crumple=0.12, seed=seed + sx)
        lg = K.mesh_object(name + " leg", lb, armour)
        lg.parent = root
    # pauldrons
    for sx in (-1, 1):
        pb = bmesh.new()
        bmesh.ops.create_uvsphere(pb, u_segments=16, v_segments=10, radius=0.12 * h)
        for v in pb.verts:
            v.co.z *= 0.75
        K.crumple_verts(pb, 0.03 * h, 6.0 / h, seed + sx * 3)
        pd = K.mesh_object(name + " pauldron", pb, metal)
        pd.parent = root
        pd.location = (sx * 0.33 * h, 0.02 * h, 0.96 * h - drop)
        ab = K.sweep([Vector((sx * 0.35 * h, 0, 0.9 * h - drop)), Vector((sx * 0.42 * h, -0.12 * h, 0.66 * h - drop)),
                      Vector((sx * 0.24 * h, -0.32 * h, 0.6 * h - drop))],
                     [0.075 * h, 0.065 * h, 0.06 * h], segs=12, crumple=0.12, seed=seed + 7 * sx)
        ar = K.mesh_object(name + " arm", ab, armour)
        ar.parent = root
    # helmet head
    hm, w, hh = helmet(0.5 * h, metal, thickness=0.55, dent=0.02)
    hm.parent = root
    hm.location = (0, -0.02 * h, 1.18 * h - drop)
    hm.rotation_euler = (math.radians(90), 0, math.radians(rr.uniform(-8, 8)))
    if cape:
        cb = K.revolve([(0.3 * h, 0.05 * h), (0.42 * h, 0.5 * h), (0.36 * h, 0.95 * h), (0.12 * h, 1.02 * h)], 18,
                       fold=0.12, seed=seed + 2, crumple=0.05, rag=0.08 * h, squash=0.7, step=0.04 * h, cap=False)
        for v in cb.verts:
            if v.co.y < -0.02 * h:
                v.co.y = -0.02 * h + (v.co.y + 0.02 * h) * 0.1
        cp = K.mesh_object(name + " cape", cb, cape)
        cp.parent = root
        cp.location = (0, 0.06 * h, 0.0 - drop * 0.5)
    return root


def ground(name, size, material, amp=0.4, z=0.0, centre=(0, 0)):
    def Hf(x, y):
        return z + amp * K.fbm(x * 0.12, y * 0.12, 0.3, 4)
    return K.grid_terrain(name, Hf, centre[0] - size[0] / 2, centre[0] + size[0] / 2,
                          centre[1] - size[1] / 2, centre[1] + size[1] / 2, 120, 80, material)


def moss_ground(name="Grass"):
    m, nt, out = K.mat(name)
    tc = nt.nodes.new("ShaderNodeTexCoord")
    co = tc.outputs["Object"]
    n = K.noise_tex(nt, 0.6, 10, 2.0, 0.66, co)
    r = K.ramp(nt, [(0.0, "#010308"), (0.4, "#06102A"), (0.55, "#0E2050"), (0.7, "#1E3A80"), (0.85, "#3A60B0")])
    nt.links.new(n.outputs["Fac"], r.inputs["Fac"])
    gr = K.noise_tex(nt, 220.0, 4, 0.0, 0.8, co)
    col = K.mix_rgb(nt, "MULTIPLY", r.outputs["Color"], K.ramp(nt, [(0.0, "#606060"), (1.0, "#FFFFFF")]).outputs["Color"])
    b = nt.nodes.new("ShaderNodeBsdfPrincipled")
    nt.links.new(col, b.inputs["Base Color"])
    b.inputs["Roughness"].default_value = 0.85
    bp = nt.nodes.new("ShaderNodeBump")
    bp.inputs["Strength"].default_value = 0.8
    nt.links.new(gr.outputs["Fac"], bp.inputs["Height"])
    nt.links.new(bp.outputs["Normal"], b.inputs["Normal"])
    nt.links.new(b.outputs[0], out.inputs["Surface"])
    return m


def cobble_mat(name="Cobble"):
    m, nt, out = K.mat(name)
    tc = nt.nodes.new("ShaderNodeTexCoord")
    co = tc.outputs["Object"]
    vo = nt.nodes.new("ShaderNodeTexVoronoi")
    vo.feature = "DISTANCE_TO_EDGE"
    vo.inputs["Scale"].default_value = 1.8
    nt.links.new(co, vo.inputs["Vector"])
    vc = nt.nodes.new("ShaderNodeTexVoronoi")
    vc.inputs["Scale"].default_value = 1.8
    nt.links.new(co, vc.inputs["Vector"])
    stone = K.ramp(nt, [(0.0, "#141A3A"), (0.5, "#3A4890"), (1.0, "#8A9AE0")])
    nt.links.new(vc.outputs["Distance"], stone.inputs["Fac"])
    gap = K.math_node(nt, "LESS_THAN", vo.outputs["Distance"], vb=0.06)
    col = K.mix_rgb(nt, "MIX", stone.outputs["Color"], vb=srgb("#010104"), fac=gap)
    b = nt.nodes.new("ShaderNodeBsdfPrincipled")
    nt.links.new(col, b.inputs["Base Color"])
    b.inputs["Roughness"].default_value = 0.45
    h = K.math_node(nt, "MINIMUM", K.math_node(nt, "MULTIPLY", vo.outputs["Distance"], vb=6.0), vb=1.0)
    nt.links.new(_bump_chain(nt, co, 30.0, 0.4, h), b.inputs["Normal"])
    nt.links.new(b.outputs[0], out.inputs["Surface"])
    return m


# ---------------------------------------------------------------- round 2: rougher world
def strata_rock(name, scale=1.0, base=("#010104", "#08081E", "#16163E", "#2E2C66", "#5A56A8"), glow=0.0):
    """Rock with life: layered strata bands bent by noise, dark crevices,
    pale worn ridges (pointiness), lichen flecks and strong bump."""
    m, nt, out = K.mat(name)
    tc = nt.nodes.new("ShaderNodeTexCoord")
    co = tc.outputs["Object"]
    warp = K.noise_tex(nt, 0.35 * scale, 6, 1.5, 0.6, co)
    wco = K.mix_rgb(nt, "ADD", co, warp.outputs["Color"], fac=2.5 / scale)
    wv = nt.nodes.new("ShaderNodeTexWave")
    wv.wave_type = "BANDS"
    wv.bands_direction = "Z"
    wv.inputs["Scale"].default_value = 0.9 * scale
    wv.inputs["Distortion"].default_value = 6.0
    wv.inputs["Detail"].default_value = 6.0
    nt.links.new(wco, wv.inputs["Vector"])
    n = K.noise_tex(nt, 1.2 * scale, 12, 2.0, 0.7, co)
    mixf = K.math_node(nt, "ADD", K.math_node(nt, "MULTIPLY", wv.outputs["Fac"], vb=0.45),
                       K.math_node(nt, "MULTIPLY", n.outputs["Fac"], vb=0.55))
    r = K.ramp(nt, [(0.0, base[0]), (0.3, base[1]), (0.5, base[2]), (0.68, base[3]), (0.85, base[4])])
    nt.links.new(mixf, r.inputs["Fac"])
    geo = nt.nodes.new("ShaderNodeNewGeometry")
    pr = K.ramp(nt, [(0.0, "#101010"), (0.45, "#606060"), (0.52, "#FFFFFF"), (0.6, "#FFFFFF"), (1.0, "#FFFFFF")])
    nt.links.new(geo.outputs["Pointiness"], pr.inputs["Fac"])
    col = K.mix_rgb(nt, "MULTIPLY", r.outputs["Color"], pr.outputs["Color"])
    vo = nt.nodes.new("ShaderNodeTexVoronoi")
    vo.feature = "DISTANCE_TO_EDGE"
    vo.inputs["Scale"].default_value = 2.5 * scale
    nt.links.new(wco, vo.inputs["Vector"])
    crev = K.math_node(nt, "LESS_THAN", vo.outputs["Distance"], vb=0.03)
    col = K.mix_rgb(nt, "MIX", col, vb=srgb("#000002"), fac=crev)
    fl = K.noise_tex(nt, 25.0 * scale, 4, 0.0, 0.8, co)
    fleck = K.math_node(nt, "MULTIPLY", K.math_node(nt, "GREATER_THAN", fl.outputs["Fac"], vb=0.66), vb=0.6)
    col = K.mix_rgb(nt, "MIX", col, vb=srgb("#7A84C8"), fac=fleck)
    b = nt.nodes.new("ShaderNodeBsdfPrincipled")
    nt.links.new(col, b.inputs["Base Color"])
    nt.links.new(col, b.inputs["Emission Color"])
    b.inputs["Emission Strength"].default_value = glow
    b.inputs["Roughness"].default_value = 0.62
    hgt = K.math_node(nt, "SUBTRACT", mixf, K.math_node(nt, "MULTIPLY", crev, vb=0.6))
    b1 = nt.nodes.new("ShaderNodeBump")
    b1.inputs["Strength"].default_value = 0.9
    b1.inputs["Distance"].default_value = 0.4
    nt.links.new(hgt, b1.inputs["Height"])
    nt.links.new(_bump_chain(nt, co, 14.0 * scale, 0.5), b1.inputs["Normal"])
    nt.links.new(b1.outputs["Normal"], b.inputs["Normal"])
    nt.links.new(b.outputs[0], out.inputs["Surface"])
    return m


def mountains(name, x0, x1, y0, y1, hmax, material, seed=0.0, nx=360, ny=60, base_z=-6.0, sharp=1.6):
    """A ragged range: ridged noise, jagged needles, ledges (terraced
    steps), the front rows falling away so it reads as a silhouette band."""
    o = Vector((seed * 13.0, seed * 7.0, 0))

    def Hf(x, y):
        p = Vector((x * 0.035, y * 0.035, 0.0)) + o
        ridge = noise.ridged_multi_fractal(p, 0.9, 2.1, 6, 1.0, 2.0, noise_basis="PERLIN_NEW")
        big = 0.5 + 0.5 * noise.noise(Vector((x * 0.012, y * 0.02, 0.5)) + o)
        hgt = hmax * (ridge * 0.42) ** sharp * (0.35 + 0.9 * big)
        hgt += 1.2 * noise.turbulence(Vector((x * 0.22, y * 0.22, 1.0)) + o, 4, True)
        step = hmax * 0.04
        hgt = hgt * 0.7 + 0.3 * (math.floor(hgt / step) * step)     # terraces
        t = (y - y0) / (y1 - y0)
        hgt *= ss_(0.0, 0.45, t)
        return base_z + max(0.0, hgt)
    ob = K.grid_terrain(name, Hf, x0, x1, y0, y1, nx, ny, material)
    ob.data.shade_smooth()
    return ob


def ss_(a, b, t):
    t = min(1.0, max(0.0, (t - a) / (b - a)))
    return t * t * (3 - 2 * t)


def crag_cliff(name, loc, w, d, h, material, seed=0.0):
    """The castle rock, round 2: a column with strata ledges, cut faces and
    a crown of broken spurs."""
    bm = bmesh.new()
    rr = random.Random(int(seed * 17) + 1)
    rings, segs, nz = [], 28, 26
    for k in range(nz + 1):
        t = k / nz
        z = -h * (1 - t)
        ring = []
        for s_ in range(segs):
            a = 2 * math.pi * s_ / segs
            r0 = 1.0 + 0.45 * (1 - t) ** 1.5
            n_ = K.fbm(math.cos(a) * 1.3 + seed, math.sin(a) * 1.3, t * 3.0, 4)
            ledge = 0.12 * (1 if (k % 4 == 0) else 0)
            r = r0 * (1 + 0.32 * n_ + ledge + 0.08 * (noise.cell(Vector((a * 2, t * 6, seed))) - 0.5))
            ring.append(bm.verts.new((math.cos(a) * r * w / 2, math.sin(a) * r * d / 2, z)))
        rings.append(ring)
    for a_, b_ in zip(rings, rings[1:]):
        for s_ in range(segs):
            t_ = (s_ + 1) % segs
            bm.faces.new((a_[s_], a_[t_], b_[t_], b_[s_]))
    bm.faces.new(tuple(rings[-1]))
    bmesh.ops.subdivide_edges(bm, edges=bm.edges[:], cuts=1)
    K.crumple_verts(bm, min(w, d) * 0.08, 6.0 / min(w, d), seed)
    ob = K.mesh_object(name, bm, material)
    ob.location = Vector(loc)
    # broken spurs around the top
    sp = bmesh.new()
    for i in range(9):
        a = rr.uniform(0, 2 * math.pi)
        rad = rr.uniform(0.35, 0.55)
        L = rr.uniform(0.15, 0.45) * h
        g = bmesh.ops.create_cone(sp, cap_ends=True, segments=5, radius1=rad * w * 0.18, radius2=0.0, depth=L)
        tilt = Vector((math.cos(a) * 0.5, math.sin(a) * 0.5, 1)).normalized()
        from mathutils import Matrix as _M
        mtx = _M.Translation((math.cos(a) * w * rad, math.sin(a) * d * rad, -L * 0.25)) @ tilt.to_track_quat("Z", "Y").to_matrix().to_4x4()
        bmesh.ops.transform(sp, matrix=mtx, verts=g["verts"])
    K.crumple_verts(sp, w * 0.02, 3.0, seed + 1)
    s = K.mesh_object(name + " spurs", sp, material, smooth=False)
    s.location = Vector(loc)
    return ob


def moon_mat2(name="Moon2", glow=0.6, dark=False):
    """A moon with life: maria (dark seas), craters at three scales with
    bright rims and rays, a cold limb, slight violet/blue variation."""
    m, nt, out = K.mat(name)
    tc = nt.nodes.new("ShaderNodeTexCoord")
    co = tc.outputs["Object"]
    sea = K.noise_tex(nt, 0.55, 4, 0.25, 0.55, co)
    sea_r = K.ramp(nt, [(0.0, "#FFFFFF"), (0.5, "#F0F2FF"), (0.6, "#A0A8E0"), (1.0, "#8890D0")])
    nt.links.new(sea.outputs["Fac"], sea_r.inputs["Fac"])
    col = sea_r.outputs["Color"]
    hsum = None
    for sc_, depth, rim in ((1.6, 0.8, 0.55), (4.5, 0.6, 0.4), (12.0, 0.35, 0.3)):
        vo = nt.nodes.new("ShaderNodeTexVoronoi")
        vo.inputs["Scale"].default_value = sc_
        vo.inputs["Randomness"].default_value = 1.0
        nt.links.new(K.mix_rgb(nt, "ADD", co, K.noise_tex(nt, sc_ * 1.3, 3, 0.0, 0.5, co).outputs["Color"], fac=0.25 / sc_ * 2), vo.inputs["Vector"])
        d = vo.outputs["Distance"]
        # bowl: dark inside 0..0.22, bright rim 0.22..0.3, flat outside
        bowl = K.ramp(nt, [(0.0, "#606A9A"), (0.2, "#8A94C8"), (0.26, "#FFFFFF"), (0.34, "#E4E8FF"), (1.0, "#E4E8FF")])
        nt.links.new(d, bowl.inputs["Fac"])
        col = K.mix_rgb(nt, "MULTIPLY", col, bowl.outputs["Color"], fac=depth * 1.3)
        h = K.math_node(nt, "MULTIPLY", K.math_node(nt, "MINIMUM", d, vb=0.3), vb=depth)
        hsum = h if hsum is None else K.math_node(nt, "ADD", hsum, h)
    tint = K.noise_tex(nt, 1.6, 4, 0.0, 0.5, co)
    tr = K.ramp(nt, [(0.0, "#9AA8FF"), (0.5, "#E8ECFF"), (1.0, "#C8B8FF")])
    nt.links.new(tint.outputs["Fac"], tr.inputs["Fac"])
    col = K.mix_rgb(nt, "MULTIPLY", col, tr.outputs["Color"])
    if dark:
        col = K.mix_rgb(nt, "MULTIPLY", col, vb=srgb("#3A4280"))
    # cold limb: a fresnel brightening at the rim
    fr = nt.nodes.new("ShaderNodeLayerWeight")
    fr.inputs["Blend"].default_value = 0.35
    limb = K.math_node(nt, "POWER", fr.outputs["Facing"], vb=2.0)
    b = nt.nodes.new("ShaderNodeBsdfPrincipled")
    nt.links.new(col, b.inputs["Base Color"])
    em = K.mix_rgb(nt, "ADD", col, vb=srgb("#6A7CFF"), fac=limb)
    nt.links.new(em, b.inputs["Emission Color"])
    b.inputs["Emission Strength"].default_value = glow
    b.inputs["Roughness"].default_value = 0.9
    bp = nt.nodes.new("ShaderNodeBump")
    bp.inputs["Strength"].default_value = 0.9
    bp.inputs["Distance"].default_value = 0.3
    nt.links.new(hsum, bp.inputs["Height"])
    nt.links.new(bp.outputs["Normal"], b.inputs["Normal"])
    nt.links.new(b.outputs[0], out.inputs["Surface"])
    return m


def moon2(loc, radius, glow=0.55):
    """Round-2 moon: the helmet as a cracked relief of darker moon rock,
    light leaking through the visor; a soft halo disc behind."""
    m = moon_mat2(glow=glow)
    bm = bmesh.new()
    bmesh.ops.create_uvsphere(bm, u_segments=96, v_segments=56, radius=radius)
    for v in bm.verts:
        v.co.y *= 0.45
    ob = K.mesh_object("Moon", bm, m)
    ob.location = Vector(loc)
    ob.visible_shadow = False
    shade = moon_mat2("MoonFace", glow=glow * 0.22, dark=True)
    hm, w, h = helmet(radius * 1.3, shade, thickness=0.1, dent=0.012, voxel=0.004)
    hm.location = Vector(loc) + Vector((radius * 0.03, -radius * 0.47, -radius * 0.05))
    hm.visible_shadow = False
    # halo: a soft radial emission card behind the moon
    hmat, nt, out = K.mat("Halo")
    tc = nt.nodes.new("ShaderNodeTexCoord")
    gr = nt.nodes.new("ShaderNodeTexGradient")
    gr.gradient_type = "SPHERICAL"
    mp = nt.nodes.new("ShaderNodeMapping")
    mp.inputs["Location"].default_value = (-1.0, -1.0, 0)
    mp.inputs["Scale"].default_value = (2.0, 2.0, 1)
    nt.links.new(tc.outputs["UV"], mp.inputs["Vector"])
    nt.links.new(mp.outputs[0], gr.inputs["Vector"])
    r = K.ramp(nt, [(0.0, "#000000"), (0.15, "#05041A"), (0.45, "#2A2490"), (1.0, "#8A90FF")])
    nt.links.new(K.math_node(nt, "MULTIPLY", gr.outputs["Fac"], vb=1.0), r.inputs["Fac"])
    e = nt.nodes.new("ShaderNodeEmission")
    nt.links.new(r.outputs["Color"], e.inputs["Color"])
    e.inputs["Strength"].default_value = 1.2
    tr = nt.nodes.new("ShaderNodeBsdfTransparent")
    add = nt.nodes.new("ShaderNodeAddShader")
    nt.links.new(e.outputs[0], add.inputs[0])
    nt.links.new(tr.outputs[0], add.inputs[1])
    nt.links.new(add.outputs[0], out.inputs["Surface"])
    halo = plane("MoonHalo", Vector(loc) + Vector((0, radius * 0.6, 0)), (radius * 3.4, radius * 3.4), hmat)
    halo.visible_diffuse = halo.visible_glossy = False
    return ob


def waterfall(name, top, width, height, material, seed=0.0, lean=0.0):
    """A falling sheet: narrow lip, spreading and bending outward as it
    falls, ragged sides; a foam cloud card at its foot."""
    rr = random.Random(int(seed * 31) + 2)
    bm = bmesh.new()
    uv = bm.loops.layers.uv.new()
    nx, nz = 6, 24
    grid = []
    for j in range(nz + 1):
        t = j / nz
        w = width * (0.7 + 0.8 * t ** 1.3)
        y = -lean * t * t * height * 0.15
        row = []
        for i in range(nx + 1):
            u = i / nx
            x = (u - 0.5) * w + 0.12 * width * K.fbm(t * 4 + seed, u * 3, 1.0, 2)
            row.append((bm.verts.new((x, y, -t * height)), (u, 1 - t)))
        grid.append(row)
    for j in range(nz):
        for i in range(nx):
            f = bm.faces.new((grid[j][i][0], grid[j][i + 1][0], grid[j + 1][i + 1][0], grid[j + 1][i][0]))
            for loop, (vv, uvv) in zip(f.loops, (grid[j][i], grid[j][i + 1], grid[j + 1][i + 1], grid[j + 1][i])):
                loop[uv].uv = uvv
    ob = K.mesh_object(name, bm, material, smooth=True)
    ob.location = Vector(top)
    ob.visible_diffuse = ob.visible_shadow = False
    foam = cloud_sky(name + "Foam", stops=[(0.0, "#000000"), (0.4, "#000000"), (0.6, "#1A2C9A"), (0.8, "#6A90FF"),
                                            (1.0, "#D8E4FF")], strength=0.9, scale=(0.6, 0.6, 1), drift=0.3)
    # foam alpha: radial falloff so the card has no edges
    nt = foam.node_tree
    out = next(n for n in nt.nodes if n.bl_idname == "ShaderNodeOutputMaterial")
    em = out.inputs["Surface"].links[0].from_node
    tc = nt.nodes.new("ShaderNodeTexCoord")
    gr = nt.nodes.new("ShaderNodeTexGradient")
    gr.gradient_type = "SPHERICAL"
    mp = nt.nodes.new("ShaderNodeMapping")
    mp.inputs["Location"].default_value = (-1.0, -1.0, 0)
    mp.inputs["Scale"].default_value = (2.0, 2.0, 1)
    nt.links.new(tc.outputs["UV"], mp.inputs["Vector"])
    nt.links.new(mp.outputs[0], gr.inputs["Vector"])
    tr = nt.nodes.new("ShaderNodeBsdfTransparent")
    mx = nt.nodes.new("ShaderNodeMixShader")
    nt.links.new(K.math_node(nt, "POWER", gr.outputs["Fac"], vb=1.5), mx.inputs["Fac"])
    nt.links.new(tr.outputs[0], mx.inputs[1])
    nt.links.new(em.outputs[0], mx.inputs[2])
    nt.links.new(mx.outputs[0], out.inputs["Surface"])
    fw = width * 3.2
    fo = plane(name + " foam", Vector(top) + Vector((0, -0.4, -height + fw * 0.15)), (fw, fw * 0.55), foam)
    fo.visible_diffuse = fo.visible_glossy = False
    return ob
