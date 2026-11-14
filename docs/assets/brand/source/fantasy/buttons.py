"""Download button variants (1120x320, transparent): five dark-fantasy
objects, each carrying an OS mark, the OS name and the download line.

  blender -b --factory-startup -P buttons.py -- OUTDIR SPP VARIANT
VARIANT = <style>-<os>, e.g. stone-mac.
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
STYLE, OS = VAR.split("-", 1)
STYLE = STYLE.split("_", 1)[-1]
W, H = 1120, 320
ORTHO = 3.5                      # world units across the frame
NAME = {"mac": "Mac", "windows": "Windows", "linux": "Linux"}[OS]
LINE = {"mac": "Download · macOS 14+", "windows": "Download · Windows 11",
        "linux": "Download · deb, rpm, AppImage"}[OS]
K.LOGOS = R.MARKS


def mark(size, material, depth=0.06):
    ob, w, h, _ = K.import_logo(OS + ".svg", size, 1.0, thickness=depth)
    ob.data.materials.clear()
    ob.data.materials.append(material)
    ob.rotation_euler = (math.radians(90), 0, 0)
    return ob, w, h


def label(font_main, font_line, mat_main, mat_line, x0, z_main, z_line, size_main, size_line, depth=0.1, y=0.0,
          max_w=2.3):
    t, tw, th = R.text(NAME, font_main, size_main, depth=depth, bevel=0.02)
    if tw > max_w:
        k = max_w / tw
        t.scale = (k, k, k)
        tw *= k
    t.data.materials.append(mat_main)
    t.location = (x0 + tw / 2, y, z_main)
    s, sw, sh = R.text(LINE, font_line, size_line, depth=depth * 0.6, bevel=0.01)
    if sw > max_w:
        k = max_w / sw
        s.scale = (k, k, k)
        sw *= k
    s.data.materials.append(mat_line)
    s.location = (x0 + sw / 2, y, z_line)
    return t, s, max(tw, sw)


def slab_round(w, h, d, r, seed=0.0, crumple=0.0, cuts=4):
    bm = bmesh.new()
    bmesh.ops.create_cube(bm, size=1.0)
    bmesh.ops.scale(bm, vec=(w, d, h), verts=bm.verts)
    ob = K.mesh_object("Slab", bm, smooth=False)
    bv = ob.modifiers.new("bev", "BEVEL")
    bv.width = r
    bv.segments = 6
    dg = bpy.context.evaluated_depsgraph_get()
    me = bpy.data.meshes.new_from_object(ob.evaluated_get(dg))
    ob.modifiers.clear()
    old = ob.data
    ob.data = me
    bpy.data.meshes.remove(old)
    if crumple:
        bm = bmesh.new()
        bm.from_mesh(me)
        bmesh.ops.subdivide_edges(bm, edges=bm.edges[:], cuts=cuts)
        K.crumple_verts(bm, crumple, 3.0, seed)
        bm.to_mesh(me)
        bm.free()
    return ob


def torn_sheet(w, h, seed=0.0, nx=280, nz=80):
    """A grid sheet with a torn outline and curled ends (parchment)."""
    rr = random.Random(int(seed * 31))
    bm = bmesh.new()
    vs = {}
    for j in range(nz + 1):
        for i in range(nx + 1):
            u, v = i / nx, j / nz
            x, z = -w / 2 + w * u, -h / 2 + h * v
            y = 0.18 * abs(2 * u - 1) ** 4 + 0.02 * K.fbm(u * 6 + seed, v * 6, 1.0, 3)
            vs[i, j] = bm.verts.new((x, -y, z))
    def inside(u, v):
        e = 0.03 + 0.025 * K.fbm(u * 9 + seed, v * 9, 2.0, 3) + 0.012 * rr.random()
        return min(u, 1 - u) * w / h * 0.35 > e * 0.6 and min(v, 1 - v) > e
    for j in range(nz):
        for i in range(nx):
            if inside((i + 0.5) / nx, (j + 0.5) / nz):
                bm.faces.new((vs[i, j], vs[i + 1, j], vs[i + 1, j + 1], vs[i, j + 1]))
    loose = [v for v in bm.verts if not v.link_faces]
    bmesh.ops.delete(bm, geom=loose, context="VERTS")
    ex = bmesh.ops.extrude_face_region(bm, geom=bm.faces[:])
    bmesh.ops.translate(bm, vec=(0, 0.012, 0), verts=[g for g in ex["geom"] if isinstance(g, bmesh.types.BMVert)])
    return K.mesh_object("Sheet", bm)


def lights():
    K.area("Key", (-3, -6, 4), (0, 0, 0), 5, 900, R.srgb("#C8D4FF")[:3])
    K.area("Warm", (4, -5, -2), (0, 0, 0), 4, 220, R.srgb("#FF9A50")[:3])
    K.area("Rim", (0, 5, 3), (0, 0, 0), 5, 900, R.srgb("#8A60FF")[:3])


def build():
    lights()
    rune = R.glow_mat("Rune", "#9CC0FF", 3.2)
    rune_dim = R.glow_mat("RuneDim", "#7A90E0", 2.2)
    gold = K.gold_mat("Gold", "#E0A840", 0.25)
    if STYLE == "stone":
        stone = R.lichen_stone("Tablet", scale=3.0, base=("#05050E", "#1A1A34", "#383A66", "#6A6CA0"), spots="#B0B8F0", crack=True)
        bm = K.shard_tablet(3.3, 0.86, 0.22, seed={"mac": 1.0, "windows": 2.0, "linux": 4.0}[OS], jag=0.06, chips=5, segs=64)
        sl = K.mesh_object("Tablet", bm, stone)
        sl.location = (0, 0.11, 0)
        m, mw, mh = mark(0.5, rune, 0.04)
        m.location = (-1.1, -0.07, 0.0)
        label("cinzeldeco", "cinzel", rune, rune_dim, -0.72, 0.1, -0.2, 0.36, 0.13, depth=0.04, y=-0.07)
    elif STYLE == "iron":
        plate = R.iron("Plate", base="#1C2040", edge="#8090D0", rough=0.4)
        sl = slab_round(3.3, 0.86, 0.12, 0.06)
        sl.data.materials.append(plate)
        sl.location = (0, 0.06, 0)
        silver = R.iron("Silver", base="#9AA4D8", edge="#FFFFFF", rough=0.2)
        for x in (-1.55, 1.55):
            for z in (-0.33, 0.33):
                bm = bmesh.new()
                bmesh.ops.create_uvsphere(bm, u_segments=12, v_segments=8, radius=0.045)
                rv = K.mesh_object("Rivet", bm, silver)
                rv.location = (x, -0.0, z)
        m, mw, mh = mark(0.5, silver, 0.06)
        m.location = (-1.05, -0.02, 0.0)
        label("cinzeldeco", "cinzel", silver, silver, -0.68, 0.1, -0.2, 0.36, 0.13, depth=0.08, y=-0.03)
    elif STYLE == "parchment":
        p = R.parchment("Parchment")
        sl = torn_sheet(3.2, 0.8, seed=3.0 + {"mac": 0, "windows": 1, "linux": 2}[OS])
        sl.data.materials.append(p)
        ink = K.emission("Ink", R.srgb("#1A0A04"), 0.0)
        ink = R.velvet("Ink", base=("#0A0302", "#140602", "#1E0A04"), sheen="#2A1206")
        # wax seal: a lumpy red disc with the mark pressed in
        wax = R.velvet("Wax", base=("#3A0004", "#8A0410", "#C01822"), sheen="#FF6060")
        wb = bmesh.new()
        bmesh.ops.create_cone(wb, cap_ends=True, segments=40, radius1=0.33, radius2=0.31, depth=0.06)
        for v in wb.verts:
            ang = math.atan2(v.co.y, v.co.x)
            k = 1 + 0.08 * math.sin(ang * 7) + 0.05 * K.fbm(math.cos(ang), math.sin(ang), 1.0, 2)
            v.co.x *= k
            v.co.y *= k
        bmesh.ops.subdivide_edges(wb, edges=wb.edges[:], cuts=2)
        seal = K.mesh_object("Seal", wb, wax)
        seal.rotation_euler = (math.radians(90), 0, 0)
        seal.location = (-1.12, -0.05, 0.0)
        m, mw, mh = mark(0.34, wax, 0.05)
        m.location = (-1.12, -0.1, 0.0)
        label("cinzeldeco", "cinzel", ink, ink, -0.7, 0.1, -0.2, 0.36, 0.13, depth=0.02, y=-0.09)
    elif STYLE == "pennant":
        cloth = R.velvet("Banner", base=("#06020E", "#24084A", "#4A1690"), sheen="#C090FF")
        bm = bmesh.new()
        nx, nz = 60, 16
        w, h = 3.2, 0.78
        grid = []
        for j in range(nz + 1):
            z = h / 2 - h * j / nz
            notch = 0.2 * (1 - abs(z) / (h / 2))
            ext = w / 2 - notch
            row = []
            for i in range(nx + 1):
                x = -ext + 2 * ext * i / nx
                y = 0.05 * math.sin(i / nx * math.pi * 3 + j * 0.15) + 0.03 * K.fbm(i * 0.2, j * 0.3, 2.0, 2)
                row.append(bm.verts.new((x, y, z)))
            grid.append(row)
        for j in range(nz):
            for i in range(nx):
                bm.faces.new((grid[j][i], grid[j][i + 1], grid[j + 1][i + 1], grid[j + 1][i]))
        ex = bmesh.ops.extrude_face_region(bm, geom=bm.faces[:])
        bmesh.ops.translate(bm, vec=(0, 0.02, 0), verts=[g for g in ex["geom"] if isinstance(g, bmesh.types.BMVert)])
        sl = K.mesh_object("Banner", bm, cloth)
        trim = gold
        rod = K.mesh_object("Rod", K.tube(Vector((-1.72, 0, 0.42)), Vector((1.72, 0, 0.42)), 0.035, 0.035, segs=16), trim)
        for x in (-1.72, 1.72):
            bm = bmesh.new()
            bmesh.ops.create_uvsphere(bm, u_segments=12, v_segments=8, radius=0.06)
            fin = K.mesh_object("Finial", bm, trim)
            fin.location = (x, 0, 0.42)
        m, mw, mh = mark(0.42, gold, 0.03)
        m.location = (-1.0, -0.06, -0.02)
        label("cinzeldeco", "cinzel", gold, gold, -0.65, 0.07, -0.21, 0.34, 0.12, depth=0.03, y=-0.06)
    elif STYLE == "shield":
        steel = R.iron("Steel", base="#30365E", edge="#C0C8FF", rough=0.3)
        field = R.velvet("Field", base=("#08021A", "#2A0C5A", "#5020A0"), sheen="#B090FF")
        # a heater shield on the left
        prof = []
        for k in range(41):
            t = k / 40
            ang = math.pi * t
            prof.append((0.38 * math.cos(ang) * (1 if t < 0.5 else 1), 0))
        bm = bmesh.new()
        pts = []
        for k in range(30):
            t = k / 29
            x = -0.4 + 0.8 * t
            pts.append((x, 0.42))
        for k in range(1, 40):
            t = k / 40
            ang = math.pi * t
            pts.append((0.4 * math.cos(ang * 0.5) if False else 0.4 - 0.4 * (1 - math.cos(ang * 0.5)) ** 0.7 * 0,
                        0.42 - 0.9 * t))
        # simpler: build the outline analytically
        pts = [(-0.4, 0.42), (0.4, 0.42)]
        for k in range(1, 31):
            t = k / 30
            pts.append((0.4 * (1 - t ** 1.8), 0.42 - 0.95 * (t ** 0.85)))
        for k in range(29, 0, -1):
            t = k / 30
            pts.append((-0.4 * (1 - t ** 1.8), 0.42 - 0.95 * (t ** 0.85)))
        f = bm.faces.new([bm.verts.new((x, 0, z)) for x, z in pts])
        ex = bmesh.ops.extrude_face_region(bm, geom=[f])
        bmesh.ops.translate(bm, vec=(0, 0.06, 0), verts=[g for g in ex["geom"] if isinstance(g, bmesh.types.BMVert)])
        sh = K.mesh_object("Shield", bm, field, smooth=False)
        sh.location = (-1.25, -0.1, 0.1)
        sh.scale = (0.88, 1, 0.82)
        bv = sh.modifiers.new("bev", "BEVEL")
        bv.width = 0.035
        bv.segments = 3
        rim = sh.copy()
        rim.data = sh.data.copy()
        rim.data.materials.clear()
        rim.data.materials.append(steel)
        K.link(rim)
        rim.scale = (0.97, 0.8, 0.87)
        rim.location = (-1.25, -0.08, 0.112)
        m, mw, mh = mark(0.36, gold, 0.05)
        m.location = (-1.25, -0.13, 0.08)
        plank = R.iron("Plank", base="#121428", edge="#6070B0", rough=0.45)
        sl = slab_round(2.6, 0.62, 0.08, 0.05)
        sl.data.materials.append(plank)
        sl.location = (0.35, 0.0, 0)
        pale = R.iron("Pale", base="#A8B0E0", edge="#FFFFFF", rough=0.25)
        label("cinzeldeco", "cinzel", gold, pale, -0.75, 0.08, -0.17, 0.3, 0.11, depth=0.05, y=-0.04, max_w=2.2)
    R.camera((0, -20, 0), (0, 0, 0), lens=50, ortho=ORTHO)


R.setup(W, H, SPP, transparent=True)
build()
R.still(os.path.join(OUT, VAR))
