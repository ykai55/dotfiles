# Printability: designing for FDM from the first line

Read this when the part is going to be printed rather than machined: orienting it, choosing
walls and clearances, or working out why a feature slices into spaghetti.

A printed part is a stack of 2D layers. Every design rule below is a consequence of that one
fact.

## Orientation comes first

- **Decide the build face before the geometry.** One flat face on the bed, `min z == 0`
  after moving the part — not after moving the bed. Rotate the part so its largest flat face
  (or the face that must stay dimensionally exact) is at `z = 0`.
- **Write the orientation into the script**, as a final `rotate`/`translate` before export, so
  every regeneration prints the same way.
- **One flat face, not a curved one.** A part that only touches the bed at a point or a ring
  needs a brim at best and will warp at worst.

## Overhangs: the 45° rule and its arithmetic

A surface that moves outward as it rises is supported only by the layer below. The
unsupported step per layer is:

```text
step = layer_height / tan(angle between the surface and the bed)
```

At 45° that is one layer height (0.2 mm), which prints fine. At 3.3° it is 3.4 mm per
layer — a shelf hanging in the air, and the slicer will draw loose strands. That exact
mistake — a 0.6 mm high locating cone on the bottom face — turned the whole first layer into
spaghetti.

- **Keep every upward-shrinking feature at ≥45° from the bed**: chamfers, countersinks, hole
  entries, drip grooves.
- **Chamfer, don't fillet, at the bed face.** A fillet curls away from the bed over several
  layers; a 45° chamfer is self-supporting and doubles as a lead-in.
- **A horizontal ceiling is a bridge, not an overhang.** Holes along Z print round; holes
  across Z need a teardrop or support.

## Threads and vertical features

- **Print threads with the screw axis vertical.** A horizontal thread has one flank facing
  down for its whole length.
- **A vertical wall can be zero-draft**; a taper is only needed for moulding, not printing.
- **A blind pocket in a wall** costs the wall's thickness: a 1.0 mm deep pocket in a 1.8 mm
  wall leaves 0.8 mm, which is two 0.4 mm lines. At that point the pocket is the wall.

## Walls and infill: solid volume is not print weight

Changing a 5–9 mm thick ring from one wall to solid changed the model's volume by +28 % but
the printed weight by only +12 %: the interior is 15 % infill, and only the surfaces are
solid. Two consequences:

- **Do not estimate print time or filament from model volume.** Slice it.
- **Do not "strengthen" a part by thickening it** until you know where the load path is:
  two extra perimeters beat 40 % infill for the same mass.

## Material

- **PLA is fine for static parts** — most brackets stay under 20 MPa.
- **Use PETG when the part is repeatedly assembled or lives in the sun** (a window adapter,
  a clamp that is tightened often). Same STL, no geometry change.
- **Do not design a polymer thread to a metal torque spec.** The limiting factor is the ear
  or boss around the thread, not the thread.

## Readiness checklist

Run this on every part before it goes into a project file:

| Check | Why |
| --- | --- |
| `len(part.solids()) == 1` | Two solids can pass a volume check and export as two bodies |
| `len(part.shells()) == 1` | A reversed or nested shell appears as a separate shell |
| Watertight STL, consistent normals | The slicer repairs silently and changes the shape |
| `bounding_box().min.Z == 0` | The part floats or sinks on the plate otherwise |
| Slice it once with the real profile | Only the slicer knows the weight, the time, and whether it needs support |
| Look at the sliced preview | The one failure the numbers cannot see |

- **Zero support is a design goal, not a slicing setting.** If the slicer wants support,
  change the chamfer or the orientation.
- **Keep a print-in-place or sliding interface's clearance in the model**, not in the
  slicer's "horizontal expansion": see [fits and snap-fits](fits-and-snap-fits.md).

## One part per plate is a luxury

- For a family of parts, plate them by `ceil(sqrt(n))` columns on the slicer's plate pitch
  (Bambu: 307.2 mm). Five objects need three columns; two rows of two plus one centred
  collides, and the slicer reports `plate N: Nothing to be sliced`.
- Put the small trial pieces on their own plates so a failed fit costs grams, not a
  half-day print ([fits and snap-fits](fits-and-snap-fits.md#prove-the-interface-on-a-coupon)).
