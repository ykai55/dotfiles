# Measuring and checking a model

Read this when the request names a dimension, a fit, a clearance or a mass, or when you
need to know whether two parts actually meet.

A check is a small script that imports the model and measures what it returns, so it tests
the model as its code is now. Keep checks worth keeping beside the part (`checks/`), and
throwaway ones in a scratch folder.

## Where a check lives

- **A number the request gave** belongs in an `assert` next to the geometry it constrains,
  in the function that builds it:

  ```python
  import math

  import build123d as bd

  def bracket_body(width, depth, thickness, bore):
      body = bd.Box(width, depth, thickness) - bd.Cylinder(bore / 2, thickness)
      expected = width * depth * thickness - math.pi * (bore / 2) ** 2 * thickness
      assert abs(body.bounding_box().size.X - width) < 1e-6
      assert abs(body.volume - expected) < 1e-3 * expected
      return body
  ```

  Assert on what the geometry measures, never on the constant itself: `assert width == 60`
  proves that Python can compare two numbers, while `assert abs(size.X - width) < 1e-6`
  catches a primitive that was called with the wrong argument.

- **A number the design implies** (a fit, a clearance, a mass) gets a check script that
  calls the model and measures the result.
- **A number nothing asks for** needs no check. Measuring everything costs kernel time and
  buys nothing.

## Single-shape numbers

```python
size   = part.bounding_box().size            # (X, Y, Z) of the axis-aligned envelope
lo, hi = part.bounding_box().min, part.bounding_box().max
volume = part.volume                         # mm³
area   = part.area                           # mm²
centre = part.center()                       # centre of mass by default
```

- **`bounding_box()` is tight but not cheap** (`optimal=True`): tens of milliseconds a
  part on curved geometry.
- **`bounding_box(optimal=False)`** is a fast, conservative box. It is safe for ruling
  pairs out, and wrong for saying where a part is: on a piece cut from a lofted face it
  measured several times too wide (a 13 mm strip read 39 to 72 mm).
- **`.volume` integrates at a fixed order** — exact on planes, cylinders and spheres and a
  little high on lofted and spline solids (0.4 % measured on one). A check that guards a
  boolean by volume allows a per cent, or integrates adaptively.
- **A negative or wildly wrong volume means inside-out geometry**, not a measurement
  error: rebuild the solid (a revolved profile instead of stacked booleans) before
  trusting it.

Elements carry their own numbers:

```python
edge = part.edges().filter_by(bd.GeomType.CIRCLE)[0]
edge.radius, edge.arc_center                  # a hole's radius and where it is
face = part.faces().filter_by(bd.GeomType.CYLINDER)[0]
face.radius, face.axis_of_rotation            # axis_of_rotation is None on a plane
face.normal_at()                              # normal at the face's centre
face.center()
```

## Distance, contact and clearance

```python
gap = a.distance_to(b)                                  # mm, 0 when touching or overlapping
gap, point_a, point_b = a.distance_to_with_closest_points(b)
```

- **0 means touching or overlapping** — distance cannot tell the two apart. Use the
  intersection volume (below) to tell them apart.
- **A part meant to rest on, bolt to or bear against another** measures distance 0.
  Anything above 0 is a gap, and the part floats.
- **Pick the pair before measuring it.** A solid-to-solid distance on vendor parts with
  thousands of faces takes seconds. Rule pairs out by bounding box first, and measure from
  a point (`bd.Vertex(*p)`) where a point answers the question.
- **A screw pattern's spacing** is the distance between the `arc_center` of its circular
  edges.

## Interference

The exact question is "do these two solids share volume":

```python
def overlap_volume(a, b):
    """mm³ shared by two solids; 0 when apart or merely touching."""
    if a.bounding_box().disjoint(b.bounding_box()):
        return 0.0
    common = a & b
    return 0.0 if common is None else common.volume
```

- **`bounding_box().disjoint()` is conservative** — its converse `intersects()` is true for
  boxes that merely touch, so a box test only ever rules pairs out.
- **An empty boolean result is an empty `Compound` or `None`**, not an error; both mean 0.
- **A pair meant to overlap is named, never inferred.** A press-fit pin in its bore and a
  bearing in its housing share volume on purpose; list them as fits instead of reporting
  them as collisions.
- **Measured overlap above ~0.001 mm³ is real**; less than that is the rounding along a
  face where two parts meet.
- **Check the poses that matter.** Parts clear as built can collide at a joint's other end:
  place the assembly at each extreme and measure again.

```python
def collisions(tree, fits=()):
    parts = list(tree.leaves)
    for i, a in enumerate(parts):
        for b in parts[i + 1:]:
            if {a.label, b.label} <= set(fits):
                continue
            volume = overlap_volume(a, b)
            if volume > 1e-3:
                print(f"{a.label} shares {volume:.1f} mm³ with {b.label}")
```

`tree.leaves` returns the parts placed where they stand, each with its own `label` — which
is why labelling every placement matters.

## Mass properties

```python
from bd_materials import metals

part.material = metals.aluminum()     # 2700 kg/m³
part.mass()                           # grams
part.center()                         # centre of mass
```

- **`mass()` needs a material** and raises `Shape's density is missing` without one.
- **`mass()` returns grams** (`mass_unit=bd.Unit.G` by default), whatever the geometry's
  units; `.volume` stays mm³.
- **Inertia and an explicit density** come from OCP:

  ```python
  from OCP.BRepGProp import BRepGProp
  from OCP.GProp import GProp_GProps

  props = GProp_GProps()
  BRepGProp.VolumeProperties_s(part.wrapped, props)
  volume, centre, inertia = props.Mass(), props.CentreOfMass(), props.MatrixOfInertia()

  density = 2.7e-6              # kg/mm³, for aluminium
  mass_kg = volume * density    # the inertia tensor scales the same way
  ```

  OCP returns geometric properties: mm³, mm, and mm⁵ for the second moment. Multiply by
  the density in kg/mm³ to get kg and kg·mm².

## Geometry diagnostics

```python
part.is_valid          # bool: BRepCheck's topology verdict (a property, not a call)
part.is_manifold       # bool: every edge shared by the expected number of faces
part.solids()          # solids in a shape or compound
```

For detail, ask the checker directly:

```python
from OCP.BRepCheck import BRepCheck_Analyzer

analyzer = BRepCheck_Analyzer(part.wrapped)
analyzer.IsValid()                     # same verdict as part.is_valid
```

- **`is_valid` is a gate, not a proof.** It does not check orientation, the requested
  shape, or that a later boolean will behave.
- **A reversed solid passes topology checks** with negative volume, and aggregate volumes
  can cancel. Check signed volume per solid.
- **Closed is not the same as manifold.** `is_manifold` checks that every edge is shared
  by the expected number of faces; a shell can be closed and still fail that.
- **A face the mesher cannot triangulate** is usually a sliver a boolean left; it shows as
  a hole or a warning wherever the model is drawn. Fix the sliver, not the viewer.

## Write checks that can fail

- **Trust a check only after seeing it fail.** Plant what it guards against — a plate
  through a thin cover, a pin a little out of its bore — and watch the check report it. A
  check that has only ever passed has shown nothing.
- **Boxes and sampled points pick the pairs worth measuring; they are never the verdict.**
  A thin wall has almost no volume for a point to land in.
- **Report what the check could not see** beside what it passed: the poses not taken, the
  pairs that could not be measured, the path between two poses checked only at its ends.
- **A failed or inconclusive check is not a pass.** Say which it was.
