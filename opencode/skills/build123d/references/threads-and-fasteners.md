# Threads and printed fasteners

Read this when a part needs a screw thread, a threaded cap, or a printed fastener that has
to actually turn after it comes off the bed.

A thread is a helical rib plus the groove it runs in. Both halves are built from nominal
numbers and then **verified by motion** — a thread that looks right and does not turn is the
normal failure, not the exception.

## Build the rib

Place the section on the helix's own start frame and sweep. This is the construction that
reproduces the analytic volume in build123d 0.13:

```python
import build123d as bd

PITCH, CORE_R = 3.0, 2.5
PROFILE = [(-0.85, -0.78), (0.5, -0.3), (0.5, 0.3), (-0.85, 0.78)]  # (radial, axial) from CORE_R


def thread_rib(core_r, z0, z1, pad=1.5):
    axial_half = max(abs(v) for _, v in PROFILE)
    assert 2 * axial_half < PITCH, "the section is wider than the pitch: the sweep self-intersects"

    helix = bd.Helix(pitch=PITCH, height=(z1 - z0) + 2 * pad, radius=core_r) \
        .translate((0, 0, z0 - pad))
    p0, t0 = helix @ 0, helix % 0                  # start point, start tangent
    plane = bd.Plane(origin=p0, z_dir=t0, x_dir=(1, 0, 0))

    with bd.BuildPart() as bp:
        with bd.BuildSketch(plane):
            with bd.BuildLine():
                bd.Polyline(*[(core_r + u, v) for u, v in PROFILE], close=True)
            bd.make_face()
        bd.sweep(path=helix)

    # the sweep runs `pad` past each end: cut it back to the nominal length
    return bp.part & bd.Box(4 * core_r, 4 * core_r, z1 - z0) \
        .translate((0, 0, (z0 + z1) / 2))
```

- **The section plane comes from `helix @ 0` and `helix % 0`.** A section placed on a fixed
  plane is carried around the helix by the sweep and comes out twisted — adjacent teeth lean
  opposite ways, and the rib volume reads about twice the analytic value.
- **Radial multi-section loft is the other valid construction** — put the same profile on
  radial planes (`z_dir` tangential, `x_dir` radial) at `θ_i = 2πi/n`, `z_i = z0 + pitch·i/n`,
  and loft them. It reads about 0.91 of the naive analytic volume because the profile's
  centroid is not at the nominal radius (see the volume judge below). Use it when you also
  need to derive the nut's groove from the same section family.
- **The rib sinks into the core.** The profile's inner points are below the core radius (here
  `-0.85`), so union the rib with the shaft and let the boolean keep the buried half.

## The groove must contain the rib

The nut's internal thread is a **void**, and that void has to be a superset of the rib. If it
is not, a helical sliver survives, and no amount of extra clearance removes it:

| Attempt | What happens |
| --- | --- |
| Sweep a larger tooth profile | The flank angles differ, so a wedge accumulates along the helix |
| `offset(rib, t)` (3D solid offset) | Mathematically right; the offset surface splits the mesh into several shells |
| `offset(section, t)` (2D offset with fillets) | Fillets add faces, and the boolean then produces a bad solid |
| **Analytic 2D offset of the section** | Each edge moved along its outward normal, corners by line intersection: sharp corners kept, flank angle unchanged |

```python
import math


def offset_poly(pts, t):
    """Offset a convex polygon by t along its edge normals, keeping sharp corners."""
    n, lines = len(pts), []
    for i in range(n):
        (x0, y0), (x1, y1) = pts[i], pts[(i + 1) % n]
        dx, dy = x1 - x0, y1 - y0
        length = math.hypot(dx, dy)
        lines.append((x0 + t * dy / length, y0 - t * dx / length, dx, dy))
    out = []
    for i in range(n):
        x1, y1, dx1, dy1 = lines[i - 1]
        x2, y2, dx2, dy2 = lines[i]
        det = dx1 * dy2 - dy1 * dx2
        k = ((x2 - x1) * dy2 - (y2 - y1) * dx2) / det
        out.append((x1 + k * dx1, y1 + k * dy1))
    return out
```

- **The offset section's axial width must stay below the pitch.** At 0.32 growth the width
  goes from 1.56 to 2.47 on a 3.0 pitch; at 0.35 it reaches 3.0, the loft/sweep
  self-intersects, and OCCT returns a bad solid with no error.
- **Do not mix constructions between the rib and the groove.** Growing the section of a
  swept rib by 0.35–0.5 still left 1.7–3.0 mm³ of a 91 mm³ rib in build123d 0.13 — a
  different helix radius or a different section frame is enough to break the superset.
- **Verify, never assume:** `residual = rib - groove` must be zero to within a fraction of a
  cubic millimetre, in the same frame. That single subtraction catches every version of this
  mistake.

## Assembly order sets the diameters

The diameters follow from the order the parts are assembled, not from "the thread is the
same size as the tube":

- A nut that comes from the inside must pass over the whole socket before it reaches the
  thread, so **its minor diameter must clear the socket's outside diameter**.
- The thread crest must then be larger than the nut's minor diameter for there to be any
  engagement at all.
- In one build that forced the thread Ø83 against a Ø80.5 socket, and the 2.5 mm step became
  the hose's insertion stop. "Thread diameter = tube diameter" was impossible in that
  assembly order.

State the assembly order before choosing any diameter; if the numbers do not close, the
order is what has to change.

## Boolean order

Threaded parts hit the boolean kernel harder than smooth parts, and the failures are quiet:

- **Head and shank in one `revolve`** from a single XZ profile. Two separate solids joined
  afterwards leave a reversed shell in the shank's section.
- **The hex socket goes on the outer end of the head.** Sunk into the inside it is a closed
  cavity: a reversed shell with exactly the hex column's volume.
- **Cut the thread groove and the pilot hole in separate operations.** Unioning the cutters
  first leaves an isolated column inside the ear and a reversed shell.
- **Trim the sweep's overrun with a box**, not by shortening the helix: a helix that stops
  mid-pitch leaves a stub of thread past the end face.

## Phase and handedness

- **The rib and the groove must start an integer number of pitches apart.** A half-pitch
  offset is a different thread; a random offset collides on the flanks (0.2 mm of clearance
  absorbs only about 24° of phase error).
- **Test with coupled motion**: rotate by θ and advance `θ/360 × pitch` at the same time.
  Rotating without advancing always interferes, and that is not a defect.
- **Get the handedness right.** The same right-hand thread tested in the wrong direction
  reads 0.037 cm³ where the correct direction reads 0.00003.

## Judges — run all four every time

1. **Volume**: rib volume ≈ section area × helix length (use the section's **centroid
   radius** for the length). Treat ±15 % as normal; the check is for a factor of 2 (the
   section twisted) or a zero (the solid is degenerate).
2. **One full turn**: 24 phases of rotate-and-advance, interference ≈ 0, with a **control
   group** (move one part 200 mm away; it must be exactly 0).
3. **Mesh**: `len(part.solids()) == 1`, `len(part.shells()) == 1`, watertight, consistent
   normals, `min z == 0` after orienting for print.
4. **Render the exported STL.** This is the one that catches a missing thread: volume,
   interference and watertightness can all pass while the exported screw is a smooth rod.

## Sizes that print (0.4 mm nozzle, PLA)

| Parameter | Value | Why |
| --- | --- | --- |
| Thread | M6×3, or M5×2.5 where the wall is thin | M6 in a 10 mm ear leaves 1.5 mm per side; M5 leaves 2.0 |
| Clearance | 0.32 mm radial and axial | Turns by hand, not loose |
| Sections per turn | 12 | Chord error ≈ `r(1 − cos 15°)` ≈ 0.09 mm |
| Wall around an internal thread | ≥ 1.5 mm (M6), ≥ 2.0 mm (M5) | Below that the ear splits when tightened |
| Very small sections (e.g. 0.7 mm deep on a 2.0 pitch) | avoid | The face triangulates to `Poly_Triangulation is None`; verify before committing |

- **Print threads with their axis vertical.** Horizontal threads become overhangs on one
  flank and need support that ruins the fit.
- **Self-tapping screws are a fallback, not a design conclusion.** A thread that will not
  converge is a modelling problem; solve it before changing the fastener.

## Version notes

The source of this material recorded three build123d traps that **did not reproduce in
0.13**: a helix sweep twisting with a tangent-plane section, `BuildSketch` + `Polyline` faces
lofting into a shell, and `.part` assignments inside `with BuildPart()` being overwritten.
Keep the judges above rather than the construction bans: they are what tells you which
version you are on.
