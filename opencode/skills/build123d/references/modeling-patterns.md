# Modeling patterns and kernel pitfalls

Read this when constructing or repairing geometry: picking faces or edges, labelling an
assembly, placing parts, or working out why a boolean, fillet, loft or revolve did not do
what the code says.

## Contents

- [Selecting edges and faces](#selecting-edges-and-faces)
- [Labels and assemblies](#labels-and-assemblies)
- [Colour and materials](#colour-and-materials)
- [Placement and frame pitfalls](#placement-and-frame-pitfalls)
- [Booleans](#booleans)
- [Lofts, revolves, sweeps](#lofts-revolves-sweeps)
- [Performance](#performance)
- [Validity and visual artifacts](#validity-and-visual-artifacts)

## Selecting edges and faces

`shape.edges()`, `.faces()`, `.vertices()`, `.wires()` and `.solids()` return a `ShapeList`,
which filters and sorts:

```python
top    = body.faces().sort_by(bd.Axis.Z)[-1]            # highest face in world Z
holes  = body.faces().filter_by(bd.GeomType.CYLINDER)   # cylindrical faces
verts  = body.vertices().group_by(bd.Axis.Z)[0]         # lowest Z level, as a group
walls  = body.faces().filter_by(bd.Plane.XZ)            # parallel to the XZ plane
ring   = body.edges().filter_by_position(bd.Axis.Z, 5.9, 6.1)
```

- **`filter_by`** takes an `Axis` (parallel to it), a `Plane` (parallel to it), a
  `GeomType` (`LINE`, `CIRCLE`, `PLANE`, `CYLINDER`, `BSPLINE`, …) or a predicate.
- **`sort_by`** orders along an `Axis` (by the centre of each element by default) or by a
  key; **`group_by`** collects elements at the same coordinate into sub-lists, which is how
  you get "the four holes at this height" without counting.
- **`filter_by_position(axis, minimum, maximum)`** is the honest way to say "these edges
  are at Z=6": a `sort_by(...)[-1]` picks one extreme, which is wrong the moment two
  features share it.
- **Select by a datum, not by a count.** A face index changes when an earlier operation
  changes; a normal, an axis, a plane or a coordinate does not.

**Re-select after anything that changes topology.** An operation returns new native
topology wherever it modified the solid, so an edge held from before may not be the
solid's own edge. Shape equality compares native handles, so `edge in solid.edges()` is
true only for the solid's own edge. Consequences:

- A **fillet silently skips** such an edge (and fails when none is left).
- A **chamfer fails** on it.
- Select a fillet's or chamfer's edges from the solid it rounds, after that solid's last
  operation.

```python
edges = body.edges().filter_by(bd.Axis.Z).sort_by(bd.Axis.X)[-4:]
part = body.fillet(2, edges)                       # method: radius first
part = bd.fillet(edges, radius=2)                  # free function: edges first
```

`part.fillet(radius, edge_list)` and `bd.fillet(edge_list, radius)` take their arguments in
the opposite order — a frequent source of `got multiple values for argument 'radius'`.

## Labels and assemblies

A label names a part, an assembly or a placement. Labels are what a reader and a check use
to find a part, and they travel into the formats that carry names (STEP product and
instance names, for one).

```python
base.label = "base"
lid.label = "lid"
screws = [bd.Pos(x, y, 30) * screw for x, y in HOLES]
for placed in screws:
    placed.label = "m3_screw"          # siblings sharing a label are m3_screw.1, m3_screw.2, ...
fasteners = bd.Compound(children=screws, label="fasteners")
assembly = bd.Compound(children=[base, lid, fasteners], label="electronics_enclosure")
```

- **Label every placement**, not just every part: the label is the only handle a reader
  (or a check) has on one occurrence of a repeated part.
- **A feature fused into or cut from a body keeps no label of its own.** A label on a
  shape names what later features make of it, so `plate + boss` gives the boss's faces
  names under `plate`.
- **Nest a functional group as a labelled `Compound`** — a fastener set, a sub-assembly —
  rather than flattening everything into one list.
- A `Compound` of `children` is how an assembly is returned. Keep child order stable: it
  is what sibling numbering (`.1`, `.2`) follows.

## Colour and materials

`Shape.color` and `Shape.material` are build123d's own:

```python
from bd_materials import metals, plastics

body.material = metals.aluminum()          # sets colour too
body.color = "#2E3742"                     # hex, colour name, Color, or 0–1 sRGB floats
body.material.pbr = body.material.pbr.override(roughness=0.35, metalness=0.9)
insert.material = plastics.abs_()
```

- `shape.color` takes a hex string (`"#f2f0ea96"`, alpha included), a colour name, a
  `bd.Color`, or 0–1 floats `(0.95, 0.94, 0.92)`. It refuses 0–255 integers.
- `shape.material` takes a `bd_materials` object or a name such as `"alu_g6061_t6"`;
  `bd_materials.material_names()` lists them. `bd_materials` is a separate package
  (`pip install bd_materials`); its families are `metals`, `plastics`, `resins`, `wood`,
  `glass`, `paper` and `textile`, each material a function of it.
- **Set materials after the geometric operations**, then any explicit colour: the material
  setter overwrites the colour. There is no `metals.steel()`; plain steel is
  `metals.mild_steel()`.
- **Density comes with the material**, in kg/m³. `shape.mass()` needs a material (it raises
  `Shape's density is missing` without one) and returns **grams**; `.center()` defaults to
  the centre of mass, so both follow the material you set.
- **A transmissive look is one fade, not two.** A see-through part lying on another (a
  window on its liner, two surfaces from one loft) draws mottled in any renderer that
  meshes each part on its own, because the two triangle sets cross. Leave a gap wider than
  both meshes' error — half a millimetre on an 80 mm part, a millimetre or two on a 400 mm
  one.
- **Smoked glass is a clear look with a tint**, not a dark colour with low transmission:
  give it `transmission=1` and a mid grey as its colour.
- **What survives an export depends on the format.** STEP can carry names, colour and
  density; GLB carries appearance; STL carries geometry alone. Check the exporter's
  options and the file the consumer actually receives, not the model in memory.

## Placement and frame pitfalls

- **Rotation frame:** an Euler-angle spelling does not reveal the local rotation axis.
  Prefer explicit direction vectors or an axis-angle construction; a valid loft can still
  join incorrectly oriented sections.
- **Primitive alignment:** `align=None` keeps the primitive's raw datum, not its centre —
  a cylinder is based at Z=0, a box starts at a corner. `Align.CENTER` per axis is what
  "centred" means.
- **Absolute versus relative placement:** `.located(loc)` replaces the location;
  `.moved(loc)` and `Location * shape` compose it. A `.located()` after a rotation can
  discard the orientation.
- **A face's winding sets its extrusion:** `bd.extrude(face, h)` runs along the face's
  normal, so a `Polygon` whose points run clockwise (seen from +Z) faces −Z and extrudes
  down. Give `dir=(0, 0, 1)` or list the points anticlockwise.
- **A location is applied in the parent's frame.** `bounding_box()` is the axis-aligned
  envelope in the shape's current frame; inside a tree, walk the leaves and accumulate
  placements rather than reading a nested node's box.

```python
box = bd.Solid.make_box(1, 1, 1)
rotated = box.rotate(bd.Axis.Z, 90)
placed = bd.Location((5, 0, 0)) * rotated      # keeps the rotation
```

- **Mirroring makes new geometry, not a placement.** `bd.mirror(part, about=bd.Plane.YZ)`
  is another shape; a mirrored model that must stay in sync is better as a shared factory
  function taking a side argument.

## Booleans

- **A boolean that removes everything returns an empty `Compound`** — zero solids, zero
  volume — and it does not raise. Placing it gives another empty shape, and the label,
  export or measurement that follows silently does nothing or reads 0, which is how a
  consumed part is discovered three lines later. Test `result.solids()` before building on
  it.
- **Near tangency or coincident boundaries** are the usual cause of a boolean that
  "succeeds" with the wrong region or the wrong solid count. A direct profile construction
  of the same shape often avoids the intersection entirely. `is_valid` does not catch it,
  and `&`, `-` and `+` make no promise of their own: where a result on a lofted or spline
  solid matters, hold it to what it must be — its `bounding_box()` inside both operands',
  its volume near the expected one — before building on it.
- **Batch independent cuts** rather than cutting one tool at a time: each boolean rebuilds
  the intersection network. Overlapping tools may need staged cuts, and oversized tool
  extents create needless intersections.
- **Tangent chains and complex outlines** can make OCCT's 3D fillets fail or crash. A
  profile bevel or a separately constructed transition is the workaround.
- **A reversed solid** can pass topology checks with negative volume, and aggregate
  volumes can then cancel. Check signed volume per solid; build the shape another way
  (a revolved profile instead of stacked booleans) before trusting its volume or a fit.

## Lofts, revolves, sweeps

- **Sampled sections need consistent feature order**, edge correspondence and orientation;
  equal sample counts alone do not guarantee it. A disconnected or self-crossing section
  produces misleading downstream loft failures, so check the section wires as well as face
  validity. A ruled loft (`ruled=True`) can localize a smooth-loft failure.
- **A smooth loft is one spline through all its sections**, so it goes where no section
  says. Measured: two sections added near the top of a 400 mm form moved its surface 77 mm
  near the bottom, and the result was still valid. Space sections evenly and carry a
  change in their values, not in more of them — or use `ruled=True`, which goes only where
  the sections say. Measure a clearance from the solid, never work it out from the
  sections.
- **A loft that ends in a `Vertex` ends in a cone point**, not a rounded cap, and on a
  blunt end it can stand millimetres past the point. Rows added near the point to round it
  move the rest of the form. A shaded picture hides both; a render shows them.
- **A revolved polyline is one conical face per segment**, and a shaded picture draws a
  band for each. A profile that will be seen should be arcs or a spline — one face.
  `EllipticalCenterArc` and `CenterArc` take `start_angle` and `arc_size`, not a start and
  an end angle.
- **Dense periodic splines:** OCP 7.9 has failed taper, inward offset and coincident-face
  fusion on some densely sampled profiles; a simpler representation or an explicitly
  constructed offset works around it.

## Performance

- **Cost follows surface area and detail**, not part count: a few large freeform shells can
  cost more than hundreds of small parts.
- **Split a slow build into cached stages.** If a sub-assembly takes seconds and an edit
  elsewhere leaves it alone, compute it once (module-level cache, `functools.lru_cache` on
  a function of its values, or a saved intermediate) and compose the result. Cache a
  *stage*, never an individual part, and only when it actually costs seconds.
- **Reuse a shape instead of rebuilding it**: a placement shares geometry, so one bolt
  solid placed twenty times costs one bolt.
- **Filter before exact queries.** Two bounding boxes that do not meet rule a pair out of
  an interference check; exact solid-to-solid distance on vendor parts with thousands of
  faces takes seconds per pair.

## Validity and visual artifacts

- **`shape.is_valid`** (a property) runs BRepCheck: a topology gate, not a proof of orientation, of
  the requested shape, or of suitability for a later boolean.
- **A validity gate around a fragile operation** (try the fillet, keep it only if the result
  is valid and its volume is near what you expect) costs kernel time on every run: use it
  where the operation is known to be fragile, not everywhere.
- **A periodic cylinder or revolved face has a seam edge** that appears in CAD linework.
  It is not a crack.
- **A face the mesher cannot triangulate** is usually a sliver a boolean left. It shows as
  a hole or a warning wherever the model is drawn; fix the sliver, not the viewer.
- **A shaded view hides what a silhouette shows.** Check the outline and a section, not
  only the default isometric.

For measuring what you built, see [measurement and checks](measurement-and-checks.md).
