"""Openbar companion: silhouettes, materials, studio and eye anchors.

Everything is built from code so the art can be rebuilt. Units are metres; the
figures are toy sized (about 6 cm), which keeps the fur, stitch and fingerprint
scales physical.

This file holds the shapes (the bean, its feet, the helmet and its crest), the
base materials and the studio. companion_v2.py adds the louder materials, the
eyes and the head items; render_sprites.py renders the app's sprites. Eyes are
not in the sprites: the app draws them as outlines filled with their fabric.
"""
from __future__ import annotations

import math

import bmesh
import bpy
from mathutils import Matrix, Vector


# ---------------------------------------------------------------- utilities

def srgb(c):
    """sRGB 0-255 -> linear 0-1."""
    def one(v):
        v = v / 255.0
        return v / 12.92 if v <= 0.04045 else ((v + 0.055) / 1.055) ** 2.4
    return tuple(one(v) for v in c) + (1.0,)


def saturate(c, k):
    lum = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
    return tuple(max(0.0, min(1.0, lum + (v - lum) * k)) for v in c[:3]) + (1.0,)


def reset():
    bpy.ops.wm.read_factory_settings(use_empty=True)


def link(obj):
    bpy.context.scene.collection.objects.link(obj)
    return obj


def mesh_object(name, bm):
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    for p in me.polygons:
        p.use_smooth = True
    return link(bpy.data.objects.new(name, me))


def apply_modifiers(obj):
    deps = bpy.context.evaluated_depsgraph_get()
    me = bpy.data.meshes.new_from_object(obj.evaluated_get(deps))
    old = obj.data
    obj.modifiers.clear()
    obj.data = me
    bpy.data.meshes.remove(old)
    for p in me.polygons:
        p.use_smooth = True


def sphere_bm(radius, segments=96, rings=48):
    bm = bmesh.new()
    bmesh.ops.create_uvsphere(bm, u_segments=segments, v_segments=rings, radius=radius)
    return bm


def box_object(name, lo, hi, bevel=0.0):
    bm = bmesh.new()
    bmesh.ops.create_cube(bm, size=1.0)
    c = [(a + b) / 2 for a, b in zip(lo, hi)]
    s = [b - a for a, b in zip(lo, hi)]
    bmesh.ops.scale(bm, vec=Vector(s), verts=bm.verts)
    bmesh.ops.translate(bm, vec=Vector(c), verts=bm.verts)
    obj = mesh_object(name, bm)
    if bevel:
        bv = obj.modifiers.new("b", "BEVEL")
        bv.width = bevel
        bv.segments = 6
        apply_modifiers(obj)
    return obj


def boolean(obj, cutter, op="DIFFERENCE"):
    m = obj.modifiers.new("bool", "BOOLEAN")
    m.operation = op
    m.object = cutter
    m.solver = "EXACT"
    apply_modifiers(obj)
    bpy.data.objects.remove(cutter)


def subsurf(obj, levels=2):
    m = obj.modifiers.new("sub", "SUBSURF")
    m.levels = levels
    m.render_levels = levels
    return m


# ---------------------------------------------------------------- shapes

def bean_bm(r=0.03, depth=0.86, base=-0.86, pear=0.10):
    """Soft bean: an ellipsoid, slightly wider low, with a flat base."""
    bm = sphere_bm(r)
    for v in bm.verts:
        x, y, z = v.co
        t = (-z / r)  # 1 at the bottom
        widen = 1.0 + pear * max(0.0, t)
        v.co.x = x * widen
        v.co.y = y * depth * widen
        if z < base * r:
            v.co.z = base * r + (z - base * r) * 0.12
    return bm


def capsule_bm(r=0.024, length=0.042, depth=0.82):
    bm = sphere_bm(r, 128, 48)
    for v in bm.verts:
        x, y, z = v.co
        v.co.x = x + math.copysign(length / 2, x) if abs(x) > 1e-9 else x
        v.co.y = y * depth
        if z < -0.80 * r:
            v.co.z = -0.80 * r + (z + 0.80 * r) * 0.15
    return bm


def crest_object(name, scale=1.0, width=0.006, origin=(0, 0, 0), tilt=0.0):
    """The helmet crest: a transverse fan of flame tips swept to one side, like
    the logo's spikes. Drawn in (x, z) so it reads from the front; extruded
    along y. The base reaches below 0 so it sinks into the head.
    """
    pts = [
        (-0.016, -0.006), (-0.018, 0.004), (-0.020, 0.010), (-0.014, 0.009),
        (-0.012, 0.016), (-0.007, 0.012), (-0.004, 0.020), (0.001, 0.014),
        (0.006, 0.022), (0.009, 0.014), (0.016, 0.019), (0.016, 0.010),
        (0.022, 0.011), (0.018, 0.003), (0.016, -0.006), (0.000, -0.009),
    ]
    bm = bmesh.new()
    vs = [bm.verts.new((x * scale, 0.0, z * scale)) for x, z in pts]
    face = bm.faces.new(vs)
    ext = bmesh.ops.extrude_face_region(bm, geom=[face])
    top = [e for e in ext["geom"] if isinstance(e, bmesh.types.BMVert)]
    bmesh.ops.translate(bm, vec=Vector((0, width, 0)), verts=top)
    bmesh.ops.translate(bm, vec=Vector((0, -width / 2, 0)), verts=bm.verts)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    obj = mesh_object(name, bm)
    obj.rotation_euler = (tilt, 0, 0)
    obj.location = origin
    bv = obj.modifiers.new("b", "BEVEL")
    bv.width = width * 0.45
    bv.segments = 5
    bv.limit_method = "ANGLE"
    bv.use_clamp_overlap = True
    apply_modifiers(obj)
    m = obj.modifiers.new("rm", "REMESH")
    m.mode = "VOXEL"
    m.voxel_size = width * 0.08
    apply_modifiers(obj)
    sm = obj.modifiers.new("sm", "SMOOTH")
    sm.factor = 1.0
    sm.iterations = 12
    apply_modifiers(obj)
    return obj


def helmet_object(name, shell_bm, thickness, eye_z, slit_w, slit_h, mouth_w, bottom_z, back_z=None):
    """A Corinthian-style shell: T-shaped opening at the front, cut at the bottom.

    shell_bm is a closed surface the helmet hugs (already scaled up a bit).
    """
    obj = mesh_object(name, shell_bm)
    so = obj.modifiers.new("thick", "SOLIDIFY")
    so.thickness = thickness
    so.offset = 1.0
    apply_modifiers(obj)
    big = 0.2
    # bottom cut; the back is cut higher so it reads as a helmet, not a bowl
    boolean(obj, box_object("cut_bottom", (-big, -big, -big), (big, big, bottom_z)))
    if back_z is not None:
        cut = box_object("cut_back", (-big, 0.006, -big), (big, big, back_z))
        cut.rotation_euler = (math.radians(-18), 0, 0)
        boolean(obj, cut)
    r = min(slit_h, mouth_w) * 0.45
    boolean(obj, box_object("cut_eyes", (-slit_w / 2, -big, eye_z - slit_h / 2), (slit_w / 2, 0.0, eye_z + slit_h / 2), bevel=r))
    boolean(obj, box_object("cut_mouth", (-mouth_w / 2, -big, -big), (mouth_w / 2, 0.0, eye_z), bevel=r * 0.6))
    bv = obj.modifiers.new("soft", "BEVEL")
    bv.width = thickness * 0.45
    bv.segments = 4
    bv.limit_method = "ANGLE"
    bv.angle_limit = math.radians(40)
    bv.use_clamp_overlap = True
    return obj


def scaled_bm(make, sx, sy, sz, dz=0.0):
    bm = make()
    for v in bm.verts:
        v.co.x *= sx
        v.co.y *= sy
        v.co.z = v.co.z * sz + dz
    return bm


def nub(name, loc, r):
    bm = sphere_bm(r, 32, 16)
    for v in bm.verts:
        v.co.z *= 0.6
        v.co.y *= 1.15
    obj = mesh_object(name, bm)
    obj.location = loc
    return obj


def surface_y(obj, x, z):
    """Front surface depth (positive) of obj at (x, z), via a ray cast."""
    deps = bpy.context.evaluated_depsgraph_get()
    ev = obj.evaluated_get(deps)
    inv = obj.matrix_world.inverted()
    o = inv @ Vector((x, -1.0, z))
    d = (inv.to_3x3() @ Vector((0, 1, 0))).normalized()
    hit, loc, _n, _i = ev.ray_cast(o, d)
    if not hit:
        return 0.0
    return -(obj.matrix_world @ loc).y


# ---------------------------------------------------------------- materials

def _nodes(mat):
    mat.use_nodes = True
    nt = mat.node_tree
    nt.nodes.clear()
    return nt.nodes, nt.links, nt.nodes.new("ShaderNodeOutputMaterial")


def _math(n, l, op, a, b=None, val=None):
    m = n.new("ShaderNodeMath")
    m.operation = op
    for i, src in enumerate((a, b)):
        if src is None:
            continue
        if isinstance(src, (int, float)):
            m.inputs[i].default_value = src
        else:
            l.new(src, m.inputs[i])
    return m.outputs[0]


def plush_material(name, color):
    """Minky plush: soft sheen, deep fuzz handled by hair curves."""
    mat = bpy.data.materials.new(name)
    n, l, out = _nodes(mat)
    bsdf = n.new("ShaderNodeBsdfPrincipled")
    tex = n.new("ShaderNodeTexNoise")
    tex.inputs["Scale"].default_value = 600.0
    tex.inputs["Detail"].default_value = 8.0
    jit = n.new("ShaderNodeMapRange")
    jit.inputs["To Min"].default_value = 0.86
    jit.inputs["To Max"].default_value = 1.08
    l.new(tex.outputs["Fac"], jit.inputs["Value"])
    mul = n.new("ShaderNodeMix")
    mul.data_type = "RGBA"
    mul.blend_type = "MULTIPLY"
    mul.inputs["Factor"].default_value = 1.0
    mul.inputs["A"].default_value = srgb(color)
    l.new(jit.outputs["Result"], mul.inputs["B"])
    l.new(mul.outputs["Result"], bsdf.inputs["Base Color"])
    bsdf.inputs["Roughness"].default_value = 0.95
    bsdf.inputs["Specular IOR Level"].default_value = 0.2
    bsdf.inputs["Sheen Weight"].default_value = 1.0
    bsdf.inputs["Sheen Roughness"].default_value = 0.35
    bsdf.inputs["Subsurface Weight"].default_value = 0.1
    bsdf.inputs["Subsurface Radius"].default_value = (0.002, 0.0015, 0.001)
    fib = n.new("ShaderNodeTexNoise")
    fib.inputs["Scale"].default_value = 3000.0
    fib.inputs["Detail"].default_value = 10.0
    fib.inputs["Distortion"].default_value = 2.0
    bump = n.new("ShaderNodeBump")
    bump.inputs["Strength"].default_value = 0.4
    bump.inputs["Distance"].default_value = 0.0002
    l.new(fib.outputs["Fac"], bump.inputs["Height"])
    l.new(bump.outputs["Normal"], bsdf.inputs["Normal"])
    l.new(bsdf.outputs[0], out.inputs["Surface"])
    mat["fuzz"] = 1
    return mat


def knit_material(name, color, stitch=0.0042, displace=True):
    """Stocking-stitch knit: rows of V stitches wrapped around the vertical axis.

    u: around the body (angle times a nominal radius), v: height.
    Each stitch is two slanted legs; the height field drives true displacement.
    """
    mat = bpy.data.materials.new(name)
    n, l, out = _nodes(mat)
    tc = n.new("ShaderNodeTexCoord")
    sep = n.new("ShaderNodeSeparateXYZ")
    l.new(tc.outputs["Object"], sep.inputs[0])
    ang = _math(n, l, "ARCTAN2", sep.outputs["X"], None)
    # ARCTAN2(x, -y): build -y
    neg_y = _math(n, l, "MULTIPLY", sep.outputs["Y"], -1.0)
    ang.node.inputs[1].default_value = 0.0
    l.new(neg_y, ang.node.inputs[1])
    u = _math(n, l, "MULTIPLY", ang, 0.03 / stitch)
    v0 = _math(n, l, "MULTIPLY", sep.outputs["Z"], 1.0 / (stitch * 0.78))
    a = _math(n, l, "FRACT", u)
    side = _math(n, l, "ABSOLUTE", _math(n, l, "SUBTRACT", a, 0.5))
    v = _math(n, l, "ADD", v0, _math(n, l, "MULTIPLY", side, 1.1))
    s = _math(n, l, "FRACT", v)
    leg_a = _math(n, l, "FRACT", _math(n, l, "MULTIPLY", a, 2.0))
    # bumps: sin(pi*s)^0.7 * sin(pi*leg)^0.9
    hs = _math(n, l, "POWER", _math(n, l, "SINE", _math(n, l, "MULTIPLY", s, math.pi)), 0.7)
    hl = _math(n, l, "POWER", _math(n, l, "SINE", _math(n, l, "MULTIPLY", leg_a, math.pi)), 0.9)
    h = _math(n, l, "MULTIPLY", hs, hl)
    # yarn ply twist: fine noise along the stitch
    ply = n.new("ShaderNodeTexNoise")
    ply.inputs["Scale"].default_value = 2600.0
    ply.inputs["Detail"].default_value = 6.0
    hp = _math(n, l, "ADD", h, _math(n, l, "MULTIPLY", ply.outputs["Fac"], 0.18))
    # colour: crevices darker
    ramp = n.new("ShaderNodeMapRange")
    ramp.inputs["To Min"].default_value = 0.55
    ramp.inputs["To Max"].default_value = 1.05
    l.new(h, ramp.inputs["Value"])
    mul = n.new("ShaderNodeMix")
    mul.data_type = "RGBA"
    mul.blend_type = "MULTIPLY"
    mul.inputs["Factor"].default_value = 1.0
    mul.inputs["A"].default_value = srgb(color)
    l.new(ramp.outputs["Result"], mul.inputs["B"])
    bsdf = n.new("ShaderNodeBsdfPrincipled")
    l.new(mul.outputs["Result"], bsdf.inputs["Base Color"])
    bsdf.inputs["Roughness"].default_value = 0.9
    bsdf.inputs["Sheen Weight"].default_value = 0.8
    bsdf.inputs["Sheen Roughness"].default_value = 0.4
    bsdf.inputs["Specular IOR Level"].default_value = 0.25
    bsdf.inputs["Subsurface Weight"].default_value = 0.12
    bsdf.inputs["Subsurface Radius"].default_value = (0.0015, 0.001, 0.0008)
    bump = n.new("ShaderNodeBump")
    bump.inputs["Strength"].default_value = 0.6
    bump.inputs["Distance"].default_value = 0.0004
    l.new(hp, bump.inputs["Height"])
    l.new(bump.outputs["Normal"], bsdf.inputs["Normal"])
    disp = n.new("ShaderNodeDisplacement")
    disp.inputs["Scale"].default_value = stitch * 0.28
    disp.inputs["Midlevel"].default_value = 0.5
    l.new(h, disp.inputs["Height"])
    if displace:
        l.new(disp.outputs[0], out.inputs["Displacement"])
    l.new(bsdf.outputs[0], out.inputs["Surface"])
    try:
        mat.displacement_method = "BOTH"
    except AttributeError:
        mat.cycles.displacement_method = "BOTH"
    mat["fuzz"] = 2
    mat["subdiv"] = 1 if displace else 0
    return mat


def gummy_material(name, color):
    mat = bpy.data.materials.new(name)
    n, l, out = _nodes(mat)
    bsdf = n.new("ShaderNodeBsdfPrincipled")
    bsdf.subsurface_method = "RANDOM_WALK"
    c = saturate(srgb(color), 1.35)
    bsdf.inputs["Base Color"].default_value = c
    bsdf.inputs["Subsurface Weight"].default_value = 1.0
    bsdf.inputs["Subsurface Radius"].default_value = (max(c[0], 0.2), max(c[1], 0.2), max(c[2], 0.2))
    bsdf.inputs["Subsurface Scale"].default_value = 0.010
    bsdf.inputs["Transmission Weight"].default_value = 0.5
    bsdf.inputs["Roughness"].default_value = 0.12
    bsdf.inputs["IOR"].default_value = 1.45
    bsdf.inputs["Coat Weight"].default_value = 1.0
    bsdf.inputs["Coat Roughness"].default_value = 0.04
    # sugar-dusted micro roughness on the coat
    tex = n.new("ShaderNodeTexNoise")
    tex.inputs["Scale"].default_value = 1800.0
    bump = n.new("ShaderNodeBump")
    bump.inputs["Strength"].default_value = 0.05
    l.new(tex.outputs["Fac"], bump.inputs["Height"])
    l.new(bump.outputs["Normal"], bsdf.inputs["Coat Normal"])
    l.new(bsdf.outputs[0], out.inputs["Surface"])
    mat["rounded"] = 1
    return mat


def clay_material(name, color):
    """Plasticine: matte, a little waxy, thumb dents and fingerprint rings."""
    mat = bpy.data.materials.new(name)
    n, l, out = _nodes(mat)
    tc = n.new("ShaderNodeTexCoord")
    bsdf = n.new("ShaderNodeBsdfPrincipled")
    c = srgb(color)
    bsdf.inputs["Base Color"].default_value = c
    bsdf.inputs["Roughness"].default_value = 0.62
    bsdf.inputs["Specular IOR Level"].default_value = 0.3
    bsdf.inputs["Subsurface Weight"].default_value = 0.2
    bsdf.inputs["Subsurface Radius"].default_value = (0.002, 0.001, 0.0007)
        # fingerprints: distorted rings, masked to a few blobs
    wave = n.new("ShaderNodeTexWave")
    wave.wave_type = "RINGS"
    wave.inputs["Scale"].default_value = 170.0
    wave.inputs["Distortion"].default_value = 6.0
    wave.inputs["Detail"].default_value = 2.0
    l.new(tc.outputs["Object"], wave.inputs["Vector"])
    vor = n.new("ShaderNodeTexVoronoi")
    vor.inputs["Scale"].default_value = 30.0
    l.new(tc.outputs["Object"], vor.inputs["Vector"])
    mask = n.new("ShaderNodeMapRange")
    mask.inputs["From Min"].default_value = 0.12
    mask.inputs["From Max"].default_value = 0.42
    mask.inputs["To Min"].default_value = 1.0
    mask.inputs["To Max"].default_value = 0.0
    l.new(vor.outputs["Distance"], mask.inputs["Value"])
    prints = _math(n, l, "MULTIPLY", wave.outputs["Fac"], mask.outputs["Result"])
    dents = n.new("ShaderNodeTexNoise")
    dents.inputs["Scale"].default_value = 55.0
    dents.inputs["Detail"].default_value = 4.0
    l.new(tc.outputs["Object"], dents.inputs["Vector"])
    h = _math(n, l, "ADD", _math(n, l, "MULTIPLY", prints, 0.12), dents.outputs["Fac"])
    bump = n.new("ShaderNodeBump")
    bump.inputs["Strength"].default_value = 0.75
    bump.inputs["Distance"].default_value = 0.0012
    l.new(h, bump.inputs["Height"])
    l.new(bump.outputs["Normal"], bsdf.inputs["Normal"])
    l.new(bsdf.outputs[0], out.inputs["Surface"])
    return mat


def void_material(name="void"):
    mat = bpy.data.materials.new(name)
    n, l, out = _nodes(mat)
    bsdf = n.new("ShaderNodeBsdfPrincipled")
    bsdf.inputs["Base Color"].default_value = srgb((14, 14, 18))
    bsdf.inputs["Roughness"].default_value = 0.8
    l.new(bsdf.outputs[0], out.inputs["Surface"])
    return mat


TEXTURES = {
    "plush": plush_material,
    "knit": knit_material,
    "gummy": gummy_material,
    "clay": clay_material,
}


# ---------------------------------------------------------------- fuzz

def fuzz_tree(name="Fuzz"):
    if name in bpy.data.node_groups:
        return bpy.data.node_groups[name]
    tree = bpy.data.node_groups.new(name, "GeometryNodeTree")
    iface = tree.interface
    iface.new_socket("Geometry", in_out="INPUT", socket_type="NodeSocketGeometry")
    iface.new_socket("Density", in_out="INPUT", socket_type="NodeSocketFloat")
    iface.new_socket("Length", in_out="INPUT", socket_type="NodeSocketFloat")
    iface.new_socket("Radius", in_out="INPUT", socket_type="NodeSocketFloat")
    iface.new_socket("Lift", in_out="INPUT", socket_type="NodeSocketFloat")
    iface.new_socket("Material", in_out="INPUT", socket_type="NodeSocketMaterial")
    iface.new_socket("Surface", in_out="INPUT", socket_type="NodeSocketObject")
    iface.new_socket("Geometry", in_out="OUTPUT", socket_type="NodeSocketGeometry")
    n, l = tree.nodes, tree.links
    gin, gout = n.new("NodeGroupInput"), n.new("NodeGroupOutput")
    info = n.new("GeometryNodeObjectInfo")
    info.transform_space = "RELATIVE"
    l.new(gin.outputs["Surface"], info.inputs["Object"])
    dist = n.new("GeometryNodeDistributePointsOnFaces")
    dist.distribute_method = "RANDOM"
    l.new(info.outputs["Geometry"], dist.inputs["Mesh"])
    l.new(gin.outputs["Density"], dist.inputs["Density"])
    rnd = n.new("FunctionNodeRandomValue")
    rnd.data_type = "FLOAT_VECTOR"
    rnd.inputs[0].default_value = (-1.0, -1.0, -1.0)
    rnd.inputs[1].default_value = (1.0, 1.0, 1.0)
    rnd.inputs["Seed"].default_value = 3
    nscale = n.new("ShaderNodeVectorMath")
    nscale.operation = "SCALE"
    l.new(dist.outputs["Normal"], nscale.inputs[0])
    l.new(gin.outputs["Lift"], nscale.inputs["Scale"])
    add = n.new("ShaderNodeVectorMath")
    add.operation = "ADD"
    l.new(nscale.outputs[0], add.inputs[0])
    l.new([o for o in rnd.outputs if o.type == "VECTOR"][0], add.inputs[1])
    align = n.new("FunctionNodeAlignRotationToVector")
    align.axis = "Z"
    l.new(add.outputs[0], align.inputs["Vector"])
    line = n.new("GeometryNodeCurvePrimitiveLine")
    line.inputs["Start"].default_value = (0, 0, 0)
    line.inputs["End"].default_value = (0, 0, 1)
    resample = n.new("GeometryNodeResampleCurve")
    resample.inputs["Count"].default_value = 5
    l.new(line.outputs[0], resample.inputs["Curve"])
    inst = n.new("GeometryNodeInstanceOnPoints")
    l.new(dist.outputs["Points"], inst.inputs["Points"])
    l.new(resample.outputs[0], inst.inputs["Instance"])
    l.new(align.outputs["Rotation"], inst.inputs["Rotation"])
    rlen = n.new("FunctionNodeRandomValue")
    rlen.data_type = "FLOAT"
    [s for s in rlen.inputs if s.name == "Min" and s.type == "VALUE"][0].default_value = 0.45
    [s for s in rlen.inputs if s.name == "Max" and s.type == "VALUE"][0].default_value = 1.0
    rlen.inputs["Seed"].default_value = 7
    slen = n.new("ShaderNodeMath")
    slen.operation = "MULTIPLY"
    l.new([o for o in rlen.outputs if o.type == "VALUE"][0], slen.inputs[0])
    l.new(gin.outputs["Length"], slen.inputs[1])
    l.new(slen.outputs[0], inst.inputs["Scale"])
    real = n.new("GeometryNodeRealizeInstances")
    l.new(inst.outputs["Instances"], real.inputs["Geometry"])
    pos = n.new("GeometryNodeInputPosition")
    noise = n.new("ShaderNodeTexNoise")
    noise.noise_dimensions = "3D"
    noise.inputs["Scale"].default_value = 3000.0
    l.new(pos.outputs[0], noise.inputs["Vector"])
    centre = n.new("ShaderNodeVectorMath")
    centre.operation = "SUBTRACT"
    centre.inputs[1].default_value = (0.5, 0.5, 0.5)
    l.new(noise.outputs["Color"], centre.inputs[0])
    param = n.new("GeometryNodeSplineParameter")
    amt = n.new("ShaderNodeMath")
    amt.operation = "MULTIPLY"
    l.new(param.outputs["Factor"], amt.inputs[0])
    l.new(gin.outputs["Length"], amt.inputs[1])
    off = n.new("ShaderNodeVectorMath")
    off.operation = "SCALE"
    l.new(centre.outputs[0], off.inputs[0])
    l.new(amt.outputs[0], off.inputs["Scale"])
    setpos = n.new("GeometryNodeSetPosition")
    l.new(real.outputs[0], setpos.inputs["Geometry"])
    l.new(off.outputs[0], setpos.inputs["Offset"])
    # taper: radius falls to 30% at the tip
    taper = n.new("ShaderNodeMapRange")
    taper.inputs["To Min"].default_value = 1.0
    taper.inputs["To Max"].default_value = 0.3
    l.new(param.outputs["Factor"], taper.inputs["Value"])
    rr = n.new("ShaderNodeMath")
    rr.operation = "MULTIPLY"
    l.new(taper.outputs["Result"], rr.inputs[0])
    l.new(gin.outputs["Radius"], rr.inputs[1])
    srad = n.new("GeometryNodeSetCurveRadius")
    l.new(setpos.outputs[0], srad.inputs["Curve"])
    l.new(rr.outputs[0], srad.inputs["Radius"])
    smat = n.new("GeometryNodeSetMaterial")
    l.new(srad.outputs[0], smat.inputs["Geometry"])
    l.new(gin.outputs["Material"], smat.inputs["Material"])
    l.new(smat.outputs[0], gout.inputs["Geometry"])
    return tree


def set_mod_input(mod, name, value):
    for item in mod.node_group.interface.items_tree:
        if getattr(item, "in_out", None) == "INPUT" and item.name == name:
            try:
                getattr(mod.properties.inputs, item.identifier).value = value
            except AttributeError:
                mod[item.identifier] = value
            return
    raise KeyError(name)


def add_fuzz(obj, mat, kind):
    """kind 1: plush pile (long, dense); kind 2: knit yarn halo (sparse, short)."""
    if kind == 1:
        density, length, radius, lift = 6.0e7, 0.0016, 0.000016, 1.4
    else:
        density, length, radius, lift = 6.0e6, 0.0007, 0.000006, 0.4
    hair = link(bpy.data.objects.new(obj.name + "_fuzz", bpy.data.hair_curves.new(obj.name + "_fuzz")))
    hair.data.materials.append(mat)
    mod = hair.modifiers.new("Fuzz", "NODES")
    mod.node_group = fuzz_tree()
    set_mod_input(mod, "Density", density)
    set_mod_input(mod, "Length", length)
    set_mod_input(mod, "Radius", radius)
    set_mod_input(mod, "Lift", lift)
    set_mod_input(mod, "Material", mat)
    set_mod_input(mod, "Surface", obj)
    return hair


# ---------------------------------------------------------------- studio

def studio(res=1024, samples=256, yaw_deg=-14.0, elev_deg=7.0, objs=(), margin=1.18):
    sc = bpy.context.scene
    sc.render.engine = "CYCLES"
    prefs = bpy.context.preferences.addons["cycles"].preferences
    prefs.compute_device_type = "METAL"
    prefs.get_devices()
    for d in prefs.devices:
        d.use = d.type == "METAL"
    try:
        prefs.metalrt = "OFF"
    except Exception:
        pass
    sc.cycles.device = "GPU"
    sc.cycles.samples = samples
    sc.cycles.use_denoising = True
    sc.render.film_transparent = True
    sc.render.resolution_x = sc.render.resolution_y = res
    sc.render.image_settings.file_format = "PNG"
    sc.render.image_settings.color_mode = "RGBA"
    sc.render.image_settings.color_depth = "16"
    try:
        sc.view_settings.view_transform = "Khronos PBR Neutral"
    except TypeError:
        sc.view_settings.view_transform = "Standard"

    world = bpy.data.worlds.new("w")
    sc.world = world
    world.use_nodes = True
    bg = world.node_tree.nodes["Background"]
    bg.inputs["Color"].default_value = (0.34, 0.34, 0.36, 1)
    bg.inputs["Strength"].default_value = 0.35

    def area(name, loc, size, power, color=(1, 1, 1)):
        ld = bpy.data.lights.new(name, "AREA")
        ld.shape = "DISK"
        ld.size = size
        ld.energy = power
        ld.color = color
        ob = link(bpy.data.objects.new(name, ld))
        ob.location = loc
        d = -Vector(loc)
        ob.rotation_euler = d.to_track_quat("-Z", "Y").to_euler()
        return ob

    area("key", (-0.20, -0.26, 0.24), 0.22, 9.0, (1.0, 0.97, 0.93))
    area("fill", (0.28, -0.22, 0.04), 0.30, 3.0, (0.92, 0.95, 1.0))
    area("rim", (0.10, 0.30, 0.22), 0.16, 7.0)
    area("rim2", (-0.22, 0.24, 0.10), 0.12, 4.0)

    # the figure turns, the camera stays: yaw is applied to a parent empty
    pivot = link(bpy.data.objects.new("pivot", None))
    for o in bpy.context.scene.objects:
        if o.type in {"MESH", "CURVES"} and o.parent is None:
            o.parent = pivot
    pivot.rotation_euler = (0, 0, math.radians(yaw_deg))
    bpy.context.view_layer.update()

    cam_d = bpy.data.cameras.new("cam")
    cam_d.lens = 100
    cam_d.sensor_width = 36
    cam = link(bpy.data.objects.new("cam", cam_d))
    sc.camera = cam
    # frame: bounding box of every mesh, from the front, a little above
    lo = Vector((1e9, 1e9, 1e9))
    hi = Vector((-1e9, -1e9, -1e9))
    for o in sc.objects:
        if o.type == "MESH":
            for c in o.bound_box:
                w = o.matrix_world @ Vector(c)
                lo = Vector(map(min, lo, w))
                hi = Vector(map(max, hi, w))
    centre = (lo + hi) / 2
    size = max(hi.x - lo.x, hi.z - lo.z) * margin
    dist = size / 2 / math.tan(math.atan(18 / 100))
    e = math.radians(elev_deg)
    cam.location = centre + Vector((0, -dist * math.cos(e), dist * math.sin(e)))
    cam.rotation_euler = (centre - cam.location).to_track_quat("-Z", "Y").to_euler()
    bpy.context.view_layer.update()
    return cam, pivot


def project(cam, pivot, p):
    """World point (in figure space, before yaw) -> pixel (x, y) from top-left, plus depth."""
    from bpy_extras.object_utils import world_to_camera_view
    sc = bpy.context.scene
    w = pivot.matrix_world @ Vector(p)
    v = world_to_camera_view(sc, cam, w)
    return v.x * sc.render.resolution_x, (1 - v.y) * sc.render.resolution_y


def eye_pixels(cam, pivot, eyes):
    out = []
    for centre, (w, h) in eyes:
        cx, cy = project(cam, pivot, centre)
        x0, _ = project(cam, pivot, (centre[0] - w / 2, centre[1], centre[2]))
        x1, _ = project(cam, pivot, (centre[0] + w / 2, centre[1], centre[2]))
        _, y0 = project(cam, pivot, (centre[0], centre[1], centre[2] + h / 2))
        _, y1 = project(cam, pivot, (centre[0], centre[1], centre[2] - h / 2))
        out.append({"x": cx, "y": cy, "w": abs(x1 - x0), "h": abs(y1 - y0)})
    return out
