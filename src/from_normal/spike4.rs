//! Spike 4: the Nyquist fix on real sets, the normal → height → normal round trip, and the AO dot trace. Results:
//! `research_notes/Texalith generation modes and settings/spike4_nyquist/README.md`. Run the measurement with
//! `SPIKE4_OUT=<folder> cargo test --lib spike4 -- --ignored --nocapture`.
//! shortcut: research code behind cfg(test); `symbol_slopes` moves into the solve once a derived normal needs it.
use super::ao::{march, march_with, BIAS};
use super::fft::{fft2, C};
use super::solve::{curvature_with, solve_with, symbol};
use super::{AoOpts, Slopes};
use std::f64::consts::PI;
use std::sync::atomic::AtomicBool;

/// `(x, y)` → index, wrapped or edge-repeating.
fn at(x: isize, y: isize, w: usize, h: usize, wrap: bool) -> usize {
    let f = |i: isize, n: usize| match wrap {
        true => i.rem_euclid(n as isize) as usize,
        false => i.clamp(0, n as isize - 1) as usize,
    };
    f(y, h) * w + f(x, w)
}

/// Slopes of `h` by the solve's own symbol, `IFFT(j·d·Ĥ)` per axis, on the solve's grid (the half-sample-even
/// 2W × 2H extension when mirrored), cropped.
fn symbol_slopes(h: &[f64], w: usize, hgt: usize, wrap: bool) -> (Vec<f64>, Vec<f64>) {
    let (n, m) = if wrap { (w, hgt) } else { (2 * w, 2 * hgt) };
    let fold = |i: usize, len: usize| if i < len { i } else { 2 * len - 1 - i };
    let mut f: Vec<C> = (0..n * m)
        .map(|i| C::new(h[fold(i / n, hgt) * w + fold(i % n, w)], 0.0))
        .collect();
    fft2(&mut f, n, m, false);
    let deriv = |along_x: bool| -> Vec<f64> {
        let mut g: Vec<C> = f
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let d = if along_x { symbol(i % n, n) } else { symbol(i / n, m) };
                C::new(-d * c.im, d * c.re) // j·d·c
            })
            .collect();
        fft2(&mut g, n, m, true);
        let mut out = Vec::with_capacity(w * hgt);
        for y in 0..hgt {
            out.extend(g[y * n..y * n + w].iter().map(|c| c.re));
        }
        out
    };
    (deriv(true), deriv(false))
}

/// Scharr 3-10-3 slopes over 32 (unit slope on a ramp), wrapped or edge-repeating.
fn scharr(h: &[f64], w: usize, hgt: usize, wrap: bool) -> (Vec<f64>, Vec<f64>) {
    let v = |x: isize, y: isize| h[at(x, y, w, hgt, wrap)];
    let (mut p, mut q) = (vec![0.0; w * hgt], vec![0.0; w * hgt]);
    for y in 0..hgt as isize {
        for x in 0..w as isize {
            let i = y as usize * w + x as usize;
            p[i] = (3.0 * (v(x + 1, y - 1) - v(x - 1, y - 1))
                + 10.0 * (v(x + 1, y) - v(x - 1, y))
                + 3.0 * (v(x + 1, y + 1) - v(x - 1, y + 1)))
                / 32.0;
            q[i] = (3.0 * (v(x - 1, y + 1) - v(x - 1, y - 1))
                + 10.0 * (v(x, y + 1) - v(x, y - 1))
                + 3.0 * (v(x + 1, y + 1) - v(x + 1, y - 1)))
                / 32.0;
        }
    }
    (p, q)
}

/// Mean and p95 angle, degrees, between `unit(−p, −q, 1)` of `s` and of `(p, q)`.
fn angles(s: &Slopes, (p, q): &(Vec<f64>, Vec<f64>)) -> (f64, f64) {
    let unit = |a: f64, b: f64| {
        let l = (a * a + b * b + 1.0).sqrt();
        [-a / l, -b / l, 1.0 / l]
    };
    let mut ang: Vec<f64> = (0..p.len())
        .map(|i| {
            let (a, b) = (unit(s.p[i], s.q[i]), unit(p[i], q[i]));
            let cross = [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ];
            let dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
            cross[0].hypot(cross[1]).hypot(cross[2]).atan2(dot).to_degrees()
        })
        .collect();
    let mean = ang.iter().sum::<f64>() / ang.len() as f64;
    let i = ((ang.len() - 1) as f64 * 0.95).round() as usize;
    (mean, *ang.select_nth_unstable_by(i, f64::total_cmp).1)
}

/// Period-2 line height over its neighbours, along x and along y: the mean power of the Nyquist bin over the rows
/// (columns), over the median of the 15 bins below it. Python's `nyq_line` high-passed first with a σ 8 Gaussian,
/// whose gain is 1 to within 1e-9 at those bins.
fn line(v: &[f64], w: usize, h: usize) -> (f64, f64) {
    let ratio = |lines: usize, len: usize, get: &dyn Fn(usize, usize) -> f64| {
        let mut power = vec![0.0; len / 2 + 1];
        for j in 0..lines {
            let mut c: Vec<C> = (0..len).map(|i| C::new(get(j, i), 0.0)).collect();
            fft2(&mut c, len, 1, false);
            for (k, pw) in power.iter_mut().enumerate() {
                *pw += c[k].re * c[k].re + c[k].im * c[k].im;
            }
        }
        let mut below = power[len / 2 - 15..len / 2].to_vec();
        below.sort_by(f64::total_cmp);
        power[len / 2] / below[7]
    };
    (
        ratio(h, w, &|y, x| v[y * w + x]),
        ratio(w, h, &|x, y| v[y * w + x]),
    )
}

/// Share of texels more than 0.05 from the median of their wrapped 3 × 3 neighbourhood.
fn dots(ao: &[f32], w: usize, h: usize) -> f64 {
    let mut n = 0usize;
    for y in 0..h as isize {
        for x in 0..w as isize {
            let mut nb: Vec<f32> = (-1..=1)
                .flat_map(|dy| (-1..=1).map(move |dx| (dx, dy)))
                .map(|(dx, dy)| ao[at(x + dx, y + dy, w, h, true)])
                .collect();
            nb.sort_by(f32::total_cmp);
            n += usize::from((ao[y as usize * w + x as usize] - nb[4]).abs() > 0.05);
        }
    }
    n as f64 / (w * h) as f64
}

#[test]
fn the_solves_own_symbol_round_trips_the_integrable_part() {
    // Heights with no content on the Nyquist lines come back exactly; there the fixed solve attenuates by
    // dy²/(4 + dy²) (pinned by `an_x_nyquist_line_in_q_is_not_amplified`).
    let (w, h) = (64, 32);
    for wrap in [true, false] {
        let hf: Vec<f64> = (0..w * h)
            .map(|i| {
                let (x, y) = ((i % w) as f64, (i / w) as f64);
                // Wrap: periodic below Nyquist. Mirror: cosines even about both edges, below the 2W grid's Nyquist.
                let (u, v) = match wrap {
                    true => (2.0 * PI * x / w as f64, 2.0 * PI * y / h as f64),
                    false => (PI * (x + 0.5) / w as f64, PI * (y + 0.5) / h as f64),
                };
                (3.0 * u).cos() * (5.0 * v + 0.3).cos() + 0.5 * (11.0 * u + 1.0).sin() + 0.2 * (15.0 * v).cos()
            })
            .collect();
        let (p, q) = symbol_slopes(&hf, w, h, wrap);
        let s = Slopes {
            width: w as u32,
            height: h as u32,
            p: p.clone(),
            q: q.clone(),
            tilt: (0.0, 0.0),
        };
        let (p2, q2) = symbol_slopes(&solve_with(&s, wrap, 0.0, true), w, h, wrap);
        let d = p
            .iter()
            .zip(&p2)
            .chain(q.iter().zip(&q2))
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f64::max);
        assert!(d < 1e-9, "wrap {wrap}: slopes moved by {d}");
    }
}

#[test]
fn scharr_keeps_cos_half_omega_of_the_symbols_slope() {
    // h = cos(ωx): the symbol gives −2·sin(ω/2)·sin(ωx), Scharr −sin(ω)·sin(ωx); the ratio is cos(ω/2), .71 at
    // wavelength 4.
    let (w, h) = (64, 8);
    for period in [4.0, 8.0, 16.0] {
        let om = 2.0 * PI / period;
        let hf: Vec<f64> = (0..w * h).map(|i| (om * (i % w) as f64).cos()).collect();
        let proj = |p: &[f64]| -> f64 {
            p.iter()
                .enumerate()
                .map(|(i, v)| v * (om * (i % w) as f64).sin())
                .sum()
        };
        let ratio = proj(&scharr(&hf, w, h, true).0) / proj(&symbol_slopes(&hf, w, h, true).0);
        assert!(
            (ratio - (om / 2.0).cos()).abs() < 1e-9,
            "period {period}: {ratio} vs {}",
            (om / 2.0).cos()
        );
    }
}

const ACG: &str = r"F:\ai\dds\bench\tools\dl_acg3";
/// Name, tiles (the bake-off's `wrap` column), and the height scale k fitted to the normal (`datasets.csv`).
const SETS: [(&str, bool, f64); 3] = [
    ("Bark012", true, 18.165189160317798),
    ("Moss001", true, 10.721526280835766),
    ("MetalPlates006", false, 17.653331963444412),
];

/// A 160-texel crop around (512, 512), wrapped, at 2x, of each image side by side with 8 px gaps.
fn montage(path: &std::path::Path, tiles: &[&[f32]], w: usize, h: usize) {
    let (size, zoom, gap) = (160usize, 2usize, 8usize);
    let tw = size * zoom;
    let width = tiles.len() * tw + (tiles.len() - 1) * gap;
    let mut img = vec![40u8; width * tw];
    for (t, v) in tiles.iter().enumerate() {
        for y in 0..tw {
            for x in 0..tw {
                let (sx, sy) = ((512 - size / 2 + x / zoom) % w, (512 - size / 2 + y / zoom) % h);
                img[y * width + t * (tw + gap) + x] = (v[sy * w + sx].clamp(0.0, 1.0) * 255.0).round() as u8;
            }
        }
    }
    image::save_buffer(path, &img, width as u32, tw as u32, image::ColorType::L8).unwrap();
}

#[test]
#[ignore = "spike 4 measurement: reads ambientCG sets under F:\\ai\\dds\\bench, writes to SPIKE4_OUT"]
fn measure() {
    let out = std::path::PathBuf::from(std::env::var("SPIKE4_OUT").expect("set SPIKE4_OUT"));
    std::fs::create_dir_all(&out).unwrap();
    let mut log = String::new();
    let mut say = |s: String| {
        println!("{s}");
        log.push_str(&s);
        log.push('\n');
    };
    for (name, wrap, k) in SETS {
        let base = format!(r"{ACG}\{name}\{name}_1K-PNG_");
        let n = image::open(format!("{base}NormalDX.png")).unwrap().to_rgba32f();
        let (w, h) = (n.width() as usize, n.height() as usize);
        let px: Vec<[f32; 4]> = n.pixels().map(|p| p.0).collect();
        let s = super::slopes(&px, w as u32, h as u32, false);
        let disp = image::open(format!("{base}Displacement.png")).unwrap().to_luma32f();
        assert_eq!((disp.width() as usize, disp.height() as usize), (w, h));
        let mut truth: Vec<f64> = disp.pixels().map(|p| f64::from(p.0[0]) * k).collect();
        let mean = truth.iter().sum::<f64>() / truth.len() as f64;
        truth.iter_mut().for_each(|v| *v -= mean);
        say(format!("\n== {name} {w}x{h} wrap {wrap}"));
        let (px_, py_) = (line(&s.p, w, h), line(&s.q, w, h));
        say(format!(
            "input slopes line: p {:.2} / {:.2}, q {:.2} / {:.2} (x / y)",
            px_.0, px_.1, py_.0, py_.1
        ));
        let tl = line(&truth, w, h);
        say(format!("true height line: {:.2} / {:.2}", tl.0, tl.1));
        for relief in [0.0, 0.3] {
            let mut curv = Vec::new();
            for fixed in [false, true] {
                let hs = solve_with(&s, wrap, relief, fixed);
                let l = line(&hs, w, h);
                let sym = angles(&s, &symbol_slopes(&hs, w, h, wrap));
                let sch = angles(&s, &scharr(&hs, w, h, wrap));
                say(format!(
                    "relief {relief} {}: height line {:.2} / {:.2}; normal from height, mean / p95 deg: symbol {:.2} / {:.2}, Scharr {:.2} / {:.2}",
                    if fixed { "fixed  " } else { "unfixed" },
                    l.0, l.1, sym.0, sym.1, sch.0, sch.1
                ));
                curv.push(curvature_with(&s, wrap, relief, fixed));
            }
            let d = curv[0].iter().zip(&curv[1]).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
            say(format!("relief {relief}: curvature max |fixed - unfixed| {d:.2e}"));
        }
        let o = AoOpts::default();
        let (h0, h1) = (solve_with(&s, wrap, 0.3, false), solve_with(&s, wrap, 0.3, true));
        let no = AtomicBool::new(false);
        let (wu, hu) = (w as u32, h as u32);
        let variants: Vec<(&str, Vec<f32>)> = vec![
            ("A0 Tessalith's march (the normal's slopes), unfixed solve", march_with(&h0, &s, wrap, &o, BIAS, 0.0)),
            ("A1 Tessalith's march, fixed solve", march_with(&h1, &s, wrap, &o, BIAS, 0.0)),
            ("B fixed solve, its own central slopes (Texalith's march, Python form)", march(&h1, wu, hu, wrap, &o, &no).unwrap()),
            ("C true height, its own central slopes", march(&truth, wu, hu, wrap, &o, &no).unwrap()),
            ("D true height, the normal's slopes", march_with(&truth, &s, wrap, &o, BIAS, 0.0)),
            ("E A1 with radii at least 1 texel", march_with(&h1, &s, wrap, &o, BIAS, 1.0)),
            ("F A1 with bias 0", march_with(&h1, &s, wrap, &o, 0.0, 0.0)),
        ];
        // C, the true height with its own slopes, is the bake-off's AO reference.
        let reference = &variants[3].1;
        for (tag, ao) in &variants {
            let v: Vec<f64> = ao.iter().map(|&a| f64::from(a)).collect();
            let l = line(&v, w, h);
            let m = v.iter().sum::<f64>() / v.len() as f64;
            let mae = ao.iter().zip(reference).map(|(a, b)| f64::from((a - b).abs())).sum::<f64>() / v.len() as f64;
            say(format!(
                "AO {tag}: dots {:.4}, line {:.2} / {:.2}, mean {m:.3}, MAE vs C {mae:.4}",
                dots(ao, w, h),
                l.0,
                l.1
            ));
        }
        let tiles: Vec<&[f32]> = variants.iter().map(|(_, v)| v.as_slice()).collect();
        montage(&out.join(format!("{name}_ao_A0-F.png")), &tiles, w, h);
    }
    std::fs::write(out.join("results.txt"), log).unwrap();
}
