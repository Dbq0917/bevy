//! Offline measurement harness for the runtime vector-asset assessment.
//!
//! Modes:
//!   (no args)            Full run: scan assets/**/*.svg, measure parse/raster
//!                        timings, produce quality-comparison PNGs and a report.
//!   --cold <svg>         Parse + rasterize one file once, print one JSON line.
//!                        Used to measure cold-start cost in a fresh process.
//!   --expect-error <p>   Attempt to load <p>; exit 0 iff a structured error
//!                        is produced (asserts "no silent blank" behavior).

use resvg::{tiny_skia, usvg};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Instant;

const SCALES: [f32; 3] = [1.0, 2.0, 3.0];

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let assets = root.join("../../assets");
    let out = root.join("out");
    std::fs::create_dir_all(&out).unwrap();

    match args.get(1).map(String::as_str) {
        Some("--cold") => cold_mode(Path::new(&args[2])),
        Some("--expect-error") => expect_error_mode(Path::new(&args[2])),
        _ => full_run(&assets, &out),
    }
}

fn parse(data: &[u8]) -> Result<usvg::Tree, usvg::Error> {
    usvg::Tree::from_data(data, &usvg::Options::default())
}

fn rasterize(tree: &usvg::Tree, scale: f32) -> tiny_skia::Pixmap {
    let size = tree.size().to_int_size();
    let w = ((size.width() as f32 * scale).ceil() as u32).max(1);
    let h = ((size.height() as f32 * scale).ceil() as u32).max(1);
    let mut pixmap = tiny_skia::Pixmap::new(w, h).unwrap();
    resvg::render(tree, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    pixmap
}

/// "Fast" quality: direct raster at target size (tiny-skia's built-in AA).
fn render_fast(tree: &usvg::Tree, scale: f32) -> tiny_skia::Pixmap {
    rasterize(tree, scale)
}

/// "High" quality: 2x supersampled raster, bicubic downscale to target.
fn render_high(tree: &usvg::Tree, scale: f32) -> tiny_skia::Pixmap {
    let big = rasterize(tree, scale * 2.0);
    let size = tree.size().to_int_size();
    let w = ((size.width() as f32 * scale).ceil() as u32).max(1);
    let h = ((size.height() as f32 * scale).ceil() as u32).max(1);
    let mut pixmap = tiny_skia::Pixmap::new(w, h).unwrap();
    let paint = tiny_skia::PixmapPaint {
        opacity: 1.0,
        blend_mode: tiny_skia::BlendMode::Source,
        quality: tiny_skia::FilterQuality::Bicubic,
    };
    pixmap.draw_pixmap(
        0,
        0,
        big.as_ref(),
        &paint,
        tiny_skia::Transform::from_scale(0.5, 0.5),
        None,
    );
    pixmap
}

/// Simulates the bitmap pipeline: raster once at 1x, upscale to target.
fn render_bitmap_upscaled(tree: &usvg::Tree, scale: f32) -> tiny_skia::Pixmap {
    let base = rasterize(tree, 1.0);
    let size = tree.size().to_int_size();
    let w = ((size.width() as f32 * scale).ceil() as u32).max(1);
    let h = ((size.height() as f32 * scale).ceil() as u32).max(1);
    let mut pixmap = tiny_skia::Pixmap::new(w, h).unwrap();
    let paint = tiny_skia::PixmapPaint {
        opacity: 1.0,
        blend_mode: tiny_skia::BlendMode::Source,
        quality: tiny_skia::FilterQuality::Bilinear,
    };
    pixmap.draw_pixmap(
        0,
        0,
        base.as_ref(),
        &paint,
        tiny_skia::Transform::from_scale(scale, scale),
        None,
    );
    pixmap
}

fn luminance(data: &[u8], i: usize) -> f32 {
    0.2126 * data[i] as f32 + 0.7152 * data[i + 1] as f32 + 0.0722 * data[i + 2] as f32
}

/// Mean absolute error over all pixels, and over "edge" pixels only
/// (pixels where the reference has a strong luminance gradient).
fn mae(reference: &tiny_skia::Pixmap, candidate: &tiny_skia::Pixmap) -> (f64, f64) {
    let (w, h) = (reference.width().min(candidate.width()), reference.height().min(candidate.height()));
    let (r, c) = (reference.data(), candidate.data());
    let (mut sum_all, mut sum_edge, mut n_edge) = (0f64, 0f64, 0u64);
    for y in 0..h as usize {
        for x in 0..w as usize {
            let i = (y * w as usize + x) * 4;
            let diff = (0..3).map(|k| (r[i + k] as f64 - c[i + k] as f64).abs()).sum::<f64>() / 3.0;
            sum_all += diff;
            let gx = if x + 1 < w as usize { (luminance(r, i + 4) - luminance(r, i)).abs() } else { 0.0 };
            let gy = if y + 1 < h as usize { (luminance(r, i + 4 * w as usize) - luminance(r, i)).abs() } else { 0.0 };
            if gx + gy > 32.0 {
                sum_edge += diff;
                n_edge += 1;
            }
        }
    }
    let n = (w * h) as f64;
    (sum_all / n, if n_edge > 0 { sum_edge / n_edge as f64 } else { 0.0 })
}

fn cold_mode(path: &Path) {
    let t0 = Instant::now();
    let data = std::fs::read(path).unwrap();
    let t_read = t0.elapsed();
    let t1 = Instant::now();
    let tree = parse(&data).unwrap();
    let t_parse = t1.elapsed();
    let t2 = Instant::now();
    let _ = rasterize(&tree, 1.0);
    let t_raster = t2.elapsed();
    println!(
        "{{\"file\":{:?},\"bytes\":{},\"read_us\":{},\"parse_us\":{},\"raster_1x_us\":{},\"total_us\":{}}}",
        path.file_name().unwrap().to_string_lossy(),
        data.len(),
        t_read.as_micros(),
        t_parse.as_micros(),
        t_raster.as_micros(),
        t0.elapsed().as_micros()
    );
}

fn expect_error_mode(path: &Path) {
    match std::fs::read(path) {
        Err(e) => {
            println!("io_error: {e}");
            std::process::exit(0);
        }
        Ok(data) => match parse(&data) {
            Err(e) => {
                println!("parse_error: {e}");
                std::process::exit(0);
            }
            Ok(_) => {
                eprintln!("FAIL: expected an error but parsing succeeded");
                std::process::exit(1);
            }
        },
    }
}

fn full_run(assets: &Path, out: &Path) {
    let mut svgs: Vec<PathBuf> = Vec::new();
    collect_svgs(assets, &mut svgs);
    svgs.sort();

    let mut md = String::new();
    let mut json_files = String::new();
    writeln!(md, "| asset | bytes | parse (us) | fast 1x/2x/3x (us) | high 2x/3x (us) | edge MAE bitmap@2x | edge MAE bitmap@3x | edge MAE fast@2x |").unwrap();
    writeln!(md, "|---|---|---|---|---|---|---|---|").unwrap();

    let mut total_bytes = 0u64;
    let mut total_parse_us = 0u128;
    let mut total_fast_us = 0u128;

    for path in &svgs {
        let name = path.strip_prefix(assets).unwrap().to_string_lossy().replace('\\', "/");
        let data = std::fs::read(path).unwrap();
        total_bytes += data.len() as u64;

        let t = Instant::now();
        let tree = parse(&data).unwrap();
        let parse_us = t.elapsed().as_micros();
        total_parse_us += parse_us;

        let mut fast_us = [0u128; 3];
        let mut high_us = [0u128; 3];
        let mut refs = Vec::new();
        for (i, &s) in SCALES.iter().enumerate() {
            let t = Instant::now();
            let pixmap = render_fast(&tree, s);
            fast_us[i] = t.elapsed().as_micros();
            total_fast_us += fast_us[i];
            refs.push(pixmap);
        }
        for (i, &s) in SCALES.iter().enumerate().skip(1) {
            let t = Instant::now();
            let _ = render_high(&tree, s);
            high_us[i] = t.elapsed().as_micros();
        }

        // Quality comparison vs the bitmap (rasterize-once-then-upscale) pipeline.
        let stem = path.file_stem().unwrap().to_string_lossy();
        let mut edge_mae_bitmap = [0f64; 3];
        let mut edge_mae_fast = [0f64; 3];
        for (i, &s) in SCALES.iter().enumerate().skip(1) {
            let bitmap = render_bitmap_upscaled(&tree, s);
            let high = render_high(&tree, s);
            let (_, e_bitmap) = mae(&refs[i], &bitmap);
            let (_, e_fast) = mae(&high, &refs[i]);
            edge_mae_bitmap[i] = e_bitmap;
            edge_mae_fast[i] = e_fast;
            if s == 3.0 {
                refs[i].save_png(out.join(format!("{stem}_vector_3x.png"))).unwrap();
                bitmap.save_png(out.join(format!("{stem}_bitmap_upscaled_3x.png"))).unwrap();
            }
        }

        writeln!(
            md,
            "| {name} | {} | {parse_us} | {}/{}/{} | {}/{} | {:.1} | {:.1} | {:.1} |",
            data.len(),
            fast_us[0], fast_us[1], fast_us[2],
            high_us[1], high_us[2],
            edge_mae_bitmap[1], edge_mae_bitmap[2], edge_mae_fast[1],
        ).unwrap();
        writeln!(
            json_files,
            "  {{\"file\":\"{name}\",\"bytes\":{},\"parse_us\":{parse_us},\"fast_us\":[{},{},{}]}}",
            data.len(), fast_us[0], fast_us[1], fast_us[2],
        ).unwrap();
    }

    // Error-handling assertions: missing file and corrupt file must error, never blank.
    let corrupt = out.join("corrupt_truncated.svg");
    let first = std::fs::read(&svgs[0]).unwrap();
    std::fs::write(&corrupt, &first[..first.len() / 2]).unwrap();
    let missing_result = match std::fs::read(assets.join("branding/does_not_exist.svg")) {
        Err(e) => format!("io_error: {e}"),
        Ok(_) => "UNEXPECTED OK".into(),
    };
    let corrupt_result = match parse(&std::fs::read(&corrupt).unwrap()) {
        Err(e) => format!("parse_error: {e}"),
        Ok(_) => "UNEXPECTED OK".into(),
    };

    let summary = format!(
        "# Vector asset measurements\n\n\
         - SVG assets found: {}\n\
         - Total SVG bytes: {total_bytes}\n\
         - Total parse time (all assets, cold): {total_parse_us} us\n\
         - Total fast-raster time (all assets, 3 scales): {total_fast_us} us\n\
         - Missing file behavior: {missing_result}\n\
         - Corrupt file behavior: {corrupt_result}\n\n{md}",
        svgs.len()
    );
    std::fs::write(out.join("report.md"), &summary).unwrap();
    std::fs::write(
        out.join("report.json"),
        format!("{{\n\"svg_count\":{},\n\"total_bytes\":{total_bytes},\n\"total_parse_us\":{total_parse_us},\n\"files\":[\n{json_files}]\n}}", svgs.len()),
    )
    .unwrap();
    print!("{summary}");
}

fn collect_svgs(dir: &Path, acc: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_svgs(&path, acc);
        } else if path.extension().is_some_and(|e| e == "svg") {
            acc.push(path);
        }
    }
}
