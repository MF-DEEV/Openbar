"""Renders the companion's sprites for one texture.

    blender -b -P render_sprites.py -- TEXTURE OUT_DIR [res] [samples] [only]

Writes, as transparent 16-bit PNGs at res x res:
  body-<texture>-<color>-<yaw>.png   the bean (with feet, no eyes) per colour
  item-<texture>-<item>-<yaw>.png    each head item alone; the body is a shadow
                                     catcher, so the item keeps its shadow on
                                     the body and hides where the body is in front
  eye-<texture>-<tone>.png           a square swatch of the eye material
  anchors-<texture>.json             eye centres and size per yaw, in pixels

Every render uses the same orthographic camera, so all sprites line up; only
the figure turns (yaw). `only` limits the run to "body", "items" or "eyes".
Tools/companion/build_assets.py turns the PNGs into the apps' WebP sprites.
"""
import json
import math
import os
import sys

import bpy
from mathutils import Vector

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import companion_lib as L  # noqa: E402
import companion_v2 as V  # noqa: E402

YAWS = [-24, -12, 0, 12, 24]

# Body colours (sRGB) and the eye tone that reads on each.
COLORS = {
    "pearl": ((240, 232, 214), "ink"),
    "lemon": ((246, 204, 70), "ink"),
    "peach": ((246, 150, 72), "ink"),
    "rose": ((232, 96, 104), "cream"),
    "berry": ((206, 58, 138), "cream"),
    "lilac": ((172, 128, 228), "ink"),
    "sky": ((96, 160, 232), "ink"),
    "navy": ((52, 74, 150), "cream"),
    "mint": ((72, 184, 150), "ink"),
    "grass": ((122, 190, 80), "ink"),
}
if os.environ.get("COMPANION_COLORS"):
    COLORS = {k: v for k, v in COLORS.items() if k in os.environ["COMPANION_COLORS"].split(",")}
TONES = {"ink": (30, 26, 30), "cream": (242, 230, 202)}

# Each head item keeps its own colours whatever the body's.
ITEMS = {
    "helmet": {"item": (58, 80, 150), "item2": (236, 110, 90)},
    "beanie": {"item": (214, 72, 72), "item2": (246, 240, 228)},
    "cap": {"item": (64, 170, 160), "item2": (246, 240, 228)},
    "crown": {"item": (246, 196, 72), "item2": (226, 60, 80)},
    "headphones": {"item": (232, 226, 214), "item2": (236, 110, 90)},
}

# The camera frames this square of the figure's front plane (metres).
FRAME_X = (-0.050, 0.050)
FRAME_Z = (-0.034, 0.066)
ELEVATION = 7.0

# Eyes, on the body's surface.
EYE_X, EYE_Z = 0.0090, 0.0060
EYE_W, EYE_H = 0.0062, 0.0105


def shade(c, k):
    return tuple(max(0, min(255, round(v * k))) for v in c)


def camera(res, samples):
    sc = bpy.context.scene
    cam, pivot = L.studio(res=res, samples=samples, yaw_deg=0.0)
    cam.data.type = "ORTHO"
    cam.data.ortho_scale = FRAME_X[1] - FRAME_X[0]
    centre = Vector((0.0, 0.0, (FRAME_Z[0] + FRAME_Z[1]) / 2))
    e = math.radians(ELEVATION)
    cam.location = centre + Vector((0, -0.6 * math.cos(e), 0.6 * math.sin(e)))
    cam.rotation_euler = (centre - cam.location).to_track_quat("-Z", "Y").to_euler()
    cam.data.clip_end = 5
    sc.render.film_transparent = True
    bpy.context.view_layer.update()
    return cam, pivot


def body_parts():
    r = 0.03
    body = L.mesh_object("body", L.bean_bm(r))
    feet = [L.nub("foot_l", (-0.011, -0.012, -0.026), 0.0068), L.nub("foot_r", (0.011, -0.012, -0.026), 0.0068)]
    return {"body": [body], "feet": feet}


def scene(texture, color, res, samples, item=None):
    L.reset()
    parts = body_parts()
    colors = {"body": color, "feet": shade(color, 0.86)}
    if item:
        extra = V.ITEMS[item](0.03)
        parts.update(extra)
        colors.update(ITEMS[item])
    V.dress(parts, [], "solid", texture, colors)
    if item:
        # the body only catches the item's shadow and hides what is behind it
        for name in ("body", "feet"):
            for obj in parts[name]:
                obj.is_shadow_catcher = True
        for obj in bpy.context.scene.objects:
            if obj.type == "CURVES" and (obj.name.startswith("body") or obj.name.startswith("foot")):
                obj.hide_render = True
    cam, pivot = camera(res, samples)
    return parts, cam, pivot


def render(path):
    bpy.context.scene.render.filepath = path
    bpy.ops.render.render(write_still=True)


def anchors(cam, pivot, body, res):
    from bpy_extras.object_utils import world_to_camera_view
    sc = bpy.context.scene
    # surface points once, unturned, in the figure's own space
    pivot.rotation_euler = (0, 0, 0)
    bpy.context.view_layer.update()
    points = []
    for sx in (-1, 1):
        loc, _n = V.front_hit(body, sx * EYE_X, EYE_Z)
        points.append(pivot.matrix_world.inverted() @ loc)
    out = []
    for yaw in YAWS:
        pivot.rotation_euler = (0, 0, math.radians(yaw))
        bpy.context.view_layer.update()

        def px(p):
            v = world_to_camera_view(sc, cam, pivot.matrix_world @ Vector(p))
            return v.x * res, (1 - v.y) * res

        eyes = []
        for local in points:
            cx, cy = px(local)
            x0, _ = px((local.x - EYE_W / 2, local.y, local.z))
            x1, _ = px((local.x + EYE_W / 2, local.y, local.z))
            _, y0 = px((local.x, local.y, local.z + EYE_H / 2))
            _, y1 = px((local.x, local.y, local.z - EYE_H / 2))
            eyes.append({"x": cx, "y": cy, "w": abs(x1 - x0), "h": abs(y1 - y0)})
        out.append({"yaw": yaw, "left": eyes[0], "right": eyes[1]})
    pivot.rotation_euler = (0, 0, 0)
    return out


def eye_swatch(texture, tone, res, samples, path):
    L.reset()
    bm = L.sphere_bm(1.0, 64, 32)
    plane = L.mesh_object("swatch", bm)
    plane.scale = (0.02, 0.004, 0.02)
    mat = V.eye_material(texture, TONES[tone])
    plane.data.materials.append(mat)
    if texture == "plush":
        V.fuzz(plane, mat, "yarn")
    cam, pivot = L.studio(res=res, samples=samples, yaw_deg=0.0)
    cam.data.type = "ORTHO"
    cam.data.ortho_scale = 0.012
    cam.location = (0, -0.5, 0)
    cam.rotation_euler = (math.radians(90), 0, 0)
    bpy.context.scene.render.film_transparent = False
    render(path)


def main():
    argv = sys.argv[sys.argv.index("--") + 1:]
    texture, out = argv[0], argv[1]
    res = int(argv[2]) if len(argv) > 2 else 384
    samples = int(argv[3]) if len(argv) > 3 else 96
    only = argv[4] if len(argv) > 4 else "all"
    os.makedirs(out, exist_ok=True)

    def done(path):
        return os.path.exists(path) and os.path.getsize(path) > 0

    if only in ("all", "body"):
        for name, (color, _tone) in COLORS.items():
            paths = [os.path.join(out, f"body-{texture}-{name}-{i}.png") for i in range(len(YAWS))]
            if all(done(p) for p in paths):
                continue
            parts, cam, pivot = scene(texture, color, res, samples)
            if texture == "clay" and name == "pearl":
                json.dump({"res": res, "yaws": YAWS, "eyes": anchors(cam, pivot, parts["body"][0], res)},
                          open(os.path.join(out, "anchors.json"), "w"), indent=1)
            for i, yaw in enumerate(YAWS):
                if done(paths[i]):
                    continue
                pivot.rotation_euler = (0, 0, math.radians(yaw))
                render(paths[i])
                print("SPRITE", paths[i], flush=True)
    if only in ("all", "items"):
        for item in ITEMS:
            paths = [os.path.join(out, f"item-{texture}-{item}-{i}.png") for i in range(len(YAWS))]
            if all(done(p) for p in paths):
                continue
            parts, cam, pivot = scene(texture, COLORS["pearl"][0], res, samples, item=item)
            for i, yaw in enumerate(YAWS):
                if done(paths[i]):
                    continue
                pivot.rotation_euler = (0, 0, math.radians(yaw))
                render(paths[i])
                print("SPRITE", paths[i], flush=True)
    if only in ("all", "eyes"):
        for tone in TONES:
            path = os.path.join(out, f"eye-{texture}-{tone}.png")
            if not done(path):
                eye_swatch(texture, tone, 128, samples, path)
                print("SPRITE", path, flush=True)
    print("ALLDONE", texture, flush=True)


main()
