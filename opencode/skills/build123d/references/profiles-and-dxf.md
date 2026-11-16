# 2D profiles and DXF

Read this for an outline, template, gasket, panel, cut layout, engraved marking, or a DXF
for a laser, waterjet, plasma or router.

## A drawing is geometry on a plane

build123d's sketch objects — `Sketch`, `Face`, `Wire`, `Edge` — are what a DXF holds. Build
the profile as algebra or in a builder, exactly like a part:

```python
import build123d as bd

WIDTH, HEIGHT, HOLE_D = 60.0, 40.0, 4.5

profile = bd.Rectangle(WIDTH, HEIGHT) - bd.Circle(HOLE_D / 2)
```

```python
with bd.BuildSketch() as sk:
    bd.Rectangle(WIDTH, HEIGHT)
    bd.Circle(HOLE_D / 2, mode=bd.Mode.SUBTRACT)
profile = sk.sketch
```

- **Keep the profile on `z=0`.** The exporters project onto XY and warn (`Exporting
  non-planar shape to 2D format; N points found outside the XY plane`) — a warning that
  you are exporting something other than the flat thing you meant. Move a face taken from
  a solid down first: `face.moved(bd.Location((0, 0, -z)))`.
- **Draw text as outlines**, never as a DXF `TEXT` entity: `bd.Text("REV B", font_size=6)`
  is geometry, which is what cut and marking toolchains consume.
- **Units are millimetres** by convention; state it and export with `unit=bd.Unit.MM`.

## Layers carry intent

`ExportDXF.add_shape(shape, layer=...)` writes each shape on its layer:

```python
profile = bd.Rectangle(WIDTH, HEIGHT) - bd.Circle(HOLE_D / 2)
mark = bd.Text("REV B", font_size=6)

exporter = bd.exporters.ExportDXF(unit=bd.Unit.MM, version="AC1027")
exporter.add_shape(profile, layer="CUT")
exporter.add_shape(mark, layer="ENGRAVE")
exporter.write("out/gasket.dxf")
```

- **One layer per CAM operation**, not per part: cut geometry on `CUT`, bend lines on
  `BEND`, engraving on `ENGRAVE`.
- **Name layers for the intent** — a shop's software classifies a layer by its name. A cut
  layer should be closed contours; open geometry belongs on a bend, engrave or reference
  layer.
- A layer name should avoid `<>/\":;?*|=` and backticks.
- **`version`** defaults to `AC1027` (AutoCAD 2013); older machinery may want an earlier
  one — say which you wrote.

## Kerf and tool radius

```python
profile = bd.Rectangle(WIDTH, HEIGHT) - bd.Circle(HOLE_D / 2)
compensated = bd.offset(profile, amount=0.15)   # + grows the outline, shrinks the holes
```

- **`bd.offset(sketch, amount=...)`** is the kerf (or tool-radius) compensation: positive
  grows the outer profile and shrinks each hole by the same amount, which is what cutting
  outside the line means. Negative reverses it.
- **Never hand-offset coordinates.** An offset at or beyond half the narrowest feature
  consumes the profile; let the kernel raise instead of producing a broken outline.
- **Curves stay curves.** The offset is an exact OCC operation, so a filleted corner
  exports as an `ARC` and a hole as a `CIRCLE`. A profile that comes out as
  hundreds of short `LINE`s fell back to a sampled path — fix the input, not the export.

## Deriving a profile from a 3D part

For a flat plate, the blank *is* one plane's faces:

```python
import build123d as bd

z = part.bounding_box().max.Z
faces = (
    part.faces()
    .filter_by(bd.Plane.XY)
    .filter_by_position(bd.Axis.Z, z - 1e-6, z + 1e-6)
)
blank = bd.Compound(children=[face.moved(bd.Location((0, 0, -z))) for face in faces])
```

- **Derive the contour from the real topology** rather than redrawing the dimensions: the
  face already carries the holes and the fillets.
- **A profile for a flat plate is one plane's faces.** For a bracket with flanges on
  several planes, move each plane's faces down and hand the exporter all of them — but do
  not call that an unfolded blank: a blank for bending needs the bend allowance, so a flat
  pattern from a bent part requires the K-factor and the bend deductions, not just the
  faces laid side by side.
- **A DXF holds a drawing, not a solid.** Export a sketch or planar face; exporting a solid
  only projects its edges and warns.

## Check a drawing

Read the file the shop gets, not the sketch in memory:

```python
import ezdxf

doc = ezdxf.readfile("out/gasket.dxf")
msp = doc.modelspace()
print(doc.header.get("$INSUNITS"))                      # 4 = millimetres
print(sorted({entity.dxf.layer for entity in msp}))
print([round(c.dxf.radius, 3) for c in msp.query("CIRCLE")])
print(len(msp.query('*[layer=="CUT"]')))
```

- **`$INSUNITS` is the drawing's unit** (`4` is millimetres). A file with `0` declares
  none, and a shop will guess.
- **Count entities by layer** — an exact duplicate is a double cut, and a zero-length
  segment is a stray point.
- **Every cut profile must close**, including chains of lines, arcs and splines. With
  `ezdxf`, `bbox.extents(msp)` (`from ezdxf import bbox`) gives the extents, and a
  `ezdxf.path.make_path` walk of each candidate wire tells you whether it closes.
- **What a check cannot see**: overlapping or partly coincident curves, features smaller
  than the tool, kerf and the shop's own rules. Report what ran, never that the file is
  cuttable.

## SVG instead of DXF

`bd.exporters.ExportSVG` writes the same shapes for a drawing meant for people
(documentation, a marking guide) rather than a cutting machine. Unlike `ExportDXF`, which
creates a layer on demand, `ExportSVG` needs the layer declared first:

```python
svg = bd.exporters.ExportSVG()
svg.add_layer("CUT", line_weight=0.35)
svg.add_shape(profile, layer="CUT")
svg.write("out/gasket.svg")
```
