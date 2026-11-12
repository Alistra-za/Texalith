# Changelog

## 0.1.1 (Unreleased)

- A slider held with the mouse recalculates once it has rested for 0.2 s, or when it is let go, and no longer on every frame of the drag. **Update delay** in the **Preview** panel sets that time, from 0 to 1 s.
- The preview's maps are packed on the generation threads, no longer on the window's thread. A result for a value the slider has already left is dropped.
- **RNM** tab: the base can be up to 8192 on a side (was 4096). The 3D preview shows an 8192 blend at 4096.

## 0.1.0

- **Material** tab: from a normal map, makes Parallax, AO, Curvature, Cavity and Height data maps; from a height map, also a Normal.
- **RNM** tab: blends any number of Bump, Detail and Bent layers into a base normal, each with its own strength, mask and green flip.
- **Cubemap** tab: makes a 6:1 `_e.png` strip from a panorama, cross, strip, mirror ball, angular map, photo or six face files.
- 3D preview drawn with a pinned copy of Community Shaders' shader files, on a sphere, plane, cube or material ball.
- Outputs are 16-bit PNGs that record their source and settings. A source file is never written over.
