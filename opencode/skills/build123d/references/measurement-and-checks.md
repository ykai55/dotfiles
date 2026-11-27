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

## Reconstructing a reference mesh

Pick a section that distinguishes the candidate features. A rounded silhouette in
plan can still have a straight side wall with two chamfers through its thickness.
Overlay mating parts at the **same height in the same assembly frame**; take a second
section before interpreting an isolated thickness reading as a pocket or fitting a loft.

For a whole-part comparison, sample by surface area and measure in both directions.
New-to-reference alone can miss removed material; raw vertex averages overweight finely
tessellated fillets. Report the sampled maximum as a sample, not a bound. Inspect small
functional features directly even when the overall percentile looks excellent.

## Checking a delivered print project

Reopen the actual 3MF. A tray and a floating lip can be two individually watertight
shells under one object name: check connectivity for every part intended to print
as one piece. Use assembly transforms for travel checks and build-item transforms
for bed contact, plate bounds and spacing between parts. Slice the delivered project
at the intended layer height to see whether small teeth and thin walls survive.

Render the exposed guide, the partially inserted slider and a close-up of both mating
features from that file. Open the resulting images and check that the feature in
question is visible; label section cuts so they cannot be mistaken for missing material.

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

**Carry a control group.** Move one part 200 mm away and measure the same pair: the answer
must be exactly 0. Without it a non-zero reading is unattributable — a wrong result and a
broken instrument look the same.

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

### Coupled motion

A threaded or geared joint moves in two axes at once. Test it the way it turns:

```python
for step in range(24):
    angle = step * 15.0
    moved = nut.rotate(bd.Axis.Z, angle).moved(
        bd.Location((0, 0, angle / 360.0 * PITCH))
    )
    clash = moved & screw
    print(angle, clash.volume if clash else 0.0)
```

- **Rotating without advancing always interferes**, and that is not a defect.
- **Rotate the right way.** The same right-hand pair reads 0.037 cm³ in one direction and
  0.00003 in the other.
- **Test a full turn**, not a few positions: a phase error shows up somewhere in the 360°,
  not necessarily at 0°.

### Insertion depth and seating

An interface that is clear in a straight duct may touch in a bend. Sweep the plug into the
real cavity one step at a time and intersect at each step
([fits](fits-and-snap-fits.md#measure-the-insertion-depth-dont-assume-it)). For seating, tilt
one side by 0.5°: a face that is genuinely in contact shows a small finite clash, not 0.

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

## Acceptance checklist

Every solid that leaves the script, before it is exported:

| Check | Reads |
| --- | --- |
| `len(part.solids()) == 1` | One body, not two that happen to share a bounding box |
| `len(part.shells()) == 1` | No reversed or nested shell |
| `part.is_valid` and `part.is_manifold` | Topology holds |
| Export, then read the mesh back | Watertight, consistent normals — the slicer repairs silently |
| `bounding_box().min.Z == 0` | The part is seated on the build plate |
| Slice it | Return code 0, no warnings, support where you expect it |
| Look at the sliced preview | The one check the numbers cannot make |

Volume and bounding box alone miss a split solid and a part floating above the plate; both
have shipped from projects whose numbers looked right.

## Write checks that can fail

- **Trust a check only after seeing it fail.** Plant what it guards against — a plate
  through a thin cover, a pin a little out of its bore — and watch the check report it. A
  check that has only ever passed has shown nothing.
- **Boxes and sampled points pick the pairs worth measuring; they are never the verdict.**
  A thin wall has almost no volume for a point to land in.
- **Report what the check could not see** beside what it passed: the poses not taken, the
  pairs that could not be measured, the path between two poses checked only at its ends.
- **A failed or inconclusive check is not a pass.** Say which it was.
