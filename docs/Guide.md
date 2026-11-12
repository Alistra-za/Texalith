# Texalith guide

For texture authors using Texalith's three tabs. It covers what to open, what each setting changes, and what **Save** writes. In the app, the **(?)** beside a setting gives the same advice.

## Make maps from a normal or height map (Material tab)

1. Click **Open a normal or height map…**, or drop the file on the window. PNG, TGA, TIFF, BMP, JPEG, WebP, EXR, HDR, PSD and DDS are read.
2. Set **Output size** and **Edges**. Leave **Edges** on **Repeats** for a tiling texture. Choose **Does not repeat** only for a crop or single photo: the wrong choice on a tile leaves a hard seam.
3. Tick the maps you want. Each section has a thumbnail, its settings behind **Show … settings**, and its own **Save** button.
4. Check each map in the 2D or 3D view.
5. Pick a folder under **Save to** and click **Save N maps**.

A normal map source reports its green direction ("Green: DirectX (guessed)"). If the bumps look like dents, set **Source green** under **Height advanced**.

| Section | Saved as | What it is | Settings |
|---|---|---|---|
| Parallax | `<base>_parallax.png` | A display map for parallax shaders, made from Height data. Ticked by default. | **Depth**, **Match the normal**, **Flat level**, **Flatten broad shapes (shared with Height)**; **Parallax advanced**: **Smooth sharp steps**, **Invert** |
| Normal | `<base>_n.png` | A normal map made from the height. Shown for a height map source only. | **Normal depth**, **Green direction** |
| AO | `<base>_ao.png` | Soft shadow that gathers in crevices; white is open. Ticked by default. | **Reach**, **AO strength**, **Surface depth** |
| Curvature | `<base>_curvature.png` | Bright edges and dark creases on grey. | **Line thickness**, **Contrast** |
| Cavity | `<base>_cavity.png` | Dark only in narrow cracks; white is open. | **Crevice width**; **Cavity advanced**: **Surface depth (shared with AO)** |
| Height data | `<base>_height.png` | The true heights, lowest black and highest white; the source for normals and other tools. Unticked by default. | **Flatten broad shapes**, **Material**, **Compare**; **Height advanced**: **Source green** |

`<base>` is the source's name without its `_n`, `_normal`, `_nrm`, `_height`, `_disp` or `_displacement` suffix.

The settings people get wrong:

- **Flat level**: keep **Mid-grey** for ordinary parallax shaders, True PBR included. Choose **White** only when the target shader treats white as flat, such as a multilayer inner layer.
- **Flatten broad shapes**: stay near 0 for rock, ground and maps whose height truly matches the normal; move toward 1 for brick, bark, weave and carved parts. **Compare** shows 0, the current value and 1 side by side.
- **Depth**: raise it if the effect is hard to see, lower it if the surface warps or swims at low angles. "Only N of 256 grey levels are used; raise Depth." means the map is too shallow to hold its detail.
- **AO strength**: leave it at 1 if your renderer has its own AO strength setting.
- **Surface depth**: raise it if AO stays nearly white. A normal map implies only shallow relief, so this is common.

The AO is estimated from the normal, not baked from a mesh. If you have a real height map, open that.

## Blend normal layers (RNM tab)

1. Click **Open base…** and pick the base normal (`_n` or `_cnr`). Model-space normals (`_msn`) are refused. Layer files beside the base whose names end in `_bump`, `_detail`, `_detailnorm` or `_bent` (also with `2`, and with `mask` for masks) are added as layers.
2. Click **Add layer** for each further layer, and choose its file.
3. Set each layer's strength. Reorder with the drag handle or the arrows: layers blend from the top down, so each layer sits over the ones above it.
4. Pick a folder under **Save to** and click **Save**. The button's hover text shows the file name.

| Layer | File it takes | What it does | Its strength |
|---|---|---|---|
| Bump | a grey height image | Its slopes become bumps over the base. | **Strength (texels)**: how tall the bumps are |
| Detail | a tangent-space normal map | Its detail is laid over the base. | **Strength** |
| Bent | a bent normal map | Leans the base toward it. | **Strength**; also **Bend mostly in shadows**, **AO map**, **Focus on crevices** |

Every layer also has:

- **Flip this layer**: turn it on if the layer's bumps look like dents next to the base's.
- **Where it applies**: a mask image limits the layer to its white parts.

On a Bent layer, **Bend mostly in shadows** uses the **AO map** so the bent normal shows most where the surface is shadowed. Without an AO map the bend is even everywhere.

The base's alpha is kept as it is. The output takes the base's name; saved into the base's own folder it becomes `<name>1.png`, so the base stays.

## Make a cubemap (Cubemap tab)

1. Click **Open source…** and pick a panorama, cross, strip, mirror ball, angular map, photo, or one of six face files.
2. Check **Source type**. A wrong type scrambles the reflection; change it if the labelled faces in the 2D view look wrong or the image is square.
3. Set **Reflection softness** and **Brightness**. +1 stop of **Brightness** doubles the light.
4. Switch to **3D** to see the cube reflected on a ball. That ball is a vanilla environment-map material, not True PBR.
5. Pick a folder under **Save to** and click **Save**. The file is `<name>_e.png`, the six faces side by side.

**Advanced** holds the rest:

| Setting | Use it when |
|---|---|
| **Side size** | Leave it on **Auto**. Raise it only for a sharp mirror seen up close: a bigger cube costs memory and a soft reflection gains nothing. |
| **Turn**, **Mirror** | The sun or a bright window should sit elsewhere in the reflection, or writing reads backwards. |
| **Highlight rolloff**, **White point** | An HDR source clips the sun to a hard white patch. |
| **Colour strength**, **Tint** | A coloured sky tints every shiny surface, or reflections should be warmer or cooler. |
| **Ground**, **Horizon line** | The bottom of the source shows a tripod, a blank area or the wrong floor. |
| **Fix one side**, **Turn this face**, **Flip this face**, **Turn bottom cell** | One face of a cross, strip or face set is rotated or mirrored against its neighbours. |

A source larger than 8192 px on a side is halved until it fits.

## Check the result (the centre view and the Preview panel)

- **3D** and **2D**, above the centre view, switch it between the lit form and the flat map.
- **Form** picks **Sphere**, **Plane**, **Cube** or **Material ball**. Drag to orbit, scroll to zoom, double-click to reset, or click **Reset view**.
- **Parallax scale** changes how deep the parallax map looks in the preview only. It is not saved.
- **Community Shaders** names the shader build the preview draws with. Texalith ships that build and its settings, and never reads your own Community Shaders install: I want a map to look the same on every machine.
- **GPU** names the graphics adapter. If the graphics device is lost, Texalith restarts itself; maps that were not saved are gone.

## Saving

- Files are 16-bit PNGs. Each one records Texalith's version, the source's path and hash, and the settings, in a text chunk.
- A file Texalith made and nobody changed is replaced without asking.
- Any other file with the same name opens **Some files will be kept**. Choose **Keep** or **Overwrite** per file; **Keep a backup in _sdc\texalith-backup** copies an overwritten file into that folder, beside the output, first.
- The file you opened is always kept.

Next: encode the saved PNGs to DDS with [Tessalith](https://github.com/Alistra-za/Tessalith).
