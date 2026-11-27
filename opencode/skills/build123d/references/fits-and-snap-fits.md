# Fits, snap-fits and printed joints

Read this when two printed parts have to meet: a bayonet, a plug, a clamp, a lid, or anything
with a deliberate clearance.

Fit is a number, not a feeling. Every rule here is about choosing the number from the
assembly, then proving it with a measurement.

## Clearance, first guess

| Interface | Gap | Note |
| --- | --- | --- |
| Sliding / twisting fit (bayonet, plug) | 0.4 mm | Below ~0.3 mm, elephant's foot and stringing close it |
| Threaded pair | 0.32 mm radial and axial | Applied to the section, not the diameter ([threads](threads-and-fasteners.md)) |
| Snap hook engagement | 0.9 mm | Less than ~0.6 mm vibrates loose |
| Press / friction fit | 0.0–0.1 mm | Only on a dimension the printer controls well (a vertical wall) |

Print the pair once and adjust from that number; do not tune clearance in the slicer.

## Assembly order decides the geometry

Before drawing anything, write the assembly as a sequence and check every passage:

1. What goes in first, and over what does it have to pass?
2. Which diameter is the largest thing that must clear which hole?
3. Where does the motion stop — and can that stop be a feature instead of a separate part?

A nut that must pass over a socket sets the socket's maximum diameter and the thread's
minimum diameter at the same time. In one build that forced the thread to be 2.5 mm larger
than the tube, and the resulting step became the hose's insertion stop. The constraint was
never "the thread equals the tube" — that was an assumption.

### Sliding guides with an end latch

Sketch four poses: entry, first contact, tooth crossing the crest, and seated. Mark
which surfaces guide, which part can flex, and what stops the motion. If a tooth is
only needed at the end but needs a relief groove along most of the rail, try moving
it to the side wall near the closed end. A local recess in the slider can then catch
it without sacrificing the long bearing surface. Leave enough compliance there:
a bump on a rigid end wall facing the insertion direction is a stop, not a latch.

Derive both guides from one cross-section. Dimension **running clearance**, **interference
at the crest**, and **clearance in the seated recess** separately: widening the whole
guide to soften the click also increases play everywhere else. For a hidden pair,
inspect both rails with the slider removed. Section each tooth and sight along the
specified viewing direction to check that the retaining lip actually covers its tip.

## Bayonets only work on round ports

A tang travels a circle around the axis; the slot it follows must be cut into a surface that
stays a circle over that arc. Two consequences:

- **A flat or oblong port cannot rotate a bayonet at all.** The oblong tube interlocks with
  the oblong hole; the tang has nowhere to go.
- **On a round port, the wall limits the rotation.** The tang sweeps outward past the wall at
  a radius that grows as it turns. Computing it: at a tang at the port's outer radius, a 2 mm
  wall allows about 11° before the tang breaks through, 2.5 mm about 12°, 4 mm about 15°.
  A 90° quarter-turn needs a local boss or a much thicker wall.

When the port is oblong, use **axial insertion plus elastic tabs**, not a bayonet.

## Snap tabs

- **Asymmetric ramps, by design.** A shallow front ramp inserts easily and a steep rear face
  resists pull-out: 42° front and 61° rear gave roughly 9 N to insert and 19 N to remove on a
  7 mm wide tab. Make the ratio deliberate; a symmetric tab is easy to insert and easy to
  lose.
- **Cut the tab out of the wall**, with a slit width the printer can resolve (0.5 mm at a
  0.4 mm nozzle), rather than adding a separate spring.
- **The hook's engagement depth is the retention**, so measure it against the mating face,
  not against the nominal overlap.
- **A tab needs room to bend.** Its free length and the gap behind it decide how far the hook
  can travel; a 0.9 mm hook in a 0.5 mm gap binds.

## Prove the interface on a coupon

Before printing the real part, build a **joint coupon**: the mating 10 mm of each side, in
the print orientation, on one plate.

- It costs grams and 20 minutes, and it answers the only two questions that matter: does it
  go together, and does it stay.
- Keep the coupon's geometry driven by the same constants as the real parts, and regenerate
  it when the interface changes.
- The coupon is also where you find out that the part needs support, that a wall is too thin,
  or that the snap needs a bigger gap.

## Measure the insertion depth, don't assume it

"Push the plug in 10 mm" fails when the cavity bends. Measure it:

```python
# Intersect with the housing material, not the empty space inside the port.
for depth in (0.0, 0.1, 1.0, 3.0, 4.0, 5.0):
    moved = plug.moved(bd.Location((0, 0, -depth)))
    clash = moved & housing
    print(depth, clash.volume if clash else 0.0)
```

One such sweep showed a plug that was clear for 5 mm in a straight duct was already touching
at 4 mm inside a 90° bend, because the inner radius of the bend was only 5.3 mm. The number
that matters is the one measured against the real cavity.

## Check the joint the way it will be used

| Question | Measurement |
| --- | --- |
| Does it turn / slide freely? | Sample the full travel from entry to seat; choose a step smaller than the shortest contact feature and refine around transitions. Record where contact begins, peaks and ends, and on which side. |
| Does it come apart? | Interference after pulling out the nominal clearance, then after the hook's engagement |
| Does it stop where it should? | Interference just past the end stop — it must be non-zero |
| Does it droop? | Tilt one side by 0.5° and measure the contact; a face that is really seated shows a small, finite clash, not silence |
| Does it rattle? | The gap at the mating face under a 0.5° tilt, and the preload (a gasket, a ramp) |

- **A joint that is clear everywhere has no stop.** The end-stop clash is the feature.
- **A joint that clears at every pose has no preload** either — something must press.
- Rigid overlap locates nominal interference; it does not measure insertion force,
  retention or wear. Test those on a print with the intended material, orientation
  and flexing length.

## Clamps and seams

An open clamp's seam width *is* its take-up:

```text
closing the seam by w reduces the diameter by  Δd ≈ w / π
```

- A 6 mm seam closes a Ø86.6 clamp to about Ø84.7, so it grips a hose between Ø84.7 and
  Ø86.6.
- **Measure the hose's circumference, not its nominal diameter.** A hose sold as 80 mm
  measured a circumference that implied a 17.5 mm larger diameter; the clamp was sized from
  the measurement.
- **Put the bolt through the same thickness of material as the band.** An ear that is a
  separate boss glued onto the band has a stress concentration at the joint; an ear that is
  part of the same extruded profile has none. Round the junction with 2D fillets in the
  profile — a 3D tangent-chain fillet fails there ([patterns](modeling-patterns.md#put-fillets-where-the-kernel-can-make-them)).
