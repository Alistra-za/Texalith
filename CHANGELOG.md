# Changelog

## 0.1.1

### Sliders and preview

- A slider held with the mouse recalculates once it has rested for 0.2 s, or when it is let go, and no longer on every frame of the drag. **Update delay** in the **Preview** panel sets that time, from 0 to 1 s.
- The preview's maps are packed on the generation threads, no longer on the window's thread. A result for a value the slider has already left is dropped.
- The 3D preview goes back to the plain form when you open another file. It kept the last file's maps until the first result, and for good if that result failed.
- A missing `cs-shaders` folder is named in the preview's message, with what to do: unzip the release again.

### Saving

- **Save** runs in the background on all three tabs. The window stays usable; **Save** is off and "Saving…" shows until the files are written.
- PNGs are written about 10 times faster: a detailed 8192 normal map takes about 2 s, was about 19 s. Files come out 2 to 50 % larger. A mostly flat map can come out 3 times larger, but such a file is small (6 MB, was 2 MB).
- A file open in any tab is kept, also when you save from another tab. The dialog says "This file is open in Texalith, so it is kept". On the **RNM** and **Cubemap** tabs the output takes the next free numbered name instead.
- The check for files Texalith made reads a PNG's text chunk and no longer decodes its pixels.
- The line under **Save** is always one row, so **Save** stays where it is. A line too long for the column is cut; hover it to read all of it.

### Material tab

- **Match the normal** sets the exact depth. The slider rounded it to a whole percent, up to a third too shallow.
- **AO strength** is applied to the AO already made and no longer runs the AO again.
- A drag of **Reach** or **Surface depth** above 1024 keeps the full-size height, so letting go no longer solves it again.
- A float source with a NaN or infinite pixel reads that pixel as 0. One NaN made a grey height read as a normal map.

### RNM tab

- The base can be up to 8192 on a side (was 4096). The 3D preview and the 2D view show an 8192 blend at 4096.
- A base, layer, mask or AO map over 8192 on a side is refused from its header, before the file is read.
- A file picked for a slot wins over an older pick that is still loading, and a mask cleared while it loads stays cleared.
- A mask or AO map keeps one channel in memory: about 270 MB at 8192, was about 1 GB.
- The base is decoded off the window's thread.

### Cubemap tab

- A float source with an infinite pixel (a clipped sun) no longer saves a black patch with **Highlight rolloff** on. Negative light reads as 0.
- A panorama, angular map or photo far larger than the cube is averaged down to it first. A small sun was hit or missed by luck, and a large photo needed about 2 GB.
- A picture whose name ends like a face file (`times_square_ny.hdr`) opens as one picture when the other five face files are not beside it, and saves as `times_square_ny_e.png`.

### Other

- When Texalith closes after a second graphics failure, or cannot open its window, a message box says why.

## 0.1.0

- **Material** tab: from a normal map, makes Parallax, AO, Curvature, Cavity and Height data maps; from a height map, also a Normal.
- **RNM** tab: blends any number of Bump, Detail and Bent layers into a base normal, each with its own strength, mask and green flip.
- **Cubemap** tab: makes a 6:1 `_e.png` strip from a panorama, cross, strip, mirror ball, angular map, photo or six face files.
- 3D preview drawn with a pinned copy of Community Shaders' shader files, on a sphere, plane, cube or material ball.
- Outputs are 16-bit PNGs that record their source and settings. A source file is never written over.
