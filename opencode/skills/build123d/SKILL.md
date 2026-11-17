---
name: build123d
description: Write and debug parametric CAD in Python with build123d — parts, assemblies, joints, threads and printed fasteners, 2D profiles and DXF cut files, materials and colours, and the measurements that prove the geometry matches the request. Use when creating or editing a build123d script, exporting STEP/STL/GLB/DXF, selecting faces or edges, placing or mating parts, designing for FDM printing (overhangs, snap-fits, clearances), or diagnosing a boolean, fillet, loft, thread, placement or kernel failure.
---

# Parametric CAD with build123d

build123d is a Python B-rep CAD library on OpenCascade (via `OCP`). A model is a function
that returns a `Shape`; geometry stays exact — planes, cylinders, splines — and a mesh is
only ever made at export time. Units are millimetres and **+Z is up** by convention: nothing
enforces it, so pick it and state it.

## Setup

```bash
uv venv .venv && uv pip install build123d
uv pip install bd_materials ezdxf   # optional: engineering materials, reading DXF
```

build123d 0.13 supports Python 3.11–3.14. `import build123d` loads OCP and takes a second
or two; that is the library's own import, not a mistake.

There is no built-in viewer: to look at a result, export a GLB/STL and open it in any
viewer, or write a DXF/SVG. A picture is for people, never for checking a dimension.

## The shape of a script

```python
import build123d as bd

WIDTH, DEPTH, THICKNESS, BORE = 60.0, 40.0, 6.0, 8.0


def bracket_body(width, depth, thickness, bore):
    body = bd.Box(width, depth, thickness)
    body = body - bd.Cylinder(bore / 2, thickness)
    body.label = "bracket"
    body.color = "#4a6fa5"
    return body


def bracket():
    return bracket_body(WIDTH, DEPTH, THICKNESS, BORE)


if __name__ == "__main__":
    part = bracket()
    print(part.bounding_box().size, part.volume)
```

- **Dimensions are arguments of a plain function**, with named constants at module level.
  A second configuration is a second call, not a global switch.
- **A model takes no user input beyond its arguments.** No `input()`, no environment
  variables, no clock, no randomness: a script that reads them produces a different part
  each run and cannot be checked.
- **Keep the build under `if __name__ == "__main__":`** so the module can be imported for
  its helper functions without building anything.

Export is a separate step on a built shape, never part of the model:

```python
from build123d import export_step, export_stl, export_gltf

export_step(part, "out/bracket.step")
export_stl(part, "out/bracket.stl", tolerance=0.05)   # linear deflection, mm
export_gltf(part, "out/bracket.glb")
```

`export_stl` defaults to `tolerance=0.001` mm — far finer than most parts need, and the
file size follows the surface area. Start at 0.02–0.1 for a visual, and go finer only where
a fit is judged from the mesh. `export_brep` and `export_obj` exist too; SVG and DXF go
through `bd.exporters.ExportSVG` / `ExportDXF` ([profiles and DXF](references/profiles-and-dxf.md)).

## Two ways to describe geometry

Both are first-class and return the same kinds of `Shape`. Pick one per file.

**Algebra mode** — shapes as values, operators as operations. Good for composition,
helper functions and parametric families:

```python
plate = bd.Box(80, 50, 6)
bore = bd.Cylinder(3, 6)
part = plate - bore
part = bd.Pos(10, 0, 0) * part                 # placement is multiplication
```

**Builder mode** — a sketch-and-feature history inside `with` blocks. Good for
profile-driven parts and for `Mode.SUBTRACT`/`Mode.INTERSECT` sketches:

```python
with bd.BuildPart() as p:
    with bd.BuildSketch(bd.Plane.XY):
        bd.Rectangle(80, 50)
        bd.Circle(3, mode=bd.Mode.SUBTRACT)
    bd.extrude(amount=6)
part = p.part
```

The two mix at the boundary: `p.part` is an ordinary shape, and a shape built in algebra
mode enters a builder with `bd.insert(shape)` (build123d 0.13 renamed the older
`bd.add`).

## Operations you will reach for

| Operation | Algebra call | Note |
| --- | --- | --- |
| Extrude | `bd.extrude(profile, 6, dir=None, both=False, taper=0.0)` | along the profile's normal; `dir` overrides the direction |
| Revolve | `bd.revolve(profile, axis=bd.Axis.Z, revolution_arc=360)` | the profile must lie in a plane that contains the axis (`bd.Plane.XZ * sketch`), or the sweep has no volume |
| Loft | `bd.loft(sections, ruled=False)` | sections in order; `ruled=True` joins corresponding edges straight instead of fitting one spline |
| Sweep | `bd.sweep(section, path)` | the section travels along a wire |
| Offset / shell | `bd.offset(shape, -2, openings=top_faces)` | a face grows or shrinks; a solid with a negative amount and `openings` becomes a shell |
| Split | `bd.split(shape, bisect_by=plane, keep=bd.Keep.TOP)` | keeps one side of a plane |
| Section | `bd.section(part, section_by=plane, height=0)` | a cross-section, returned as a `Sketch` |
| Fillet / chamfer | `bd.fillet(edges, radius)` / `bd.chamfer(edges, length)` | edges taken from the shape you just built |
| Mirror | `bd.mirror(shape, about=bd.Plane.YZ)` | new geometry, not a placement |
| Scale | `bd.scale(shape, by=2)` | `by` also takes `(x, y, z)` |
| Make a face | `bd.make_face(edges)` | closes wires into a `Face`/`Sketch` |
| Thicken | `bd.thicken(face, 3)` | gives a surface a thickness |

In builder mode the same functions take no first argument — the context supplies the shape
(`bd.extrude(amount=6)` inside `BuildPart`).

## Rules that prevent most failures

1. **Select from the shape you just made, after its last operation.** Every operation
   returns new topology wherever it touched the solid; an edge held from before is no
   longer the solid's own edge, so a fillet silently skips it and a chamfer fails.
2. **Select by geometry, not by number.** Normal, axis, plane, position and curve type
   survive edits; face and edge indices do not. See
   [modeling patterns](references/modeling-patterns.md#selecting-edges-and-faces).
3. **`align=` decides the datum.** `align=None` keeps the primitive's raw datum — a box
   starts at a corner, a cylinder at Z=0 — while `Align.CENTER` centres that axis.
4. **`.moved()` composes, `.located()` replaces.** `Location * shape` composes like
   `.moved()`, so a `.located()` after a rotation throws the rotation away.
5. **A placement shares its geometry.** `Pos(...) * part`, `.moved()` and `.located()`
   re-place the same B-rep; a part used ten times is one solid placed ten times. Copy with
   `BRepBuilderAPI_Copy` when you need an independent body.
6. **Test every boolean result before building on it.** A cut that removes everything
   returns an empty `Compound` (and `&` of two solids can return `None`); labelling or
   placing that fails a few lines later with a confusing error.
7. **A valid shape is not a correct shape.** `is_valid` runs BRepCheck; it does not prove
   the orientation, the volume, or that the part is the one asked for.
8. **Never read a dimension off a picture.** A render shows arrangement and appearance.
   Measure with `bounding_box()`, `distance_to()`, `volume`, `area`.
9. **Check a numeric claim where the number is.** An `assert` next to the geometry beats a
   second script that recomputes the same formula.
10. **Delete the assumption, not the feature.** A retry that shrinks a specified radius or
    skips a fillet has built a different design: say so instead.

## Working loop

1. Pin the units, the coordinate frame and the mating datum before the first line.
2. Write the dimensions as named constants; give helper functions the varying ones.
3. Build the body, then place features by datum and select them geometrically.
4. Run it — it is plain Python — and assert the numbers you were given.
5. Measure the derived numbers that matter ([measurement](references/measurement-and-checks.md)).
6. Export the formats asked for; report paths and any check that did not run.

## References

| Question | Read |
| --- | --- |
| Selecting faces or edges; labels and assemblies; placement frames; boolean, loft and fillet failures; validity | [modeling-patterns.md](references/modeling-patterns.md) |
| Placing and mating parts; build123d joints, ranges and `connect_to` | [assemblies-and-joints.md](references/assemblies-and-joints.md) |
| Dimensions, clearances, interference, mass, geometry diagnostics | [measurement-and-checks.md](references/measurement-and-checks.md) |
| 2D profiles, flat patterns, kerf, DXF export and validation | [profiles-and-dxf.md](references/profiles-and-dxf.md) |
| Screw threads, threaded caps, printed fasteners, turning fits | [threads-and-fasteners.md](references/threads-and-fasteners.md) |
| Print orientation, overhangs, walls and infill, readiness checklist | [printability.md](references/printability.md) |
| Clearances, bayonets, snap tabs, clamps, assembly order | [fits-and-snap-fits.md](references/fits-and-snap-fits.md) |

## Attribution

The patterns and measured failure modes here are adapted from the CAD skill of
[earthtojake/text-to-cad](https://github.com/earthtojake/text-to-cad) (MIT). Everything here
is stock build123d: the `cad` command, model documents and viewer that repository builds
around its skills are a separate layer, and nothing in this skill needs them.
