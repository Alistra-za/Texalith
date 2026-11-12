//! Spike 6: times Texalith's from_normal steps at one size and edge mode: slopes, the solve, curvature, the AO march
//! at 8 and 16 slices, and the cavity march.
//! Run with: cargo run --release --example from_normal_timing -- <size> <wrap|mirror>
//! Prints one line per step with the first and the median of 3 runs. Peak memory is read from outside, so the
//! spike's `run.ps1` starts one process per size.
use std::hint::black_box;
use std::sync::atomic::AtomicBool;
use std::time::Instant;
use texalith::from_normal::{curvature, march, slopes, solve, AoOpts};

fn time(name: &str, mut f: impl FnMut()) {
    let mut t: Vec<f64> = (0..3)
        .map(|_| {
            let s = Instant::now();
            f();
            s.elapsed().as_secs_f64()
        })
        .collect();
    let first = t[0];
    t.sort_by(f64::total_cmp);
    println!("{name}: first {first:.3} s, median {:.3} s", t[1]);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let size: u32 = args.get(1).and_then(|s| s.parse().ok()).expect("a size, such as 4096");
    let wrap = match args.get(2).map(String::as_str) {
        Some("wrap") => true,
        Some("mirror") => false,
        _ => panic!("edge mode: wrap or mirror"),
    };
    let n = size as usize;
    // Any normal will do: no step branches on content except the march's horizon test.
    let mut seed = 1u64;
    let mut lcg = || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (seed >> 40) as f32 / (1u64 << 24) as f32 - 0.5
    };
    let px: Vec<[f32; 4]> = (0..n * n)
        .map(|i| {
            let (x, y) = ((i % n) as f32, (i / n) as f32);
            let r = 0.1 * lcg();
            [0.5 + 0.2 * (x * 0.05).sin() + r, 0.5 + 0.2 * (y * 0.07).cos() - r, 1.0, 1.0]
        })
        .collect();
    let mut s = None;
    time("slopes", || s = Some(slopes(&px, size, size, false)));
    let s = s.unwrap();
    let mut h = Vec::new();
    time("solve", || h = solve(&s, wrap, 0.3));
    time("curvature", || {
        black_box(curvature(&s, wrap, 0.3));
    });
    let no = AtomicBool::new(false);
    let ao8 = AoOpts {
        slices: 8,
        ..AoOpts::default()
    };
    time("AO 8 slices", || {
        black_box(march(&h, size, size, wrap, &ao8, &no));
    });
    time("AO 16 slices", || {
        black_box(march(&h, size, size, wrap, &AoOpts::default(), &no));
    });
    // The report's cavity: GTAO at radius W/128, 2 to 16 texels, 8 slices, steps min(radius, 8).
    let r = (size as f32 / 128.0).clamp(2.0, 16.0);
    let cavity = AoOpts {
        radius: r / size as f32,
        slices: 8,
        steps: (r as u32).min(8),
        ..AoOpts::default()
    };
    time("cavity", || {
        black_box(march(&h, size, size, wrap, &cavity, &no));
    });
}
