# Third-party notices

Texalith (`texalith.exe`) is GPL-3.0-only (see `LICENSE`).

The licences of the Rust crates compiled into the executable, with their texts, are in `THIRD_PARTY_LICENSES.md` (generated). Regenerate with
`cargo about generate --frozen -c about.toml about.hbs -o THIRD_PARTY_LICENSES.md` (needs `cargo install cargo-about --locked --features cli`).
Where a crate offers a choice of licences, `about.toml` records the choice: MIT where offered, then Apache-2.0. No licence in the tree is incompatible with GPL-3.0.

## Shipped in the executable

- **Tessalith crates** (https://github.com/Alistra-za/Tessalith, the commit pinned in `Cargo.toml`): `render3d` and `nif` are GPL-3.0-or-later; `engine` and `encoders-sys` are MPL-2.0. Parts of Texalith's `src/from_normal` and `src/rnm` are ports of Tessalith code.
- **bc7e.ispc** (bc7enc_rdo commit b943862), Copyright (C) 2018-2021 Binomial LLC, Apache License 2.0 (`LICENSES/Apache-2.0-bc7e.txt`). Linked through `encoders-sys` as Tessalith's modified copy (its patch `0001-exact-opaque-alpha.patch`).
- **rgbcx** (bc7enc_rdo commit b943862), Richard Geldreich, Jr., MIT or Public Domain (Unlicense); used under MIT (`LICENSES/bc7enc_rdo-LICENSE.txt`). Linked through `encoders-sys` as Tessalith's modified `rgbcx.h`.
- **Intel ISPC Texture Compressor** (commit 79ddbc9), Copyright (c) 2016-2019 Intel Corporation, MIT (`LICENSES/ISPCTextureCompressor-license.txt`). Linked through `encoders-sys`.
- **Studio HDRI** `studio_small_09` (Poly Haven, https://polyhaven.com/a/studio_small_09), CC0 1.0 Universal per Tessalith's record. Embedded by `render3d` as the 3D preview's background and ambient light.
- **egui's default fonts** (`epaint_default_fonts`): Ubuntu-Light (Ubuntu Font Licence 1.0), NotoEmoji-Regular (SIL Open Font License 1.1), Hack and emoji-icon-font (MIT). The licence texts are in `THIRD_PARTY_LICENSES.md`.

## Shipped beside the executable

- **Community Shaders shader files** in `cs-shaders\shaders`, unmodified copies of the build named in `cs-shaders\PIN.toml`. GPL-3.0 with the Modding Exception and the GPL-3.0 Linking Exception; some files carry MIT or Apache-2.0 code. The licence, the exceptions and the per-file table are in `cs-shaders\licenses` (`COPYING.txt`, `EXCEPTIONS.md`, `THIRD_PARTY.md`).
- The emblem (`docs/assets/texalith-emblem.png`) and the icon (`assets/texalith.ico`, `assets/texalith-256.png`) are Texalith's own, made from Tessalith's emblem by the project author.
