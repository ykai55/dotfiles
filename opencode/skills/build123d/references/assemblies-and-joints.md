# Assemblies, placement and joints

Read this when placing parts, choosing a mating datum, writing a mechanism, or checking
that two parts actually sit where the design says.

## Frames and transforms

A `Location` is a rigid transform (position + rotation). Three ways to apply one:

```python
placed = bd.Pos(-30, 0, 12) * part         # composes: keeps the part's own rotation
placed = part.moved(bd.Rotation(0, 0, 45)) # composes too
placed = part.located(loc)                 # replaces the location entirely
```

- **`.moved()` composes, `.located()` replaces.** After a rotation, `.located()` throws
  the rotation away unless the new location carries it.
- **A placement shares geometry.** `Pos(...) * part`, `.moved()` and `.located()` place
  the same B-rep again, so a part used ten times is one solid placed ten times — in memory
  and in every file. Use `BRepBuilderAPI_Copy` when a body must be independent.
- **A joint's `Axis` and a `RigidJoint`'s location are in world coordinates**, not the
  part's own frame. An axis written in the part's frame is `axis.located(part.location)`.
- **Mirroring makes new geometry**, not a placement: `bd.mirror(part, about=bd.Plane.YZ)`
  is a shape of its own. For left/right pairs that must stay in sync, write one factory
  function and call it with a side argument.

Labels give the tree its names:

```python
left  = bd.Pos(-pitch / 2, 0, height) * spacer()
left.label = "spacer_left"
right = bd.Pos( pitch / 2, 0, height) * spacer()
right.label = "spacer_right"
pair  = bd.Compound(children=[left, right], label="spacer_pair")
```

## Joints

build123d has five joint types. A joint belongs to a part and lives in `part.joints`, a
dict keyed by its label:

| Joint | Constructor | Takes |
| --- | --- | --- |
| `RigidJoint` | `(label, to_part, joint_location=Location())` | a `Location` |
| `RevoluteJoint` | `(label, to_part, axis=Axis.Z, angle_reference=None, angular_range=(0, 360))` | an `Axis` |
| `LinearJoint` | `(label, to_part, axis=Axis.Z, linear_range=(0, inf))` | an `Axis` |
| `CylindricalJoint` | `(label, to_part, axis=Axis.Z, angle_reference=None, linear_range=(0, inf), angular_range=(0, 360))` | an `Axis` |
| `BallJoint` | `(label, to_part, joint_location=Location(), angular_range=((0,360),(0,360),(0,360)), angle_reference=Plane.XY)` | a `Location` |

`connect_to` belongs to the joint that stays; the moving part's joint is its argument:

```python
parent.joints[name].connect_to(child.joints[name], angle=30)      # revolute, degrees
parent.joints[name].connect_to(child.joints[name], position=20)   # linear, millimetres
parent.joints[name].connect_to(child.joints[name], angles=(10, 0, 0))  # ball
```

- **`connect_to` moves the child's part** into the frame the parent's joint defines.
  Connect before putting both parts in the `Compound` that holds them.
- **`angular_range` and `linear_range` are limits**, in degrees and millimetres, and the
  value passed to `connect_to` must lie inside them.
- **Each joint takes the datum its motion needs.** A `RigidJoint` fixes a part; give the
  child a `RigidJoint` at the frame the moving joint places it at and turn it with the
  moving joint.
- **A `LinearJoint` places its child by position alone**; the child's `RigidJoint` should
  be `bd.Location(slide_axis.position)` — the slide's position with **no rotation** — or
  the child stands on end. On a rail that is itself turned, use
  `rail.location * bd.Location(slide_axis.position)`.
- **Snap directions read from geometry.** A direction carrying `1e-17` of noise gives a
  plane another X, so the child turns at zero. Round anything within `1e-9` of zero.
- **A joint exists because the mechanism moves there.** A prismatic joint invented per
  part to slide it out for an exploded view is not a mechanism; move the parts.

### A hinge, placed by its joint

```python
import build123d as bd

ELBOW = ((40, 0, 20), (0, -1, 0))       # origin and direction: +angle lifts the arm

base = bd.Box(100, 60, 20, align=(bd.Align.CENTER, bd.Align.CENTER, bd.Align.MIN))
arm = bd.Pos(40, 0, 20) * bd.Box(
    120, 20, 10, align=(bd.Align.MIN, bd.Align.CENTER, bd.Align.MIN)
)
base.label, arm.label = "base", "arm"

elbow = bd.RevoluteJoint("elbow", base, axis=bd.Axis(*ELBOW), angular_range=(-10, 120))
bd.RigidJoint("elbow", arm, elbow.location)     # the hinge's own frame: at 0 the arm stays put
elbow.connect_to(arm.joints["elbow"], angle=30)  # as built: 30°

assembly = bd.Compound(children=[base, arm], label="hinge")
```

- **A `RigidJoint` elsewhere, or one whose Z is not the joint's axis, is a plain
  placement**: `connect_to` puts the child on the joint, and nothing is checked. A wheel
  built at the origin with `bd.RigidJoint("axle", wheel, bd.Location())` goes onto its
  axle that way.
- **Connect the part before you nest it.** `connect_to` sets the child's own location;
  once that child sits inside a nested `Compound`, its placement is relative to that
  compound and the joint no longer lands where the numbers say. Place the parts, connect
  them, then build the compounds around them.

## Checking an assembly

A joint places parts; it does not promise they clear each other. Check the relationship
you intended, by naming the features:

```python
import build123d as bd

fixed = base.faces().sort_by(bd.Axis.Z)[-1]      # the base's top
moving = lid.faces().sort_by(bd.Axis.Z)[0]       # the lid's underside
n = fixed.normal_at().normalized()
m = moving.normal_at().normalized()

signed_gap_mm = (moving.center() - fixed.center()).dot(n)
parallel_error_deg = min(n.get_angle(m), n.get_angle(-m))
print(signed_gap_mm, parallel_error_deg)
assert abs(signed_gap_mm - 0.5) < 0.01
assert parallel_error_deg < 0.1
```

- **A plane gap and a parallelism test do not establish lateral alignment** or that two
  face footprints overlap; and accepting either parallel direction misses a flipped part.
- **A screw pattern's spacing** is between the analytic centres of its circular edges
  (`edge.arc_center`), not between sampled points.
- **Clear space between bodies** is the minimum distance, `a.distance_to(b)` or
  `a.distance_to_with_closest_points(b)`; zero means touching or overlapping, and it does
  not tell the two apart — see [interference](measurement-and-checks.md#interference).
- **A part meant to rest on another** touches it: distance 0, measured on the placed parts
  in the assembly's frame.
