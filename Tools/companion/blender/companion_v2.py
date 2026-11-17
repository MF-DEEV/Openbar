"""The companion's materials as the app shows them: needle felt with flyaway
fibres, chunky knit, sugared gummy and fingerprinted clay; the eye fabrics; and
the head items (helmet, beanie, cap, crown, headphones) it can put on and take
off. Shapes and the studio come from companion_lib.
"""
from __future__ import annotations

import math

import bmesh
import bpy
from mathutils import Vector

import companion_lib as L
from companion_lib import _math, _nodes, srgb

EYE_BLACK = (30, 26, 30)
EYE_CREAM = (242, 230, 202)


# ---------------------------------------------------------------- materials

def _mottle(n, l, color, scale=160.0, lo=0.82, hi=1.14):
    """Base colour with wool-like mottling: blotches plus fine fibre streaks."""
    tc = n.new("ShaderNodeTexCoord")
    blot = n.new("ShaderNodeTexNoise")
    blot.inputs["Scale"].default_value = scale
    blot.inputs["Detail"].default_value = 8.0
    blot.inputs["Roughness"].default_value = 0.65
    l.new(tc.outputs["Object"], blot.inputs["Vector"])
    streak = n.new("ShaderNodeTexNoise")
    streak.inputs["Scale"].default_value = 2200.0
    streak.inputs["Detail"].default_value = 4.0
    streak.inputs["Distortion"].default_value = 4.0
    l.new(tc.outputs["Object"], streak.inputs["Vector"])
    mix = _math(n, l, "ADD", _math(n, l, "MULTIPLY", blot.outputs["Fac"], 0.7),
                _math(n, l, "MULTIPLY", streak.outputs["Fac"], 0.3))
    rng = n.new("ShaderNodeMapRange")
    rng.inputs["From Min"].default_value = 0.3
    rng.inputs["From Max"].default_value = 0.7
    rng.inputs["To Min"].default_value = lo
    rng.inputs["To Max"].default_value = hi
    l.new(mix, rng.inputs["Value"])
    mul = n.new("ShaderNodeMix")
    mul.data_type = "RGBA"
    mul.blend_type = "MULTIPLY"
    mul.inputs["Factor"].default_value = 1.0
    mul.inputs["A"].default_value = srgb(color)
    l.new(rng.outputs["Result"], mul.inputs["B"])
    return mul.outputs["Result"], tc


def _displace(n, l, out, tc, scale, amount, mat):
    lump = n.new("ShaderNodeTexNoise")
    lump.inputs["Scale"].default_value = scale
    lump.inputs["Detail"].default_value = 3.0
    l.new(tc.outputs["Object"], lump.inputs["Vector"])
    disp = n.new("ShaderNodeDisplacement")
    disp.inputs["Scale"].default_value = amount
    l.new(lump.outputs["Fac"], disp.inputs["Height"])
    l.new(disp.outputs[0], out.inputs["Displacement"])
    try:
        mat.displacement_method = "BOTH"
    except AttributeError:
        mat.cycles.displacement_method = "BOTH"


def felt(name, color, fuzz=True, sheen=1.0):
    mat = bpy.data.materials.new(name)
    n, l, out = _nodes(mat)
    base, tc = _mottle(n, l, color)
    bsdf = n.new("ShaderNodeBsdfPrincipled")
    l.new(base, bsdf.inputs["Base Color"])
    l.new(base, bsdf.inputs["Sheen Tint"])
    bsdf.inputs["Roughness"].default_value = 1.0
    bsdf.inputs["Specular IOR Level"].default_value = 0.15
    bsdf.inputs["Sheen Weight"].default_value = sheen
    bsdf.inputs["Sheen Roughness"].default_value = 0.5
    bsdf.inputs["Subsurface Weight"].default_value = 0.08
    bsdf.inputs["Subsurface Radius"].default_value = (0.002, 0.0015, 0.001)
    fib = n.new("ShaderNodeTexNoise")
    fib.inputs["Scale"].default_value = 2800.0
    fib.inputs["Detail"].default_value = 12.0
    fib.inputs["Distortion"].default_value = 5.0
    l.new(tc.outputs["Object"], fib.inputs["Vector"])
    bump = n.new("ShaderNodeBump")
    bump.inputs["Strength"].default_value = 0.8
    bump.inputs["Distance"].default_value = 0.0004
    l.new(fib.outputs["Fac"], bump.inputs["Height"])
    l.new(bump.outputs["Normal"], bsdf.inputs["Normal"])
    l.new(bsdf.outputs[0], out.inputs["Surface"])
    _displace(n, l, out, tc, 38.0, 0.0007, mat)
    mat["fuzz"] = "felt" if fuzz else ""
    mat["subdiv"] = 1
    return mat


def knit(name, color, stitch=0.0056, displace=True):
    mat = L.knit_material(name, color, stitch=stitch, displace=displace)
    nt = mat.node_tree
    for nd in nt.nodes:
        if nd.bl_idname == "ShaderNodeDisplacement":
            nd.inputs["Scale"].default_value = stitch * 0.42
        if nd.bl_idname == "ShaderNodeBump":
            nd.inputs["Strength"].default_value = 0.85
    mat["fuzz"] = "yarn"
    return mat


def gummy(name, color):
    mat = L.gummy_material(name, color)
    bsdf = next(nd for nd in mat.node_tree.nodes if nd.bl_idname == "ShaderNodeBsdfPrincipled")
    bsdf.inputs["Roughness"].default_value = 0.3
    bsdf.inputs["Transmission Weight"].default_value = 0.45
    mat["sugar"] = 1
    return mat


def clay(name, color):
    mat = L.clay_material(name, color)
    n, l = mat.node_tree.nodes, mat.node_tree.links
    out = next(nd for nd in n if nd.bl_idname == "ShaderNodeOutputMaterial")
    tc = next(nd for nd in n if nd.bl_idname == "ShaderNodeTexCoord")
    for nd in n:
        if nd.bl_idname == "ShaderNodeBump":
            nd.inputs["Strength"].default_value = 1.0
            nd.inputs["Distance"].default_value = 0.0022
        if nd.bl_idname == "ShaderNodeTexWave":
            nd.inputs["Scale"].default_value = 150.0
    # prints weigh more
    for nd in n:
        if nd.bl_idname == "ShaderNodeMath" and nd.operation == "MULTIPLY" and abs(nd.inputs[1].default_value - 0.12) < 1e-6:
            nd.inputs[1].default_value = 0.4
    _displace(n, l, out, tc, 30.0, 0.0006, mat)
    mat["subdiv"] = 1
    return mat


def sugar_material():
    if "sugar" in bpy.data.materials:
        return bpy.data.materials["sugar"]
    mat = bpy.data.materials.new("sugar")
    n, l, out = _nodes(mat)
    b = n.new("ShaderNodeBsdfPrincipled")
    b.inputs["Base Color"].default_value = (1, 1, 1, 1)
    b.inputs["Transmission Weight"].default_value = 1.0
    b.inputs["Roughness"].default_value = 0.03
    b.inputs["IOR"].default_value = 1.55
    l.new(b.outputs[0], out.inputs["Surface"])
    return mat


def glow_material(color=(214, 248, 255)):
    mat = bpy.data.materials.new("glow")
    n, l, out = _nodes(mat)
    e = n.new("ShaderNodeEmission")
    e.inputs["Color"].default_value = srgb(color)
    e.inputs["Strength"].default_value = 6.0
    l.new(e.outputs[0], out.inputs["Surface"])
    return mat


TEXTURES = {"plush": felt, "knit": knit, "gummy": gummy, "clay": clay}


def eye_material(texture, color):
    if texture == "plush":
        return felt("eye", color, fuzz=False, sheen=0.35)
    if texture == "knit":
        # satin-stitched embroidery: tight parallel ridges
        mat = bpy.data.materials.new("eye_knit")
        n, l, out = _nodes(mat)
        tc = n.new("ShaderNodeTexCoord")
        wave = n.new("ShaderNodeTexWave")
        wave.inputs["Scale"].default_value = 900.0
        wave.inputs["Distortion"].default_value = 1.5
        l.new(tc.outputs["Object"], wave.inputs["Vector"])
        b = n.new("ShaderNodeBsdfPrincipled")
        b.inputs["Base Color"].default_value = srgb(color)
        b.inputs["Roughness"].default_value = 0.7
        b.inputs["Sheen Weight"].default_value = 0.15
        bump = n.new("ShaderNodeBump")
        bump.inputs["Strength"].default_value = 0.9
        bump.inputs["Distance"].default_value = 0.0003
        l.new(wave.outputs["Fac"], bump.inputs["Height"])
        l.new(bump.outputs["Normal"], b.inputs["Normal"])
        l.new(b.outputs[0], out.inputs["Surface"])
        return mat
    if texture == "gummy":
        mat = bpy.data.materials.new("eye_gummy")
        n, l, out = _nodes(mat)
        b = n.new("ShaderNodeBsdfPrincipled")
        b.inputs["Base Color"].default_value = srgb(color)
        b.inputs["Roughness"].default_value = 0.12
        b.inputs["Coat Weight"].default_value = 1.0
        b.inputs["Coat Roughness"].default_value = 0.03
        l.new(b.outputs[0], out.inputs["Surface"])
        return mat
    return L.clay_material("eye_clay", color)


# ---------------------------------------------------------------- surface dressing

def fuzz(obj, mat, kind):
    if kind == "felt":
        layers = [(7.0e7, 0.0013, 1.6e-5, 1.2), (1.4e6, 0.0045, 9e-6, 0.35)]
    else:
        layers = [(2.0e7, 0.0011, 8e-6, 0.5), (5.0e5, 0.0035, 7e-6, 0.3)]
    for i, (density, length, radius, lift) in enumerate(layers):
        hair = L.link(bpy.data.objects.new(f"{obj.name}_fz{i}", bpy.data.hair_curves.new(f"{obj.name}_fz{i}")))
        hair.data.materials.append(mat)
        mod = hair.modifiers.new("Fuzz", "NODES")
        mod.node_group = L.fuzz_tree()
        L.set_mod_input(mod, "Density", density)
        L.set_mod_input(mod, "Length", length)
        L.set_mod_input(mod, "Radius", radius)
        L.set_mod_input(mod, "Lift", lift)
        L.set_mod_input(mod, "Material", mat)
        L.set_mod_input(mod, "Surface", obj)


def sugar_tree():
    if "Sugar" in bpy.data.node_groups:
        return bpy.data.node_groups["Sugar"]
    tree = bpy.data.node_groups.new("Sugar", "GeometryNodeTree")
    iface = tree.interface
    iface.new_socket("Geometry", in_out="INPUT", socket_type="NodeSocketGeometry")
    iface.new_socket("Material", in_out="INPUT", socket_type="NodeSocketMaterial")
    iface.new_socket("Geometry", in_out="OUTPUT", socket_type="NodeSocketGeometry")
    n, l = tree.nodes, tree.links
    gin, gout = n.new("NodeGroupInput"), n.new("NodeGroupOutput")
    dist = n.new("GeometryNodeDistributePointsOnFaces")
    dist.inputs["Density"].default_value = 3.0e6
    l.new(gin.outputs["Geometry"], dist.inputs["Mesh"])
    cube = n.new("GeometryNodeMeshCube")
    cube.inputs["Size"].default_value = (1, 1, 1)
    setm = n.new("GeometryNodeSetMaterial")
    l.new(cube.outputs[0], setm.inputs["Geometry"])
    l.new(gin.outputs["Material"], setm.inputs["Material"])
    rot = n.new("FunctionNodeRandomValue")
    rot.data_type = "FLOAT_VECTOR"
    rot.inputs[0].default_value = (0, 0, 0)
    rot.inputs[1].default_value = (6.3, 6.3, 6.3)
    size = n.new("FunctionNodeRandomValue")
    size.data_type = "FLOAT"
    [s for s in size.inputs if s.name == "Min" and s.type == "VALUE"][0].default_value = 0.00012
    [s for s in size.inputs if s.name == "Max" and s.type == "VALUE"][0].default_value = 0.00030
    size.inputs["Seed"].default_value = 5
    inst = n.new("GeometryNodeInstanceOnPoints")
    l.new(dist.outputs["Points"], inst.inputs["Points"])
    l.new(setm.outputs[0], inst.inputs["Instance"])
    e2r = n.new("FunctionNodeEulerToRotation")
    l.new([o for o in rot.outputs if o.type == "VECTOR"][0], e2r.inputs[0])
    l.new(e2r.outputs[0], inst.inputs["Rotation"])
    l.new([o for o in size.outputs if o.type == "VALUE"][0], inst.inputs["Scale"])
    join = n.new("GeometryNodeJoinGeometry")
    l.new(gin.outputs["Geometry"], join.inputs[0])
    l.new(inst.outputs["Instances"], join.inputs[0])
    l.new(join.outputs[0], gout.inputs["Geometry"])
    return tree


def sugar(obj):
    mod = obj.modifiers.new("Sugar", "NODES")
    mod.node_group = sugar_tree()
    L.set_mod_input(mod, "Material", sugar_material())


# ---------------------------------------------------------------- eyes

def front_hit(obj, x, z):
    deps = bpy.context.evaluated_depsgraph_get()
    ev = obj.evaluated_get(deps)
    inv = obj.matrix_world.inverted()
    o = inv @ Vector((x, -1.0, z))
    d = (inv.to_3x3() @ Vector((0, 1, 0))).normalized()
    hit, loc, nrm, _ = ev.ray_cast(o, d)
    if not hit:
        return Vector((x, 0, z)), Vector((0, -1, 0))
    return obj.matrix_world @ loc, (obj.matrix_world.to_3x3() @ nrm).normalized()


def eye_object(name, loc, normal, w, h, depth):
    bm = L.sphere_bm(1.0, 48, 24)
    for v in bm.verts:
        v.co.x *= w / 2
        v.co.z *= h / 2
        v.co.y *= depth
    obj = L.mesh_object(name, bm)
    q = (-normal).to_track_quat("Y", "Z")
    obj.rotation_euler = q.to_euler()
    obj.location = loc + normal * depth * 0.25
    return obj


# ---------------------------------------------------------------- head items
# All sized for the bean (r = 0.03, top at z = 0.03).

def item_helmet(r=0.03):
    helmet = L.helmet_object(
        "helmet", L.scaled_bm(lambda: L.bean_bm(r), 1.06, 1.08, 1.06, 0.001),
        thickness=0.0028, eye_z=0.006, slit_w=0.036, slit_h=0.012, mouth_w=0.010,
        bottom_z=-0.012, back_z=-0.004)
    crest = L.crest_object("crest", scale=0.85, width=0.0055, origin=(0, 0.0, 0.0285), tilt=0.12)
    return {"item": [helmet], "item2": [crest]}


def _shell_cap(r, cut_z, grow=1.07):
    obj = L.mesh_object("cap", L.scaled_bm(lambda: L.bean_bm(r), grow, grow * 1.02, grow, 0.0005))
    so = obj.modifiers.new("t", "SOLIDIFY")
    so.thickness = 0.0024
    so.offset = 1.0
    L.apply_modifiers(obj)
    L.boolean(obj, L.box_object("cut", (-0.2, -0.2, -0.2), (0.2, 0.2, cut_z)))
    bv = obj.modifiers.new("b", "BEVEL")
    bv.width = 0.001
    bv.segments = 4
    bv.limit_method = "ANGLE"
    return obj


def _ring(name, z, rx, ry, minor, segs=96):
    bm = bmesh.new()
    bmesh.ops.create_circle(bm, cap_ends=False, radius=1.0, segments=segs)
    obj = L.mesh_object(name, bm)
    obj.scale = (rx, ry, 1)
    obj.location = (0, 0, z)
    sk = bpy.data.curves.new(name + "_c", "CURVE")
    sk.dimensions = "3D"
    sp = sk.splines.new("NURBS")
    sp.points.add(segs - 1)
    for i in range(segs):
        a = 2 * math.pi * i / segs
        sp.points[i].co = (rx * math.cos(a), ry * math.sin(a), 0, 1)
    sp.use_cyclic_u = True
    sp.order_u = 4
    sk.bevel_depth = minor
    sk.bevel_resolution = 8
    cob = L.link(bpy.data.objects.new(name, sk))
    cob.location = (0, 0, z)
    bpy.data.objects.remove(obj)
    deps = bpy.context.evaluated_depsgraph_get()
    me = bpy.data.meshes.new_from_object(cob.evaluated_get(deps))
    mobj = L.link(bpy.data.objects.new(name, me))
    mobj.location = cob.location
    bpy.data.objects.remove(cob)
    for p in me.polygons:
        p.use_smooth = True
    return mobj


def item_beanie(r=0.03):
    cap = _shell_cap(r, 0.0185)
    brim = _ring("brim", 0.0192, 0.0272, 0.0240, 0.0038)
    pom = L.mesh_object("pom", L.sphere_bm(0.0075, 48, 24))
    pom.location = (0, 0, 0.0375)
    return {"item": [cap, brim], "item2": [pom]}


def item_cap(r=0.03):
    cap = _shell_cap(r, 0.0175)
    bm = bmesh.new()
    bmesh.ops.create_cone(bm, cap_ends=True, segments=64, radius1=0.018, radius2=0.018, depth=0.0022)
    brim = L.mesh_object("brim", bm)
    brim.scale = (1.0, 0.75, 1.0)
    brim.location = (0, -0.0255, 0.0182)
    brim.rotation_euler = (math.radians(-8), 0, 0)
    bv = brim.modifiers.new("b", "BEVEL")
    bv.width = 0.0009
    bv.segments = 4
    btn = L.mesh_object("btn", L.sphere_bm(0.0028, 24, 12))
    btn.location = (0, 0, 0.0335)
    return {"item": [cap, brim], "item2": [btn]}


def item_crown(r=0.03):
    bm = bmesh.new()
    bmesh.ops.create_cone(bm, cap_ends=False, segments=64, radius1=0.0165, radius2=0.0175, depth=0.007)
    band = L.mesh_object("band", bm)
    band.location = (0, 0, 0.0285)
    so = band.modifiers.new("t", "SOLIDIFY")
    so.thickness = 0.0022
    bv = band.modifiers.new("b", "BEVEL")
    bv.width = 0.0008
    bv.segments = 3
    spikes, gems = [], []
    for i in range(5):
        a = 2 * math.pi * i / 5 + math.pi / 2
        bm = bmesh.new()
        bmesh.ops.create_cone(bm, cap_ends=True, segments=24, radius1=0.0048, radius2=0.0004, depth=0.010)
        sp = L.mesh_object(f"spike{i}", bm)
        sp.location = (0.0168 * math.cos(a), 0.0168 * math.sin(a) * 0.9, 0.0365)
        sp.rotation_euler = (0, 0, 0)
        spikes.append(sp)
        g = L.mesh_object(f"tip{i}", L.sphere_bm(0.0022, 24, 12))
        g.location = (sp.location.x, sp.location.y, 0.042)
        gems.append(g)
    return {"item": [band] + spikes, "item2": gems}


def item_headphones(r=0.03):
    bm = bmesh.new()
    # band: half ring in the x-z plane
    sk = bpy.data.curves.new("band", "CURVE")
    sk.dimensions = "3D"
    sp = sk.splines.new("NURBS")
    pts = 24
    sp.points.add(pts - 1)
    for i in range(pts):
        a = math.pi * i / (pts - 1)
        sp.points[i].co = (0.0335 * math.cos(a), 0.0, 0.0335 * math.sin(a) * 1.02, 1)
    sp.order_u = 4
    sp.use_endpoint_u = True
    sk.bevel_depth = 0.0026
    sk.bevel_resolution = 6
    sk.use_fill_caps = True
    cob = L.link(bpy.data.objects.new("band", sk))
    deps = bpy.context.evaluated_depsgraph_get()
    me = bpy.data.meshes.new_from_object(cob.evaluated_get(deps))
    band = L.link(bpy.data.objects.new("band_m", me))
    bpy.data.objects.remove(cob)
    bm.free()
    cups = []
    for sx in (-1, 1):
        bm = bmesh.new()
        bmesh.ops.create_cone(bm, cap_ends=True, segments=48, radius1=0.0095, radius2=0.0095, depth=0.0075)
        c = L.mesh_object(f"cup{sx}", bm)
        c.rotation_euler = (0, math.radians(90), 0)
        c.location = (sx * 0.0325, 0.0, 0.002)
        bv = c.modifiers.new("b", "BEVEL")
        bv.width = 0.0022
        bv.segments = 6
        cups.append(c)
    for p in me.polygons:
        p.use_smooth = True
    return {"item": [band], "item2": cups}


ITEMS = {
    "none": lambda r=0.03: {},
    "helmet": item_helmet,
    "beanie": item_beanie,
    "cap": item_cap,
    "crown": item_crown,
    "headphones": item_headphones,
}


# ---------------------------------------------------------------- dressing

def dress(parts, eyes, eye_style, texture, colors):
    make = TEXTURES[texture]
    mats = {}
    for part, objs in parts.items():
        for obj in objs:
            if part == "void":
                mat = L.void_material()
            else:
                key = part
                if key not in mats:
                    col = colors.get(part) or colors["body"]
                    if texture == "knit" and part in ("item2", "feet", "crest"):
                        mats[key] = knit(f"knit_{part}", col, stitch=0.0032, displace=False)
                    else:
                        mats[key] = make(f"{texture}_{part}", col)
                mat = mats[key]
            obj.data.materials.clear()
            obj.data.materials.append(mat)
            if part != "void":
                if mat.get("subdiv"):
                    L.subsurf(obj, 1 if texture != "knit" else 2)
                if mat.get("fuzz"):
                    fuzz(obj, mat, mat["fuzz"])
                if mat.get("sugar"):
                    sugar(obj)
    eye_col = colors.get("eyes", EYE_BLACK)
    emat = glow_material() if eye_style == "glow" else eye_material(texture, eye_col)
    for i, (loc, nrm, w, h) in enumerate(eyes):
        e = eye_object(f"eye{i}", loc, nrm, w, h, 0.0011 if eye_style != "glow" else 0.0006)
        e.data.materials.append(emat)
        if texture == "plush" and eye_style != "glow":
            fuzz(e, emat, "yarn")
