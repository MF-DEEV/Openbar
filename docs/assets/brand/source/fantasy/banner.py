"""Banner scene variants (1800x506): the Openbar lockup in five night scenes
after @ozavry's reel.

  blender -b --factory-startup -P banner.py -- OUTDIR SPP VARIANT [FRAME]
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
from mathutils import Vector  # noqa: E402

a = K.args()
OUT, SPP, VAR = a[0], int(a[1]), a[2]
FRAME = int(a[3]) if len(a) > 3 else 1
W, H = (1280, 640) if os.environ.get("SOCIAL") else (1800, 506)
WORD = os.environ.get("WORD", "iron")
rr = random.Random(5)


def lockup(center, width, facing=None, style=WORD):
    """Helmet + Openbar, scaled to `width`, centred on `center`, facing -Y."""
    if style == "iron":
        m = R.iron("WordIron", base="#30366A", edge="#C8D2FF")
        font, sx, depth, bevel = "saira", 1.25, 0.22, 0.03
    elif style == "gold":
        m = K.gold_mat("WordGold", "#D8A040", 0.28)
        font, sx, depth, bevel = "fraktur", 1.0, 0.2, 0.018
    elif style == "fraktur":
        m = R.iron("WordIron", base="#30366A", edge="#C8D2FF")
        font, sx, depth, bevel = "fraktur", 1.0, 0.24, 0.02
    elif style == "moon":
        m = R.moon_mat("WordMoon", glow=0.25)
        font, sx, depth, bevel = "cinzeldeco", 1.0, 0.22, 0.025
    else:
        m = R.lichen_stone("WordStone", scale=2.2)
        font, sx, depth, bevel = "grenze", 1.05, 0.3, 0.02
    t, tw, th = R.text("Openbar", font, 1.0, depth=depth, bevel=bevel, scale_x=sx)
    t.data.materials.append(m)
    hm_mat = m if style not in ("gold",) else R.iron("HelmIron", base="#30366A", edge="#C8D2FF")
    hm, hw, hh = R.helmet(th * 1.3, hm_mat, thickness=0.25, dent=0.01)
    gap = 0.2
    total = hw + gap + tw
    root = K.link(bpy.data.objects.new("Lockup", None))
    hm.parent = root
    t.parent = root
    hm.location = (-total / 2 + hw / 2, 0, 0.0)
    t.location = (-total / 2 + hw + gap + tw / 2, 0, 0)
    k = width / total
    root.scale = (k, k, k)
    root.location = Vector(center)
    if facing is not None:
        root.rotation_mode = "QUATERNION"
        root.rotation_quaternion = K.look_rot(Vector(facing))
    return root, width, th * k


def word_lights(center, s=1.0):
    c = Vector(center)
    K.area("WordKey", c + Vector((-3, -9, 5)) * s, c, 6 * s, 1500 * s * s, R.srgb("#B4C4FF")[:3])
    K.area("WordRimL", c + Vector((-7, 5, 2)) * s, c, 4 * s, 900 * s * s, R.srgb("#7A4CFF")[:3])
    K.area("WordRimR", c + Vector((7, 5, 2)) * s, c, 4 * s, 900 * s * s, R.srgb("#4C6CFF")[:3])


def link_only(light, objs):
    """Light-link: `light` lights only `objs` (keeps the key off the set)."""
    col = bpy.data.collections.new(light.name + " receivers")
    for o in objs:
        col.objects.link(o)
        for ch in o.children_recursive:
            col.objects.link(ch)
    light.light_linking.receiver_collection = col


def word_set(center, width, s=1.0):
    root, w, h = lockup(center, width)
    before = set(bpy.data.objects)
    word_lights(center, s)
    lights = [o for o in bpy.data.objects if o not in before]
    for lo in lights:
        link_only(lo, [root])
    return root


def far_crags(y, x0, x1, n, h0, h1, mat, seed=1.0, z0=-4.0):
    r = random.Random(int(seed * 10))
    for i in range(n):
        x = x0 + (x1 - x0) * (i + r.uniform(0.1, 0.9)) / n
        hh = r.uniform(h0, h1)
        K.crag(f"Crag{seed}_{i}", x, y + r.uniform(-4, 4), z0, hh * 0.35, hh, mat, sub=5, amp=hh * 0.05, lean=r.uniform(-1, 1),
               seed=seed + i, shards=4)


def stars(cam, fwd, n=900, R_=120.0):
    K.star_field(cam, Vector(fwd).normalized(), n=n, R=R_, bright_p=0.03, cone=0.6, big=(0.25, 0.5),
                 bright_strength=30.0, dim_strength=2.0, up_min=0.02)


# ---------------------------------------------------------------- A: moon over the castle
def scene_a():
    cam = (0, -30, 3.0)
    R.camera(cam, (0, 0, 4.6), lens=32)
    R.sky((0, 160, 30), (420, 160), strength=0.9, scale=(0.012, 0.03, 1.0))
    R.moon((-15, 95, 19), 7.0)
    rock = R.lichen_stone("Rock", scale=0.3, base=("#020208", "#0A0A22", "#1A1A44", "#33336E"), spots="#6A70C0")
    far = K.marble("Far", 0.1, [(0.0, "#010006"), (0.5, "#06041C"), (1.0, "#120C40")], glow=0.05)
    R.cliff("CliffR", (11, 26, 5.5), 9, 7, 18, rock, seed=2.0)
    R.castle((11, 26, 5.5), s=0.6, seed=4)
    R.cliff("CliffL", (-17, 32, 3.0), 7, 6, 14, rock, seed=5.0)
    R.castle((-17, 32, 3.0), s=0.34, seed=9)
    far_crags(70, -60, 60, 10, 8, 18, far, seed=3.0, z0=-6)
    water = R.water_mat("Water")
    for x, w_, h_ in ((7.5, 1.6, 15), (9.6, 1.0, 13), (14.6, 1.3, 14), (-14.5, 1.0, 11)):
        y_ = 22.6 if x > 0 else 29.0
        ztop = 5.6 if x > 0 else 3.1
        R.plane(f"Fall{x}", (x, y_, ztop - h_ / 2), (w_, h_), water).visible_diffuse = False
    mist = R.cloud_sky("Mist", stops=[(0.0, "#000000"), (0.45, "#000000"), (0.6, "#0A0A50"), (0.8, "#3040C0"), (1.0, "#8090FF")],
                       strength=0.6, scale=(0.05, 0.15, 1))
    R.plane("MistPlane", (0, 40, -3.0), (160, 60), mist, rot=(0, 0, 0))
    # moonlight from behind (rims everything) + cold fill from the front
    K.area("MoonKey", (-15, 90, 22), (0, 25, 3), 25, 60000, R.srgb("#9AB0FF")[:3])
    K.area("Fill", (0, -20, 10), (0, 25, 3), 30, 9000, R.srgb("#4A3CC0")[:3])
    stars(cam, (0, 1, 0.25), 600, 140)
    word_set((0, -6, 3.0), 13.5)


# ---------------------------------------------------------------- B: knights round the campfire
def scene_b():
    cam = (0, -7.5, 1.5)
    R.camera(cam, (0, 4.5, 1.9), lens=27)
    R.sky((0, 90, 18), (300, 110), strength=0.8, scale=(0.02, 0.05, 1.0))
    R.moon((15, 60, 9.5), 3.6)
    grass = R.moss_ground()
    R.ground("Ground", (80, 60), grass, amp=0.5, z=0.0, centre=(0, 22))
    armour = R.lichen_stone("Armour", scale=1.2)
    metal = R.lichen_stone("ArmourMetal", scale=1.6, base=("#0A100A", "#26331F", "#56704A", "#A8BC98"))
    fire_c = Vector((0, 4.5, 0.1))
    R.fire(fire_c, 1.6, 2.4, cam, n=7, seed=1.0)
    K.point("FireLight", fire_c + Vector((0, 0, 0.9)), 700, R.srgb("#FF7A2A")[:3], soft=0.6)
    stone = R.lichen_stone("FireStones", scale=2.0, base=("#05050A", "#1A1A2A", "#3A3A55", "#70709A"), crack=False)
    for i in range(11):
        ang = 2 * math.pi * i / 11
        bm = bmesh.new()
        bmesh.ops.create_icosphere(bm, subdivisions=3, radius=rr.uniform(0.22, 0.32))
        K.crumple_verts(bm, 0.06, 4.0, i)
        st = K.mesh_object("FS", bm, stone)
        st.location = fire_c + Vector((math.cos(ang) * 1.05, math.sin(ang) * 0.9, 0.05))
    for (x, y, sit, rot) in ((-2.8, 3.6, 1.0, 1), (2.8, 3.8, 1.0, -1), (-1.9, 6.6, 1.0, 1), (2.0, 6.9, 1.0, -1)):
        d = (fire_c - Vector((x, y, 0)))
        d.z = 0
        R.knight(f"Knight{x}", (x, y, 0), d, h=1.9, armour=armour, metal=metal, sit=sit, seed=x + y)
    bark = R.lichen_stone("Bark", scale=1.0, base=("#020204", "#0C0A18", "#1E1A36", "#3A3460"), spots="#4A4A80", crack=False)
    leaves = R.velvet("Leaves", base=("#020210", "#0A0C40", "#1C2A90"), sheen="#4A6AFF")
    R.tree("TreeL", (-7.5, 10, 0), 8, 0.9, bark, None, seed=1.0, branches=9)
    R.tree("TreeR", (8.0, 12, 0), 7, -0.8, bark, None, seed=2.0, branches=9)
    for i in range(8):
        R.tree(f"TreeBg{i}", (-24 + i * 7 + rr.uniform(-2, 2), 26 + rr.uniform(0, 8), 0), rr.uniform(6, 10), rr.uniform(-0.8, 0.8),
               bark, None, seed=10 + i, branches=6)
    K.area("MoonKey", (12, 55, 14), (0, 5, 1), 15, 22000, R.srgb("#8CA4FF")[:3])
    K.area("Fill", (0, -12, 6), (0, 5, 1), 12, 1400, R.srgb("#3A30A0")[:3])
    stars(cam, (0, 1, 0.3), 500, 70)
    word_set((0, 8.0, 3.7), 6.2, s=0.6)


# ---------------------------------------------------------------- C: the road to the gate
def scene_c():
    cam = (0, -5, 1.6)
    R.camera(cam, (0, 30, 6.8), lens=23)
    R.sky((0, 140, 30), (400, 150), strength=0.85, scale=(0.012, 0.03, 1.0))
    R.moon((13, 100, 22), 6.0)
    cob = R.cobble_mat()
    grass = R.moss_ground()
    R.ground("Ground", (120, 80), grass, amp=0.6, z=-0.15, centre=(0, 30))
    R.plane("Road", (0, 12, 0.02), (4.2, 38), cob, rot=(0, 0, 0)).visible_shadow = True
    stone = R.castle_stone("Gate", scale=0.6)
    # gatehouse: two towers and an arch with the lockup on its lintel
    gx, gy = 0, 30
    for sx in (-1, 1):
        prof = [(2.4, 0), (2.2, 1.0), (2.15, 12.0), (2.6, 12.4), (2.6, 13.6), (0.02, 13.6)]
        bm = K.revolve(prof, 14, seed=sx * 3.0, crumple=0.04, squash=1.0, step=0.4)
        tw = K.mesh_object("GateTower", bm, stone)
        tw.location = (gx + sx * 7.2, gy + 0.5, 0)
        sp = [(2.9, 0), (1.6, 2.5), (0.5, 5.5), (0.01, 7.5)]
        bm = K.revolve(sp, 12, seed=sx * 5.0, crumple=0.06, squash=1.0, step=0.4, bend=sx * 0.4)
        ss = K.mesh_object("GateSpire", bm, R.castle_stone("Roof", scale=1.2, tint=("#020108", "#0A0820", "#1A1640", "#2A2460")))
        ss.location = (gx + sx * 7.2, gy + 0.5, 13.6)
    wb = bmesh.new()
    bmesh.ops.create_cube(wb, size=1.0)
    bmesh.ops.scale(wb, vec=(12.4, 2.4, 3.6), verts=wb.verts)
    bmesh.ops.translate(wb, vec=(gx, gy, 9.6), verts=wb.verts)
    for x in (-5.4, -3.6, -1.8, 0, 1.8, 3.6, 5.4):
        for z0, z1 in ((0, 7.8),):
            pass
    for sx in (-1, 1):
        g = bmesh.ops.create_cube(wb, size=1.0)
        bmesh.ops.scale(wb, vec=(3.2, 2.4, 7.8), verts=g["verts"])
        bmesh.ops.translate(wb, vec=(gx + sx * 4.6, gy, 3.9), verts=g["verts"])
    for k in range(9):
        if k in (2, 6):
            continue
        g = bmesh.ops.create_cube(wb, size=1.0)
        bmesh.ops.scale(wb, vec=(0.8, 2.4, 0.9), verts=g["verts"])
        bmesh.ops.translate(wb, vec=(gx - 5.6 + k * 1.4, gy, 11.85), verts=g["verts"])
    K.mesh_object("GateWall", wb, stone, smooth=False)
    # arch: a half ring under the lintel
    bm = bmesh.new()
    pts = []
    for k in range(25):
        t = math.pi * k / 24
        pts.append(Vector((gx + 3.0 * math.cos(t), gy - 1.25, 5.0 + 2.8 * math.sin(t))))
    K.sweep(pts, [0.35] * len(pts), segs=10, crumple=0.1, seed=1.0, bm=bm)
    K.mesh_object("Arch", bm, stone)
    # the dark gateway with a warm glow deep inside
    R.plane("Gateway", (gx, gy + 1.0, 3.9), (6.0, 7.8), R.glow_mat("GateGlow", "#C04A10", 0.5), rot=(90, 0, 0))
    iron_ = R.iron("Portcullis", base="#14182E", edge="#6070B0", rough=0.45)
    pb = bmesh.new()
    for k in range(9):
        x = gx - 2.6 + k * 0.65
        K.tube(Vector((x, gy - 0.2, 3.2)), Vector((x, gy - 0.2, 7.8)), 0.06, 0.06, segs=8, bm=pb)
    for k in range(6):
        z = 3.4 + k * 0.75
        K.tube(Vector((gx - 2.9, gy - 0.25, z)), Vector((gx + 2.9, gy - 0.25, z)), 0.05, 0.05, segs=8, bm=pb)
    for k in range(9):
        x = gx - 2.6 + k * 0.65
        g = bmesh.ops.create_cone(pb, cap_ends=True, segments=6, radius1=0.09, radius2=0.0, depth=0.4)
        bmesh.ops.rotate(pb, verts=g["verts"], cent=(0, 0, 0), matrix=__import__("mathutils").Matrix.Rotation(math.pi, 3, "X"))
        bmesh.ops.translate(pb, vec=(x, gy - 0.2, 3.0), verts=g["verts"])
    K.mesh_object("Portcullis", pb, iron_)
    K.point("GateLight", (gx, gy - 1.0, 3.0), 3000, R.srgb("#FF8A30")[:3], soft=1.0)
    for sx in (-1, 1):
        K.torch(gx + sx * 3.8, gy - 1.6, 4.0, hgt=0.0, energy=400) if hasattr(K, "torch") else None
    # walls running off both sides
    for sx in (-1, 1):
        wb = bmesh.new()
        bmesh.ops.create_cube(wb, size=1.0)
        bmesh.ops.scale(wb, vec=(30, 1.6, 6.5), verts=wb.verts)
        bmesh.ops.translate(wb, vec=(gx + sx * 24.5, gy + 3, 3.25), verts=wb.verts)
        bmesh.ops.subdivide_edges(wb, edges=wb.edges[:], cuts=8, use_grid_fill=True)
        K.crumple_verts(wb, 0.25, 0.3, sx)
        K.mesh_object("CurtainWall", wb, stone, smooth=False)
    bark = R.lichen_stone("Bark", scale=1.0, base=("#020204", "#0C0A18", "#1E1A36", "#3A3460"), spots="#4A4A80", crack=False)
    leaves = R.velvet("Leaves", base=("#020210", "#0A0C40", "#1C2A90"), sheen="#4A6AFF")
    for i, (x, y) in enumerate(((-6, 4), (6.5, 7), (-7.5, 13), (8, 16), (-9, 21))):
        R.tree(f"RoadTree{i}", (x, y, 0), rr.uniform(6, 9), -0.4 * math.copysign(1, x), bark, None, seed=i + 3.0, branches=8)
    K.area("MoonKey", (9, 95, 26), (0, 25, 3), 20, 50000, R.srgb("#9AB0FF")[:3])
    K.area("Fill", (0, -10, 8), (0, 20, 3), 20, 14000, R.srgb("#3A30A0")[:3])
    stars(cam, (0, 1, 0.35), 600, 120)
    word_set((gx, gy - 1.35, 9.6), 10.6, s=1.3)


# ---------------------------------------------------------------- D: lockup, moon, clouds
def scene_d():
    cam = (0, -26, 0.5)
    R.camera(cam, (0, 0, 0.6), lens=46)
    R.sky((0, 80, 4), (260, 90), strength=0.8, scale=(0.02, 0.05, 1.0))
    R.moon((11.5, 45, 5.6), 4.2)
    K.area("MoonKey", (5, 28, 6), (0, 0, 0), 10, 5000, R.srgb("#9AB0FF")[:3])
    stars(cam, (0, 1, 0.1), 700, 70)
    word_set((0, 0, 0.4), 15.0)


# ---------------------------------------------------------------- E: knight under the tree, blue cauldron
def scene_e():
    cam = (0, -9, 1.6)
    R.camera(cam, (0, 4, 2.2), lens=27)
    R.sky((0, 90, 18), (300, 110), strength=0.85, scale=(0.02, 0.05, 1.0))
    R.moon((-13.0, 60, 11.5), 4.2)
    grass = R.moss_ground()
    R.ground("Ground", (80, 60), grass, amp=0.6, z=0.0, centre=(0, 22))
    bark = R.lichen_stone("Bark", scale=1.0, base=("#020204", "#0C0A18", "#1E1A36", "#3A3460"), spots="#4A4A80", crack=False)
    leaves = R.velvet("Leaves", base=("#020210", "#0A0C40", "#1C2A90"), sheen="#4A6AFF")
    R.tree("Tree", (-7.8, 6.6, 0), 8, 0.35, bark, None, seed=4.0, branches=10)
    armour = R.iron("KnightIron", base="#1E2448", edge="#8C9CE0", rough=0.35)
    R.knight("Resting", (-6.0, 4.6, 0), Vector((0.6, -1, 0)), h=2.3, armour=armour, metal=armour, sit=1.0, seed=3.0)
    # cauldron
    pot = R.iron("Pot", base="#0C0E18", edge="#5060A0", rough=0.5)
    prof = [(0.0, 0.05), (0.4, 0.06), (0.62, 0.3), (0.66, 0.6), (0.56, 0.86), (0.6, 0.92)]
    bm = K.revolve(prof, 24, seed=1.0, crumple=0.02, squash=1.0, step=0.06, cap=True)
    cpos = Vector((-3.6, 3.4, 0.0))
    K.mesh_object("Cauldron", bm, pot).location = cpos
    blue = [(0.0, "#000000"), (0.05, "#000830"), (0.2, "#0828D0"), (0.4, "#2A70FF"), (0.6, "#80C0FF"), (0.85, "#E8F6FF")]
    R.fire(cpos + Vector((0, 0, 0.75)), 1.0, 1.6, cam, n=6, seed=4.0, stops=blue, strength=3.0)
    K.point("CauldronLight", cpos + Vector((0, 0, 1.4)), 900, R.srgb("#4A80FF")[:3], soft=0.5)
    K.point("WarmLeft", (-11, 9, 1.2), 600, R.srgb("#FF7A2A")[:3], soft=0.4)
    for i in range(7):
        R.tree(f"TreeBg{i}", (-2 + i * 6 + rr.uniform(-2, 2), 24 + rr.uniform(0, 8), 0), rr.uniform(6, 10), rr.uniform(-0.8, 0.8),
               bark, None, seed=20 + i, branches=6)
    rock = R.lichen_stone("Rock", scale=0.3, base=("#020208", "#0A0A22", "#1A1A44", "#33336E"), spots="#6A70C0")
    R.cliff("FarCliff", (16, 50, 6), 6, 5, 12, rock, seed=8.0)
    R.castle((16, 50, 6), s=0.4, seed=12)
    K.area("MoonKey", (-3, 55, 14), (-3, 4, 1), 15, 20000, R.srgb("#8CA4FF")[:3])
    K.area("Fill", (0, -12, 6), (0, 5, 1), 12, 1200, R.srgb("#3A30A0")[:3])
    stars(cam, (0, 1, 0.3), 500, 70)
    word_set((3.3, 3.0, 2.4), 6.6, s=0.7)


def bats(n, f0, f1, y, z, x0, x1, seed=1):
    """Small black bats flapping across the sky between frames f0 and f1."""
    r = random.Random(seed)
    black = K.emission("Bat", (0.002, 0.001, 0.006, 1), 0.0)
    for i in range(n):
        root = K.link(bpy.data.objects.new(f"Bat{i}", None))
        s_ = r.uniform(1.3, 2.1)
        bm = bmesh.new()
        bmesh.ops.create_icosphere(bm, subdivisions=1, radius=0.12)
        body = K.mesh_object(f"BatBody{i}", bm, black, smooth=False)
        body.parent = root
        body.scale = (0.8, 0.8, 1.3)
        for sx in (-1, 1):
            wb = bmesh.new()
            pts = [(0, 0), (sx * 0.5, 0.18), (sx * 0.95, 0.05), (sx * 0.8, -0.12), (sx * 0.6, -0.02), (sx * 0.4, -0.15), (sx * 0.2, -0.05)]
            f = wb.faces.new([wb.verts.new((x, 0, zz)) for x, zz in pts])
            wing = K.mesh_object(f"BatWing{i}{sx}", wb, black, smooth=False)
            wing.parent = root
            ph = r.uniform(0, 6.28)
            for fr in range(f0, f1 + 1, 2):
                wing.rotation_euler.y = sx * 0.9 * math.sin(fr * 1.4 + ph)
                wing.keyframe_insert("rotation_euler", frame=fr)
        root.scale = (s_, s_, s_)
        dy = r.uniform(-6, 6)
        dz = r.uniform(-3, 3)
        g0 = f0 + r.randint(0, 8)
        g1 = f1 - r.randint(0, 8)
        root.location = (x0 + r.uniform(-3, 3), y + dy, z + dz)
        root.keyframe_insert("location", frame=g0 - 1)
        root.location = (x1 + r.uniform(-3, 3), y + dy, z + dz + r.uniform(-2, 3))
        root.keyframe_insert("location", frame=g1)
        root.rotation_euler.z = math.radians(r.uniform(-10, 10))
        K.linear_keys(root)
        root.hide_render = False
        for fr, hide in ((g0 - 2, True), (g0 - 1, False), (g1, False), (g1 + 1, True)):
            for ob in [root] + list(root.children):
                ob.hide_render = hide
                ob.keyframe_insert("hide_render", frame=fr)


def scene_a2():
    """Round 2 of A: ragged ranges with strata, a cratered helmet moon,
    a rougher castle rock, mist, bats."""
    cam = (0, -30, 3.0)
    R.camera(cam, (0, 0, 4.6), lens=32)
    R.sky((0, 170, 30), (440, 170), strength=0.9, scale=(0.012, 0.03, 1.0), flow=(0.11, 0.0), drift=0.03)
    R.moon2((-15, 100, 19.5), 7.0)
    near = R.strata_rock("RockNear", scale=0.5)
    mid = R.strata_rock("RockMid", scale=0.25, base=("#010103", "#050516", "#0E0E30", "#1E1C4C", "#3A3880"), glow=0.02)
    far = R.strata_rock("RockFar", scale=0.12, base=("#000002", "#03030E", "#080820", "#121236", "#24225A"), glow=0.05)
    R.mountains("RangeFar", -170, 170, 85, 135, 30, far, seed=1.0, nx=420, ny=50, base_z=-8)
    R.mountains("RangeMid", -110, 110, 48, 80, 15, mid, seed=4.0, nx=360, ny=50, base_z=-7, sharp=1.9)
    R.crag_cliff("CliffR", (15.5, 28, 5.0), 9, 7, 18, near, seed=2.0)
    R.castle((15.5, 28, 5.0), s=0.6, seed=4, stone=R.strata_rock("CastleRock", scale=1.2, base=("#020208", "#0A0A22", "#181840", "#2C2A66", "#4A48A0")))
    R.crag_cliff("CliffL", (-22, 34, 3.5), 7, 6, 14, near, seed=5.0)
    R.castle((-22, 34, 3.5), s=0.38, seed=9)
    water = R.water_mat("Water")
    for x, w_, h_ in ((12.1, 1.8, 15), (14.3, 1.1, 13), (18.9, 1.4, 14), (-19.6, 1.1, 11)):
        y_ = 24.4 if x > 0 else 30.8
        ztop = 5.1 if x > 0 else 3.6
        R.waterfall(f"Fall{x}", (x, y_, ztop), w_, h_, water, seed=x, lean=1.0)
    mist = R.cloud_sky("Mist", stops=[(0.0, "#02021A"), (0.4, "#08084A"), (0.6, "#1A1C80"), (0.8, "#4050D0"), (1.0, "#90A0FF")],
                       strength=0.55, scale=(0.05, 0.15, 1), flow=(0.25, 0.0), drift=0.05)
    R.plane("MistPlane", (0, 40, -3.0), (160, 60), mist, rot=(0, 0, 0))
    K.area("MoonKey", (-15, 95, 22), (0, 25, 3), 25, 60000, R.srgb("#9AB0FF")[:3])
    K.area("Side", (-80, 30, 40), (0, 40, 0), 50, 90000, R.srgb("#7C8CFF")[:3])
    K.area("Fill", (0, -20, 10), (0, 25, 3), 30, 4000, R.srgb("#4A3CC0")[:3])
    stars(cam, (0, 1, 0.25), 600, 160)
    bats(4, 22, 66, 45, 13, -34, 26, seed=3)
    word_set((0, -6, 2.4), 12.2)


SCENES = {"A2-moon-castle": scene_a2, "A-moon-castle": scene_a, "B-campfire": scene_b, "C-gate": scene_c, "D-minimal": scene_d, "E-cauldron": scene_e}

R.setup(W, H, SPP, n_frames=int(os.environ.get("NFRAMES", "1")))
SCENES[VAR]()
if os.environ.get("SAVE"):
    bpy.ops.wm.save_as_mainfile(filepath=os.environ["SAVE"])
if len(a) > 4:
    K.render(os.path.join(OUT, VAR), FRAME, int(a[4]))
else:
    R.still(os.path.join(OUT, VAR), FRAME)
