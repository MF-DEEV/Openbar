"""Product GIF set, round 2: the real MacBook on a cracked stone table by the
campfire, Sketchfab knights, Poly Haven rock/ground scans, the banner's
mountains, castle and helmet moon behind.

  blender -b --factory-startup -P product.py -- OUTDIR SPP SHOT [START END]
SHOT: wide | close | anim (anim keys the push-in over N frames)
"""
import math
import os
import random
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import rlib as R  # noqa: E402
from rlib import K  # noqa: E402
import bmesh  # noqa: E402
import bpy  # noqa: E402
from mathutils import Matrix, Vector  # noqa: E402

a = K.args()
OUT, SPP, SHOT = a[0], int(a[1]), a[2]
START = int(a[3]) if len(a) > 3 else 1
END = int(a[4]) if len(a) > 4 else START
W, H = int(os.environ.get("RW", "1600")), int(os.environ.get("RH", "900"))
N = int(os.environ.get("NFRAMES", "150"))
FPS = int(os.environ.get("FPS", "30"))
K.FPS = FPS
_tn = K.time_node
K.time_node = lambda nt, rate=None: _tn(nt, (rate if rate is not None else K.TIME_RATE) * 30.0 / FPS)
PH = os.path.join(R.ROOT, "assets", "polyhaven")
SF = os.path.join(R.ROOT, "assets", "sketchfab")
MACBOOK = os.path.join(R.ROOT, "..", "feature", "blender", "assets", "macbook-src", "source", "macbook_pro_14_silver.usdz")
MACTEX = os.path.join(R.ROOT, "..", "feature", "blender", "assets", "macbook-tex")
SCREEN_IMG = os.environ.get("SCREEN_IMG", os.path.join(R.ROOT, "assets", "island", "screen.png"))
SCREEN_SEQ = os.environ.get("SCREEN_SEQ")          # first frame of an image sequence
rr = random.Random(11)


# ---------------------------------------------------------------- scanned materials
def scan_mat(name, tex, scale=1.0, tint="#FFFFFF", bump=1.0, rough_k=1.0, sat=1.0, val=1.0, moss=0.0):
    """Poly Haven PBR set, box-projected in object space, tinted for the
    night; optional moss creeping over the tops."""
    d = os.path.join(PH, "tex", tex)
    m, nt, out = K.mat(name)
    tc = nt.nodes.new("ShaderNodeTexCoord")
    mp = nt.nodes.new("ShaderNodeMapping")
    mp.inputs["Scale"].default_value = (scale, scale, scale)
    nt.links.new(tc.outputs["Object"], mp.inputs["Vector"])

    def img(fname, cs):
        n = nt.nodes.new("ShaderNodeTexImage")
        n.image = bpy.data.images.load(os.path.join(d, fname), check_existing=True)
        n.image.colorspace_settings.name = cs
        n.projection = "BOX"
        n.projection_blend = 0.3
        nt.links.new(mp.outputs[0], n.inputs["Vector"])
        return n
    fn = {f.split(".")[0]: f for f in os.listdir(d)}
    diff = img(fn["diff"], "sRGB")
    rough = img(fn["rough"], "Non-Color")
    nor = img(fn["nor"], "Non-Color")
    hsv = nt.nodes.new("ShaderNodeHueSaturation")
    hsv.inputs["Saturation"].default_value = sat
    hsv.inputs["Value"].default_value = val
    nt.links.new(diff.outputs["Color"], hsv.inputs["Color"])
    col = K.mix_rgb(nt, "MULTIPLY", hsv.outputs["Color"], vb=R.srgb(tint))
    if moss:
        geo = nt.nodes.new("ShaderNodeNewGeometry")
        sep = nt.nodes.new("ShaderNodeSeparateXYZ")
        nt.links.new(geo.outputs["Normal"], sep.inputs[0])
        mn = K.noise_tex(nt, 3.0, 6, 1.0, 0.6, tc.outputs["Object"])
        mk = K.math_node(nt, "MULTIPLY", K.math_node(nt, "SUBTRACT", sep.outputs["Z"], vb=0.45), vb=2.2)
        mk = K.math_node(nt, "MULTIPLY", K.math_node(nt, "MAXIMUM", mk, vb=0.0),
                         K.math_node(nt, "GREATER_THAN", mn.outputs["Fac"], vb=0.47))
        mr = K.ramp(nt, [(0.0, "#0A1A08"), (0.5, "#24401A"), (1.0, "#4E7A30")])
        nt.links.new(mn.outputs["Fac"], mr.inputs["Fac"])
        col = K.mix_rgb(nt, "MIX", col, mr.outputs["Color"], fac=K.math_node(nt, "MINIMUM", K.math_node(nt, "MULTIPLY", mk, vb=moss), vb=1.0))
    b = nt.nodes.new("ShaderNodeBsdfPrincipled")
    nt.links.new(col, b.inputs["Base Color"])
    nt.links.new(K.math_node(nt, "MULTIPLY", rough.outputs["Color"], vb=rough_k), b.inputs["Roughness"])
    nm = nt.nodes.new("ShaderNodeNormalMap")
    nm.inputs["Strength"].default_value = bump
    nt.links.new(nor.outputs["Color"], nm.inputs["Color"])
    nt.links.new(nm.outputs["Normal"], b.inputs["Normal"])
    nt.links.new(b.outputs[0], out.inputs["Surface"])
    return m


def disp_texture(tex):
    d = os.path.join(PH, "tex", tex)
    f = [x for x in os.listdir(d) if x.startswith("disp")][0]
    t = bpy.data.textures.new(tex + "_disp", "IMAGE")
    t.image = bpy.data.images.load(os.path.join(d, f), check_existing=True)
    t.image.colorspace_settings.name = "Non-Color"
    return t


# ---------------------------------------------------------------- imports
def import_gltf(path, height=None, loc=(0, 0, 0), yaw=0.0, scale=None, name=None):
    """Imports a glTF/GLB under one empty, bottom on z=0 at `loc`, scaled to
    `height` (or by `scale`)."""
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=path)
    new = [o for o in bpy.data.objects if o not in before]
    root = K.link(bpy.data.objects.new(name or os.path.basename(path), None))
    for o in new:
        if o.parent is None:
            o.parent = root
    bpy.context.view_layer.update()
    meshes = [o for o in new if o.type == "MESH"]
    lo = Vector((1e9,) * 3)
    hi = Vector((-1e9,) * 3)
    for o in meshes:
        for c in o.bound_box:
            w = o.matrix_world @ Vector(c)
            lo = Vector(map(min, lo, w))
            hi = Vector(map(max, hi, w))
    k = scale if scale else (height / (hi.z - lo.z) if height else 1.0)
    for o in new:
        if o.parent is root:
            o.location = (o.location - Vector(((lo.x + hi.x) / 2, (lo.y + hi.y) / 2, lo.z))) if False else o.location
    root.scale = (k, k, k)
    centre = Vector(((lo.x + hi.x) / 2, (lo.y + hi.y) / 2, lo.z))
    for o in new:
        if o.parent is root:
            o.location -= centre
    root.location = Vector(loc)
    root.rotation_euler.z = math.radians(yaw)
    for o in meshes:
        o.visible_shadow = True
    return root, meshes


def ph_model(mid, **kw):
    d = os.path.join(PH, "models", mid)
    f = [x for x in os.listdir(d) if x.endswith(".gltf")][0]
    return import_gltf(os.path.join(d, f), name=mid, **kw)


def facing_yaw(src, dst):
    v = Vector(dst) - Vector(src)
    return math.degrees(math.atan2(v.x, -v.y))     # model front assumed toward -Y


# ---------------------------------------------------------------- the laptop
def screen_material():
    m, nt, out = K.mat("ScreenImg")
    tc = nt.nodes.new("ShaderNodeTexCoord")
    im = nt.nodes.new("ShaderNodeTexImage")
    if SCREEN_SEQ:
        im.image = bpy.data.images.load(SCREEN_SEQ)
        im.image.source = "SEQUENCE"
        im.image_user.frame_duration = int(os.environ.get("SCREEN_LEN", "150"))
        im.image_user.frame_start = 1
        im.image_user.use_auto_refresh = True
    else:
        im.image = bpy.data.images.load(SCREEN_IMG)
    im.interpolation = "Cubic"
    nt.links.new(tc.outputs["UV"], im.inputs["Vector"])
    e = nt.nodes.new("ShaderNodeEmission")
    nt.links.new(im.outputs["Color"], e.inputs["Color"])
    e.inputs["Strength"].default_value = 1.15
    gl = nt.nodes.new("ShaderNodeBsdfGlossy")
    gl.inputs["Roughness"].default_value = 0.04
    mx = nt.nodes.new("ShaderNodeMixShader")
    mx.inputs["Fac"].default_value = 0.025
    nt.links.new(e.outputs[0], mx.inputs[1])
    nt.links.new(gl.outputs[0], mx.inputs[2])
    nt.links.new(mx.outputs[0], out.inputs["Surface"])
    return m


def macbook(loc):
    before = set(bpy.data.objects)
    bpy.ops.wm.usd_import(filepath=MACBOOK)
    new = [o for o in bpy.data.objects if o not in before]
    lap_mats = {sl.material for o in new if o.type == "MESH" for sl in o.material_slots if sl.material}
    for mat in lap_mats:
        if not mat.use_nodes:
            continue
        for n in mat.node_tree.nodes:
            if n.bl_idname == "ShaderNodeTexImage" and n.image is not None:
                stem = os.path.splitext(n.image.name)[0]
                for ext in (".png", ".jpg", ".jpeg"):
                    p = os.path.join(MACTEX, stem + ext)
                    if os.path.exists(p):
                        cs = n.image.colorspace_settings.name
                        n.image = bpy.data.images.load(p, check_existing=True)
                        n.image.colorspace_settings.name = cs
                        break
        for b in [n for n in mat.node_tree.nodes if n.bl_idname == "ShaderNodeBsdfPrincipled"]:
            for nm in ("Emission Color", "Emission Strength"):
                for l_ in list(b.inputs[nm].links):
                    mat.node_tree.links.remove(l_)
            b.inputs["Emission Strength"].default_value = 0.0
    # space grey: darker aluminium so it sits in the night
    for nm_ in ("hPcehRUjcLAosED", "zqeFZcIteZtOShc", "pZbDFXVUkfRwjmQ"):
        mat = bpy.data.materials.get(nm_)
        if mat and mat.use_nodes:
            b = next(n for n in mat.node_tree.nodes if n.bl_idname == "ShaderNodeBsdfPrincipled")
            for l_ in list(b.inputs["Base Color"].links):
                mat.node_tree.links.remove(l_)
            b.inputs["Base Color"].default_value = (0.16, 0.165, 0.18, 1)
            b.inputs["Roughness"].default_value = 0.32
    names = [o.name for o in new]
    for nm_ in ("FnbkdmFKVeCCxTX", "rQNJSUqpGWbPRJz"):
        o = bpy.data.objects.get(nm_)
        if o:
            bpy.data.objects.remove(o, do_unlink=True)
    new = [bpy.data.objects[n] for n in names if n in bpy.data.objects]
    root = K.link(bpy.data.objects.new("MacBook", None))
    for o in new:
        if o.parent is None:
            o.parent = root
    root.location = Vector(loc) + Vector((0, 0, 0.014))
    bpy.context.view_layer.update()
    scr = bpy.data.objects["abgVijaHVNRUvcc"]
    vs = [scr.matrix_world @ v.co for v in scr.data.vertices]
    xs = [v.x for v in vs]
    top = max(vs, key=lambda v: v.z)
    bot = min(vs, key=lambda v: v.z)
    up = Vector((0, top.y - bot.y, top.z - bot.z)).normalized()
    normal = Vector((0, -up.z, up.y))
    origin = Vector((0, bot.y, bot.z))
    vv = [(v - origin).dot(up) for v in vs]
    dd = [(v - origin).dot(normal) for v in vs]
    plane_o = origin + normal * (max(dd) + 0.0025)
    x0, x1, v1 = min(xs), max(xs), max(vv)
    v0 = v1 - (x1 - x0) * 1964 / 3024
    bm = bmesh.new()
    uv = bm.loops.layers.uv.new()
    cs = [plane_o + up * v0 + Vector((x0, 0, 0)), plane_o + up * v0 + Vector((x1, 0, 0)),
          plane_o + up * v1 + Vector((x1, 0, 0)), plane_o + up * v1 + Vector((x0, 0, 0))]
    f = bm.faces.new([bm.verts.new(c) for c in cs])
    for loop, uvv in zip(f.loops, ((0, 0), (1, 0), (1, 1), (0, 1))):
        loop[uv].uv = uvv
    ob = K.mesh_object("ScreenImage", bm, screen_material(), smooth=False)
    ob.visible_shadow = False
    scr.hide_render = True
    glass = bpy.data.objects.get("fiqlelggeOoTUAw")      # cover glass: reflects the sky over the picture
    if glass:
        glass.hide_render = True
    xc = Vector(((x0 + x1) / 2, 0, 0))
    notch = plane_o + up * (v1 - 0.004) + xc
    centre = plane_o + up * (v0 + v1) / 2 + xc
    # the screen lights the keyboard and the table a little
    sg = K.area("ScreenGlow", centre + normal * 0.08, centre + normal * 1.0, 0.28, 0.7, R.srgb("#7A84C0")[:3])
    sg.visible_camera = False
    sg.visible_glossy = False
    return notch, centre, normal


# ---------------------------------------------------------------- round 3: trees, candle tips, idles
def gnarled_tree(name, base, height, lean, bark, seed=0.0, limbs=9):
    """An old dead tree: flared roots, a twisting knotted trunk, crooked
    limbs with twigs; dense rings so it never reads as flat facets; scanned
    bark with real displacement."""
    rr_ = random.Random(int(seed * 100) + 9)
    b = Vector(base)
    bm = bmesh.new()
    pts, rad = [], []
    n = 30
    for i in range(n):
        t = i / (n - 1)
        bend = Vector((lean * t * t * height * 0.35 + 0.45 * K.fbm(t * 2.5, seed, 0, 3),
                       0.35 * K.fbm(seed, t * 2.5, 1, 3), t * height))
        pts.append(b + bend)
        r0 = height * 0.075 * (1.0 - 0.75 * t) ** 1.1
        r0 *= 1 + 1.4 * max(0.0, 0.12 - t) / 0.12          # root flare
        r0 *= 1 + 0.06 * math.sin(t * 17 + seed) * (1 - t)   # knots
        rad.append(r0)
    K.sweep(pts, rad, segs=24, crumple=0.0, seed=seed, bm=bm, twist=0.06)
    # roots crawling out
    for k in range(6):
        a = 2 * math.pi * k / 6 + rr_.uniform(-0.3, 0.3)
        d = Vector((math.cos(a), math.sin(a), 0))
        rp = [b + d * height * 0.03 * j + Vector((0, 0, height * 0.05 * (1 - j / 6) - 0.05 * j / 6)) for j in range(7)]
        K.sweep(rp, [height * 0.035 * (1 - j / 7) + 0.01 for j in range(7)], segs=10, crumple=0.3, seed=seed + k, bm=bm)
    for k in range(limbs):
        i0 = rr_.randint(int(n * 0.45), n - 3)
        p0 = pts[i0]
        a = rr_.uniform(0, 2 * math.pi)
        dirn = Vector((math.cos(a), math.sin(a) * 0.7, rr_.uniform(0.15, 0.85))).normalized()
        L = height * rr_.uniform(0.25, 0.5)
        lp = []
        for j in range(12):
            t = j / 11
            lp.append(p0 + dirn * L * t + Vector((0.18 * L * K.fbm(t * 3, k, seed, 2), 0.12 * L * K.fbm(k, t * 3, seed, 2),
                                                   -0.25 * L * t * t)))
        K.sweep(lp, [rad[i0] * 0.75 * (1 - j / 12) ** 1.1 + 0.008 * height for j in range(12)], segs=12, crumple=0.15,
                seed=seed + k, bm=bm)
        for q in range(rr_.randint(2, 4)):
            j0 = rr_.randint(4, 9)
            a2 = rr_.uniform(0, 2 * math.pi)
            d2 = (dirn + Vector((math.cos(a2), math.sin(a2), rr_.uniform(-0.1, 0.7)))).normalized()
            L2 = L * rr_.uniform(0.25, 0.45)
            tp = [lp[j0] + d2 * L2 * (m / 6) + Vector((0.1 * L2 * K.fbm(m * 0.5, q, seed, 2), 0, -0.12 * L2 * (m / 6) ** 2))
                  for m in range(7)]
            K.sweep(tp, [rad[i0] * 0.18 * (1 - m / 7) + 0.003 * height for m in range(7)], segs=7, crumple=0.3,
                    seed=seed + k * 7 + q, bm=bm)
    ob = K.mesh_object(name, bm, bark)
    sub = ob.modifiers.new("sub", "SUBSURF")
    sub.levels = sub.render_levels = 1
    dm = ob.modifiers.new("disp", "DISPLACE")
    dm.texture = disp_texture("knotted_pine_bark")
    dm.texture_coords = "OBJECT"
    dm.strength = height * 0.012
    lump = ob.modifiers.new("lump", "DISPLACE")
    ct = bpy.data.textures.new(name + "_lump", "CLOUDS")
    ct.noise_scale = height * 0.08
    ct.noise_depth = 3
    lump.texture = ct
    lump.texture_coords = "OBJECT"
    lump.strength = height * 0.02
    lump.mid_level = 0.5
    return ob


def candle_tips(meshes, min_up=0.02):
    """World positions of candle tips: vertices that are the highest in their
    1 cm neighbourhood and stand above the holder."""
    cells = {}
    allz = []
    for o in meshes:
        mw = o.matrix_world
        for v in o.data.vertices:
            w = mw @ v.co
            allz.append(w.z)
            key = (round(w.x / 0.006), round(w.y / 0.006))
            if key not in cells or w.z > cells[key].z:
                cells[key] = w
    zmin = min(allz)
    tips = []
    for (i, j), w in cells.items():
        if w.z < zmin + min_up:
            continue
        if all(cells.get((i + di, j + dj), Vector((0, 0, -1e9))).z <= w.z
               for di in range(-3, 4) for dj in range(-3, 4) if (di, dj) != (0, 0)):
            if not any((w - t).length < 0.012 for t in tips):
                tips.append(w)
    return tips


def idle_static(root, seed, frames, breath=0.006, sway=0.5):
    """Unrigged figures: breathing (vertical scale) and a slow sway."""
    for f in range(1, frames + 1, 3):
        t = f / float(FPS)
        k = 1 + breath * math.sin(t * 2 * math.pi / 3.4 + seed)
        root.scale = tuple(c for c in root.scale[:2]) + (root.scale[0] * k,)
        root.keyframe_insert("scale", frame=f)
        root.rotation_euler.x = math.radians(sway * 0.4 * math.sin(t * 0.9 + seed))
        root.rotation_euler.y = math.radians(sway * math.sin(t * 0.6 + seed * 2))
        root.keyframe_insert("rotation_euler", frame=f)


def twist_upper(meshes, frames, seed, deg=5.0, lo=0.6):
    """A head/shoulder turn for an unrigged one-mesh figure: SimpleDeform
    TWIST limited to the upper part, keyed slowly."""
    for o in meshes:
        md = o.modifiers.new("idle", "SIMPLE_DEFORM")
        md.deform_method = "TWIST"
        md.deform_axis = "Z"
        md.limits = (lo, 1.0)
        for f in range(1, frames + 1, 3):
            t = f / float(FPS)
            md.angle = math.radians(deg * math.sin(t * 0.55 + seed) + deg * 0.3 * math.sin(t * 1.7 + seed))
            o.keyframe_insert(f'modifiers["idle"].angle', frame=f)


def idle_rig(root, frames, seed):
    """Rigged knight: breathing chest, slow head look, shoulder settle."""
    arm = next((c for c in root.children_recursive if c.type == "ARMATURE"), None)
    if not arm:
        return
    pb = arm.pose.bones
    want = {k: [b for b in pb if k.lower() in b.name.lower()] for k in ("upperSpine", "core", "neck", "head", "clavicle")}
    for b in pb:
        b.rotation_mode = "XYZ"
    for f in range(1, frames + 1, 3):
        t = f / float(FPS)
        br = math.sin(t * 2 * math.pi / 3.6 + seed)
        for b in want["upperSpine"] + want["core"]:
            b.rotation_euler = (math.radians(1.2 * br), 0, math.radians(1.0 * math.sin(t * 0.5 + seed)))
            b.keyframe_insert("rotation_euler", frame=f)
        for b in want["neck"] + want["head"]:
            b.rotation_euler = (math.radians(2.0 * math.sin(t * 0.7 + seed)), 0, math.radians(9.0 * math.sin(t * 0.45 + seed + 1)))
            b.keyframe_insert("rotation_euler", frame=f)
        for b in want["clavicle"]:
            b.rotation_euler = (0, 0, math.radians(1.5 * br))
            b.keyframe_insert("rotation_euler", frame=f)


def flicker_light(light, base, frames, seed):
    for f in range(1, frames + 1):
        light.data.energy = base * (0.82 + 0.3 * (K.fbm(f * 0.21, seed, 0, 2) + 0.5))
        light.data.keyframe_insert("energy", frame=f)


def sf_tree(fname, part, height, loc, yaw):
    """One tree from a Sketchfab pack: keep only `part` (None = all), stand
    it on z=0 at `loc`, `height` tall, night-tinted."""
    root, meshes = import_gltf(os.path.join(SF, fname), name="T_" + fname)
    keep = [o for o in meshes if not part or o.name.startswith(part)]
    for o in [o for o in meshes if o not in keep]:
        bpy.data.objects.remove(o, do_unlink=True)
    bpy.context.view_layer.update()
    lo = Vector((1e9,) * 3)
    hi = Vector((-1e9,) * 3)
    for o in keep:
        for c in o.bound_box:
            w = o.matrix_world @ Vector(c)
            lo = Vector(map(min, lo, w))
            hi = Vector(map(max, hi, w))
    k = height / (hi.z - lo.z)
    holder = K.link(bpy.data.objects.new("Tree_" + str(part), None))
    holder.location = (-(lo.x + hi.x) / 2, -(lo.y + hi.y) / 2, -lo.z)
    root.parent = holder
    top = K.link(bpy.data.objects.new("TreeTop_" + str(part), None))
    holder.parent = top
    top.scale = (k, k, k)
    top.location = Vector(loc)
    top.rotation_euler.z = math.radians(yaw)
    for o in keep:
        for sl in o.material_slots:
            m = sl.material
            if m and m.use_nodes and not m.get("tinted"):
                m["tinted"] = True
                b = next((n for n in m.node_tree.nodes if n.bl_idname == "ShaderNodeBsdfPrincipled"), None)
                if b and b.inputs["Base Color"].is_linked:
                    src = b.inputs["Base Color"].links[0].from_socket
                    mix = K.mix_rgb(m.node_tree, "MULTIPLY", src, vb=R.srgb("#7078C0"))
                    m.node_tree.links.new(mix, b.inputs["Base Color"])
    return top


# ---------------------------------------------------------------- set
def build():
    sky = R.sky((0, 175, 26), (460, 180), strength=0.85, scale=(0.012, 0.03, 1.0), flow=(0.11, 0.0), drift=0.03)
    R.moon2((-10, 150, 30), 10.0, glow=0.9)
    far = R.strata_rock("RockFar", scale=0.12, base=("#000002", "#03030E", "#080820", "#121236", "#24225A"), glow=0.05)
    mid = R.strata_rock("RockMid", scale=0.25, base=("#010103", "#050516", "#0E0E30", "#1E1C4C", "#3A3880"), glow=0.02)
    R.mountains("RangeFar", -170, 170, 80, 130, 26, far, seed=1.0, nx=420, ny=50, base_z=-6)
    R.mountains("RangeMid", -110, 110, 40, 72, 13, mid, seed=4.0, nx=360, ny=50, base_z=-5, sharp=1.9)
    near = R.strata_rock("RockNear", scale=0.5)
    R.crag_cliff("CastleCliff", (9, 40, 6.0), 9, 7, 16, near, seed=2.0)
    R.castle((9, 40, 6.0), s=0.6, seed=4, stone=R.strata_rock("CastleRock", scale=1.2, base=("#020208", "#0A0A22", "#181840", "#2C2A66", "#4A48A0")))
    water = R.water_mat("Water")
    for x, w_, h_ in ((5.6, 1.8, 14), (13.0, 1.4, 13)):
        R.waterfall(f"Fall{x}", (x, 36.6, 6.0), w_, h_, water, seed=x, lean=1.0)

    # ground: scanned forest floor with real displacement, then a few big undulations
    ground_m = scan_mat("Ground", "brown_mud_leaves_01", scale=0.3, tint="#6A70D8", bump=1.4, sat=0.8, val=0.5, moss=0.8)
    bm = bmesh.new()
    bmesh.ops.create_grid(bm, x_segments=320, y_segments=260, size=1.0)
    bmesh.ops.scale(bm, vec=(16, 13, 1), verts=bm.verts)
    bmesh.ops.translate(bm, vec=(0, 9, 0), verts=bm.verts)
    for v in bm.verts:
        v.co.z = 0.18 * K.fbm(v.co.x * 0.25, v.co.y * 0.25, 0.4, 4) + 0.6 * R.ss_(6, 22, v.co.y) * K.fbm(v.co.x * 0.08, v.co.y * 0.08, 1.0, 3)
    gr = K.mesh_object("Ground", bm, ground_m)
    dm = gr.modifiers.new("disp", "DISPLACE")
    dm.texture = disp_texture("brown_mud_leaves_01")
    dm.texture_coords = "OBJECT"
    dm.strength = 0.09
    dm.mid_level = 0.5
    sub = gr.modifiers.new("sub", "SUBSURF")
    sub.levels = sub.render_levels = 1
    gr.modifiers.move(1, 0)
    # far ground plane to the mountains
    R.ground("FarGround", (300, 80), scan_mat("FarGround", "rocks_ground_02", scale=0.05, tint="#2A2E70", sat=0.5, val=0.16),
             amp=1.5, z=-0.3, centre=(0, 60))

    # the cracked stone table
    rock = scan_mat("TableRock", "rock_face_03", scale=1.6, tint="#A0A8E8", bump=1.6, sat=0.5, val=0.9, moss=1.0)
    top = R.cliff("TableTop", (0, 0.0, 0.8), 1.35, 0.85, 0.14, rock, seed=1.0, sub=5)
    for x in (-0.46, 0.46):
        R.cliff(f"TableLeg{x}", (x, 0.02, 0.68), 0.3, 0.5, 0.68, rock, seed=2.0 + x, sub=4)
    notch, centre, nrm = macbook((0.05, -0.12, 0.8))
    # candles in brass holders on the table
    cr, cm = ph_model("brass_candleholders", height=0.22, loc=(-0.48, 0.12, 0.8), yaw=15)
    bpy.context.view_layer.update()
    cflame = [(0.0, "#000000"), (0.1, "#401000"), (0.3, "#FF7A20"), (0.6, "#FFD080"), (0.9, "#FFF8E0")]
    for i, tip in enumerate(candle_tips(cm)):
        R.fire(tip + Vector((0, 0, 0.002)), 0.022, 0.05, (0.8, -2.6, 1.5), n=2, seed=30.0 + i, stops=cflame, strength=6.0)
    K.point("Candle", (-0.48, 0.12, 1.08), 6, R.srgb("#FF9A40")[:3], soft=0.03)

    # campfire ring of scanned rocks, logs
    fc = Vector((-1.9, 1.5, 0.05))
    R.fire(fc, 0.9, 1.3, (0.8, -2.6, 1.5), n=7, seed=2.0)
    fire_l = K.point("Fire", fc + Vector((0, 0, 0.55)), 170, R.srgb("#FF7426")[:3], soft=0.35)
    flicker_light(fire_l, 170, N, 1.0)
    rock_m = scan_mat("FireRock", "mossy_rock", scale=2.0, tint="#B0B8F0", sat=0.6, val=0.9)
    for i in range(10):
        ang = 2 * math.pi * i / 10 + rr.uniform(-0.1, 0.1)
        bm = bmesh.new()
        bmesh.ops.create_icosphere(bm, subdivisions=4, radius=rr.uniform(0.16, 0.24))
        K.crumple_verts(bm, 0.07, 5.0, i * 1.3)
        for v in bm.verts:
            v.co.z *= 0.7
        st = K.mesh_object("FireStone", bm, rock_m)
        st.location = fc + Vector((math.cos(ang) * 0.75, math.sin(ang) * 0.62, 0.04))
    bark = scan_mat("Log", "brown_mud_leaves_01", scale=3.0, tint="#6A5040", sat=0.5, val=0.6)
    for i in range(4):
        ang = rr.uniform(0, math.pi)
        p0 = fc + Vector((math.cos(ang) * 0.5, math.sin(ang) * 0.4, 0.06))
        p1 = fc + Vector((-math.cos(ang) * 0.5, -math.sin(ang) * 0.4, 0.22))
        K.mesh_object("Log", K.tube(p0, p1, 0.06, 0.05, segs=10, crumple=0.2, seed=i), bark)

    # knights (Sketchfab, CC BY)
    s1 = Vector((-2.95, 2.0, 0))
    k1, m1 = import_gltf(os.path.join(SF, "sf-knight-sitting.glb"), height=1.55, loc=s1, yaw=facing_yaw(s1, fc), name="KnightSit1")
    s2 = Vector((-0.75, 2.75, 0))
    k2, m2 = import_gltf(os.path.join(SF, "sf-knight-sitting.glb"), height=1.5, loc=s2, yaw=facing_yaw(s2, fc), name="KnightSit2")
    k3, m3 = import_gltf(os.path.join(SF, "sf-heavy-knight.glb"), height=1.95, loc=(1.9, 2.9, 0), yaw=-25, name="HeavyKnight")
    k4, m4 = import_gltf(os.path.join(SF, "sf-plated-knight.glb"), height=2.0, loc=(4.2, 5.2, 0), yaw=-30, name="PlatedKnight")
    for kk, mm, sd in ((k1, m1, 0.3), (k2, m2, 2.1)):
        idle_static(kk, sd, N, breath=0.006, sway=0.35)
        twist_upper(mm, N, sd, deg=4.0, lo=0.62)
    idle_rig(k3, N, 0.7)
    idle_static(k4, 4.0, N, breath=0.005, sway=0.3)

    # props from Poly Haven
    ph_model("dead_tree_trunk", height=0.7, loc=(-4.4, 3.2, -0.05), yaw=40)
    ph_model("dead_tree_trunk_02", height=0.6, loc=(3.4, 3.4, -0.05), yaw=-70)
    bark_t = scan_mat("Bark", "knotted_pine_bark", scale=1.2, tint="#8A90D8", bump=1.8, sat=0.5, val=0.55, moss=0.7)
    sf_tree("sf-oak-set.glb", "Object_4", 7.5, (-6.5, 8.0, 0), 30)
    sf_tree("sf-oak-set.glb", "Object_3", 6.5, (7.5, 10.0, 0), -50)
    sf_tree("sf-old-tree.glb", None, 6.0, (-11.0, 15.0, 0), 120)
    sf_tree("sf-oak-set.glb", "Object_5", 5.5, (12.0, 16.0, 0), 200)
    ph_model("boulder_01", height=1.4, loc=(5.6, 3.6, -0.1), yaw=30)
    ph_model("namaqualand_boulder_02", height=1.1, loc=(-4.6, 4.2, -0.05), yaw=10)
    ph_model("namaqualand_boulder_05", height=0.9, loc=(1.6, 6.4, -0.05), yaw=80)
    ph_model("rock_07", height=0.4, loc=(0.9, -0.6, -0.02), yaw=20)
    ph_model("rock_09", height=0.35, loc=(-1.1, -0.7, -0.02), yaw=60)
    ph_model("pine_roots", height=0.6, loc=(-6.2, 6.8, 0.0), yaw=40)
    ph_model("moss_01", height=0.25, loc=(0.7, 0.6, 0.0), yaw=0)
    ph_model("moss_01", height=0.25, loc=(-0.6, -0.4, 0.0), yaw=90)
    sh, _ = ph_model("kite_shield", height=0.95, loc=(0.78, 0.25, 0.0), yaw=-15)
    sh.rotation_euler.x = math.radians(-14)

    # banners with the helmet on poles
    banner = R.velvet("Banner", base=("#06020E", "#2A0A52", "#5A1A9A"), sheen="#C090FF")
    pole = R.iron("Pole", base="#14182E", edge="#6070B0", rough=0.45)
    for x, y in ((-2.6, 5.2), (3.0, 6.6)):
        K.mesh_object("Pole", K.tube(Vector((x, y, 0)), Vector((x, y, 3.6)), 0.04, 0.035, segs=10), pole)
        bm = bmesh.new()
        uvl = bm.loops.layers.uv.new()
        nx_, nz_ = 10, 18
        g = [[bm.verts.new((x - 0.4 + 0.8 * i / nx_ + 0.0, y - 0.02 + 0.05 * math.sin(i * 0.9 + j * 0.4), 3.45 - 1.6 * j / nz_
                            - (0.25 * (1 - abs(i / nx_ - 0.5) * 2) if j == nz_ else 0))) for i in range(nx_ + 1)] for j in range(nz_ + 1)]
        for j in range(nz_):
            for i in range(nx_):
                bm.faces.new((g[j][i], g[j][i + 1], g[j + 1][i + 1], g[j + 1][i]))
        K.mesh_object("Banner", bm, banner)
        hm, hw, hh = R.helmet(0.45, K.gold_mat("BannerGold", "#C89A40", 0.3), thickness=0.08, dent=0.0)
        hm.location = (x, y - 0.05, 2.75)

    # light
    K.area("MoonKey", (-14, 140, 30), (0, 3, 1), 40, 300000, R.srgb("#8CA4FF")[:3])
    K.area("Side", (-40, 10, 18), (0, 3, 0.5), 20, 16000, R.srgb("#6A7CFF")[:3])
    K.area("Fill", (2, -6, 3), (0, 1, 0.9), 6, 260, R.srgb("#4A3CC0")[:3])
    return notch, centre, nrm


def camera(notch, nrm):
    wide = (Vector((1.15, -3.0, 1.62)), Vector((-0.35, 1.2, 1.05)), 28.0)
    close = (notch + nrm * 0.38 + Vector((0, 0, -0.015)), notch - Vector((0, 0, 0.07)), 50.0)
    cam = K.camera("Cam", lens=wide[2])
    cam.data.sensor_fit = "HORIZONTAL"
    cam.data.sensor_width = 36.0
    if SHOT == "anim":
        # hold wide, push in (ease), hold close on the island
        h0 = int(os.environ.get("HOLD", str(int(N * 0.12))))
        h1 = int(os.environ.get("PUSH", str(int(N * 0.42))))
        keys = [(1, wide), (h0, wide), (h1, close), (N, close)]
    else:
        keys = [(1, wide if SHOT == "wide" else close)]
    for f, (loc, at, lens) in keys:
        cam.location = loc
        cam.rotation_quaternion = (at - loc).to_track_quat("-Z", "Y")
        cam.data.lens = lens
        cam.keyframe_insert("location", frame=f)
        cam.keyframe_insert("rotation_quaternion", frame=f)
        cam.data.keyframe_insert("lens", frame=f)
    K.smooth_keys(cam)
    K.smooth_keys(cam.data)
    return cam


R.setup(W, H, SPP, n_frames=N)
nt_, ce_, nr_ = build()
camera(nt_, nr_)
if os.environ.get("SAVE"):
    bpy.ops.wm.save_as_mainfile(filepath=os.environ["SAVE"])
if os.environ.get("DBG"):
    exec(open(os.environ["DBG"]).read())
else:
    K.render(os.path.join(OUT, SHOT), START, END)
