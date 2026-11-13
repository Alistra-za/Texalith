//! Area and bilinear resampling. Copied from Tessalith at 36384560 (GPL-3.0): engine's `area_resample`
//! (`crates/engine/src/mips.rs`) is crate-private, and `rnm::blend::resample` goes in Tessalith 3.0.
use engine::image::Rgba32F;
use std::borrow::Cow;

/// Per destination texel, the source texels it covers and their area shares (`n` → `m` along one axis); the shares
/// of one destination texel sum to 1.
fn axis_weights(n: u32, m: u32) -> Vec<Vec<(u32, f64)>> {
    let (n64, m64) = (u64::from(n), u64::from(m));
    (0..m64)
        .map(|i| {
            // Work in units of 1/m texel: destination covers [i*n, (i+1)*n), source j covers [j*m, (j+1)*m).
            let (lo, hi) = (i * n64, (i + 1) * n64);
            let first = lo / m64;
            let last = hi.div_ceil(m64).min(n64);
            (first..last)
                .map(|j| {
                    let overlap = hi.min((j + 1) * m64) - lo.max(j * m64);
                    (j as u32, overlap as f64 / n64 as f64)
                })
                .collect()
        })
        .collect()
}

/// Area-exact box resample of `prev` to `w`×`h`, accumulated in f64 (Tessalith's unweighted path).
fn area_resample(prev: &Rgba32F, (w, h): (u32, u32)) -> Rgba32F {
    let (wx, wy) = (axis_weights(prev.width, w), axis_weights(prev.height, h));
    let mut out = Rgba32F::new(w, h, [0.0; 4]);
    for (y, ys) in wy.iter().enumerate() {
        for (x, xs) in wx.iter().enumerate() {
            let mut sum = [0.0f64; 4];
            for &(cy, vy) in ys {
                for &(cx, vx) in xs {
                    let k = prev.at(cx, cy);
                    let area = vx * vy;
                    for c in 0..4 {
                        sum[c] += area * f64::from(k[c]);
                    }
                }
            }
            *out.at_mut(x as u32, y as u32) = sum.map(|v| v as f32);
        }
    }
    out
}

/// `img` at `size`: borrowed as it is when equal, the area filter when no axis grows, else bilinear (texel centres,
/// clamped edges).
pub fn resample(img: &Rgba32F, size: (u32, u32)) -> Cow<'_, Rgba32F> {
    if (img.width, img.height) == size {
        return Cow::Borrowed(img);
    }
    if img.width >= size.0 && img.height >= size.1 {
        return Cow::Owned(area_resample(img, size));
    }
    let (sx, sy) = (img.width as f32 / size.0 as f32, img.height as f32 / size.1 as f32);
    let axis = |o: u32, s: f32, n: u32| {
        let f = ((o as f32 + 0.5) * s - 0.5).clamp(0.0, (n - 1) as f32);
        let i = f.floor() as u32;
        (i, (i + 1).min(n - 1), f - i as f32)
    };
    let mut out = Rgba32F::new(size.0, size.1, [0.0; 4]);
    for y in 0..size.1 {
        let (y0, y1, ty) = axis(y, sy, img.height);
        for x in 0..size.0 {
            let (x0, x1, tx) = axis(x, sx, img.width);
            let (a, b, c, d) = (img.at(x0, y0), img.at(x1, y0), img.at(x0, y1), img.at(x1, y1));
            let px = out.at_mut(x, y);
            for i in 0..4 {
                let top = a[i] + (b[i] - a[i]) * tx;
                let bot = c[i] + (d[i] - c[i]) * tx;
                px[i] = top + (bot - top) * ty;
            }
        }
    }
    Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_box_down_keeps_the_mean() {
        let img = Rgba32F { width: 4, height: 4, px: (0..16).map(|i| [i as f32; 4]).collect() };
        let r = resample(&img, (2, 2));
        assert_eq!(r.px.iter().map(|p| p[0]).collect::<Vec<_>>(), vec![2.5, 4.5, 10.5, 12.5]);
    }
}
