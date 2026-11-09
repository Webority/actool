//! Vector image sets rasterised off macOS match the pixels Xcode 26.5 stored.
//!
//! Off macOS `resvg` and `hayro` stand in for CoreSVG and CoreGraphics. Each
//! rendition is compared in straight alpha with the bitmap Xcode 26.5 stored
//! for `ref_samples/Raster.xcassets`, see `ref_samples/raster_output/pixels/CAPTURE.txt`.
#![cfg(all(test, not(target_os = "macos")))]

use actool::{pdf_raster, svg_raster};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

const SIDE: u32 = 24;
const ALPHA_LIMIT: u8 = 8;

/// How far a rasteriser may sit from Xcode's stored bitmap. Every pixel's alpha stays within
/// `ALPHA_LIMIT`, except that at most `edge_share` of the pixels may reach `edge_alpha`.
struct Tolerance {
    mean_rgb: f64,
    edge_alpha: u8,
    edge_share: f64,
}

/// hayro against CoreGraphics on vector.pdf, Xcode 26.5: mean RGB 0.35 to 0.94, alpha equal everywhere.
const PDF: Tolerance = Tolerance {
    mean_rgb: 4.0,
    edge_alpha: ALPHA_LIMIT,
    edge_share: 0.0,
};

/// resvg against CoreSVG on glyph.svg, Xcode 26.5: mean RGB 0.15 to 0.63; edge coverage differs
/// by at most 21 of 255 on 6, 20 and 20 pixels (1.04%, 0.87%, 0.39% of 1x, 2x, 3x).
const SVG: Tolerance = Tolerance {
    mean_rgb: 4.0,
    edge_alpha: 24,
    edge_share: 0.02,
};

/// A square RGBA image in straight alpha.
struct Image {
    side: usize,
    rgba: Vec<u8>,
}

struct Measured {
    mean_rgb: f64,
    max_alpha: u8,
    over_limit: usize,
    worst: usize,
}

fn unpremultiply(c: u8, a: u8) -> u8 {
    if a == 0 {
        return 0;
    }
    let (c, a) = (u32::from(c), u32::from(a));
    ((c * 255 + a / 2) / a).min(255) as u8
}

/// `rasterize_svg` and `rasterize_pdf` return premultiplied BGRA.
fn from_premultiplied_bgra(bgra: &[u8], side: usize) -> Image {
    let mut rgba = Vec::with_capacity(bgra.len());
    for &[b, g, r, a] in bgra.as_chunks::<4>().0 {
        rgba.extend_from_slice(&[
            unpremultiply(r, a),
            unpremultiply(g, a),
            unpremultiply(b, a),
            a,
        ]);
    }
    Image { side, rgba }
}

fn read_asset(relative: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/ref_samples/Raster.xcassets")
        .join(relative);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn reference(set: &str, scale: u32) -> Image {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/ref_samples/raster_output/pixels")
        .join(format!("{set}@{scale}x.png"));
    let file = File::open(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut decoder = png::Decoder::new(BufReader::new(file));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder
        .read_info()
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let size = reader
        .output_buffer_size()
        .unwrap_or_else(|| panic!("{}: no buffer size", path.display()));
    let mut buf = vec![0; size];
    let info = reader
        .next_frame(&mut buf)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    assert_eq!(
        info.color_type,
        png::ColorType::Rgba,
        "{}: not straight RGBA",
        path.display()
    );
    let want = SIDE * scale;
    assert_eq!(
        (info.width, info.height),
        (want, want),
        "{}: size",
        path.display()
    );
    buf.truncate(info.buffer_size());
    Image {
        side: want as usize,
        rgba: buf,
    }
}

fn measure(ours: &Image, theirs: &Image) -> Measured {
    let mut rgb_sum = 0u64;
    let mut max_alpha = 0u8;
    let mut worst = 0usize;
    let mut over_limit = 0usize;
    let pixels = ours
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .zip(theirs.rgba.as_chunks::<4>().0);
    for (i, (o, t)) in pixels.enumerate() {
        rgb_sum += o[..3]
            .iter()
            .zip(&t[..3])
            .map(|(a, b)| u64::from(a.abs_diff(*b)))
            .sum::<u64>();
        let da = o[3].abs_diff(t[3]);
        if da > max_alpha {
            max_alpha = da;
            worst = i;
        }
        if da > ALPHA_LIMIT {
            over_limit += 1;
        }
    }
    let channels = (ours.rgba.len() / 4 * 3) as f64;
    Measured {
        mean_rgb: rgb_sum as f64 / channels,
        max_alpha,
        over_limit,
        worst,
    }
}

fn check_set(set: &str, limit: &Tolerance, render: impl Fn(u32) -> Image) {
    eprintln!(
        "{:<12} {:>10} {:>10} {:>12}",
        "image", "mean RGB", "max alpha", "alpha>8 px"
    );
    let mut failures = Vec::new();
    for scale in 1..=3u32 {
        let ours = render(scale);
        let theirs = reference(set, scale);
        assert_eq!(ours.side, theirs.side, "{set}@{scale}x: rendered size");
        assert_eq!(ours.rgba.len(), theirs.rgba.len());
        let m = measure(&ours, &theirs);
        let name = format!("{set}@{scale}x");
        eprintln!(
            "{:<12} {:>10.3} {:>10} {:>12}",
            name, m.mean_rgb, m.max_alpha, m.over_limit
        );
        let edge_share = m.over_limit as f64 / (ours.rgba.len() / 4) as f64;
        if m.mean_rgb >= limit.mean_rgb
            || m.max_alpha > limit.edge_alpha
            || edge_share > limit.edge_share
        {
            let side = ours.side;
            let (x, y) = (m.worst % side, m.worst / side);
            failures.push(format!(
                "{set}@{scale}x: mean RGB {:.3} (limit < {}), max alpha {} (limit <= {}), \
                 alpha over {ALPHA_LIMIT} on {:.2}% of pixels (limit {:.2}%); \
                 worst alpha pixel ({x}, {y}): ours {:?}, Xcode {:?}",
                m.mean_rgb,
                limit.mean_rgb,
                m.max_alpha,
                limit.edge_alpha,
                edge_share * 100.0,
                limit.edge_share * 100.0,
                &ours.rgba[m.worst * 4..][..4],
                &theirs.rgba[m.worst * 4..][..4],
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn glyph_svg_matches_xcode_pixels() {
    let svg = read_asset("Glyph.imageset/glyph.svg");
    check_set("Glyph", &SVG, |scale| {
        let bgra = svg_raster::rasterize_svg(&svg, SIDE, SIDE, scale)
            .unwrap_or_else(|e| panic!("rasterize glyph.svg @{scale}x: {e}"));
        from_premultiplied_bgra(&bgra, (SIDE * scale) as usize)
    });
}

#[test]
fn vector_pdf_matches_xcode_pixels() {
    let pdf = read_asset("Vector.imageset/vector.pdf");
    check_set("Vector", &PDF, |scale| {
        let page = pdf_raster::rasterize_pdf(&pdf, scale)
            .unwrap_or_else(|e| panic!("rasterize vector.pdf @{scale}x: {e}"));
        let want = SIDE * scale;
        assert_eq!(
            (page.width, page.height),
            (want, want),
            "vector.pdf @{scale}x: page size"
        );
        from_premultiplied_bgra(&page.bgra, want as usize)
    });
}
