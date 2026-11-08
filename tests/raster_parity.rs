//! Vector image sets compile to the renditions Xcode 26.5's actool writes.
//!
//! Off macOS the SVG and PDF images are rasterized by `resvg` and `hayro`
//! instead of CoreSVG and CoreGraphics, which are Apple's own on a Mac. The
//! reference is `ref_samples/raster_output`, captured from Xcode 26.5 over
//! `ref_samples/Raster.xcassets`.
#![cfg(not(target_os = "macos"))]

use actool::compiler;
use std::collections::HashMap;
use std::path::Path;

fn read_u32_be(b: &[u8], off: usize) -> u32 {
    u32::from_be_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}
fn read_u16_be(b: &[u8], off: usize) -> u16 {
    u16::from_be_bytes([b[off], b[off + 1]])
}
fn read_u32_le(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}

struct Bom {
    data: Vec<u8>,
    blocks: HashMap<u32, (usize, usize)>,
    named: HashMap<String, u32>,
}

impl Bom {
    fn open(path: &Path) -> Bom {
        let data = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_eq!(&data[..8], b"BOMStore", "{}: not a BOM", path.display());
        let idx_off = read_u32_be(&data, 16) as usize;
        let idx_len = read_u32_be(&data, 20) as usize;
        let idx = &data[idx_off..idx_off + idx_len];
        let mut blocks = HashMap::new();
        for i in 0..read_u32_be(idx, 0) {
            let at = 4 + i as usize * 8;
            blocks.insert(
                i,
                (
                    read_u32_be(idx, at) as usize,
                    read_u32_be(idx, at + 4) as usize,
                ),
            );
        }
        let vars_off = read_u32_be(&data, 24) as usize;
        let vars_len = read_u32_be(&data, 28) as usize;
        let vars = &data[vars_off..vars_off + vars_len];
        let mut named = HashMap::new();
        let mut p = 4usize;
        for _ in 0..read_u32_be(vars, 0) {
            let block = read_u32_be(vars, p);
            let len = vars[p + 4] as usize;
            let name = String::from_utf8_lossy(&vars[p + 5..p + 5 + len]).into_owned();
            p += 5 + len;
            named.insert(name, block);
        }
        Bom {
            data,
            blocks,
            named,
        }
    }

    fn block(&self, idx: u32) -> &[u8] {
        let (addr, len) = self.blocks[&idx];
        &self.data[addr..addr + len]
    }

    /// Values of a tree, left to right.
    fn tree_values(&self, name: &str) -> Vec<&[u8]> {
        fn walk<'a>(bom: &'a Bom, idx: u32, out: &mut Vec<&'a [u8]>) {
            let b = bom.block(idx);
            let count = read_u16_be(b, 2) as usize;
            if read_u16_be(b, 0) != 0 {
                for i in 0..count {
                    out.push(bom.block(read_u32_be(b, 12 + i * 8)));
                }
            } else {
                walk(bom, read_u32_be(b, 12), out);
                for i in 0..count {
                    walk(bom, read_u32_be(b, 16 + i * 8 + 4), out);
                }
            }
        }
        let header = self.block(self.named[name]);
        let mut out = Vec::new();
        walk(self, read_u32_be(header, 8), &mut out);
        out
    }
}

/// What a rendition is called and how big it is: (name, scale, width, height).
type Shape = (String, u32, u32, u32);

fn car_shapes(car: &Path) -> Vec<Shape> {
    let bom = Bom::open(car);
    let mut shapes: Vec<Shape> = bom
        .tree_values("RENDITIONS")
        .into_iter()
        .map(|csi| {
            assert_eq!(&csi[..4], b"ISTC", "rendition is not a CSI record");
            let name = &csi[40..168];
            let end = name.iter().position(|b| *b == 0).unwrap_or(name.len());
            (
                String::from_utf8_lossy(&name[..end]).into_owned(),
                read_u32_le(csi, 20) / 100,
                read_u32_le(csi, 12),
                read_u32_le(csi, 16),
            )
        })
        .collect();
    shapes.sort();
    shapes
}

fn reference_shapes(info: &Path) -> Vec<Shape> {
    let text = std::fs::read_to_string(info).expect("read assets-info.json");
    let rows: Vec<serde_json::Value> = serde_json::from_str(&text).expect("assets-info.json");
    let number = |row: &serde_json::Value, key: &str| row[key].as_u64().unwrap_or(0) as u32;
    let mut shapes: Vec<Shape> = rows
        .iter()
        .filter(|row| row.get("AssetType").is_some())
        .map(|row| {
            (
                row["RenditionName"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
                number(row, "Scale"),
                number(row, "PixelWidth"),
                number(row, "PixelHeight"),
            )
        })
        .collect();
    shapes.sort();
    shapes
}

#[test]
fn vector_sets_compile_to_xcode_renditions() {
    let reference = Path::new("tests/ref_samples/raster_output");
    let out = tempfile::TempDir::new().expect("tempdir");
    compiler::compile_catalog(
        Path::new("tests/ref_samples/Raster.xcassets"),
        out.path(),
        "iphoneos",
        "15.1",
        None,
        Some(&out.path().join("partial.plist")),
        None,
        None,
        "default",
        None,
        None,
        true,
    )
    .expect("compile");

    let want = reference_shapes(&reference.join("assets-info.json"));
    assert_eq!(
        car_shapes(&reference.join("compiled/Assets.car")),
        want,
        "the reader disagrees with assets-info.json on the Xcode car"
    );
    assert_eq!(car_shapes(&out.path().join("Assets.car")), want);
}

#[test]
fn pure_rust_renderers_fill_every_scale() {
    let sets = Path::new("tests/ref_samples/Raster.xcassets");
    let svg = std::fs::read(sets.join("Glyph.imageset/glyph.svg")).expect("svg");
    let pdf = std::fs::read(sets.join("Vector.imageset/vector.pdf")).expect("pdf");
    for scale in 1..=3u32 {
        let side = 24 * scale;
        let bgra = actool::svg_raster::rasterize_svg(&svg, 24, 24, scale).expect("svg");
        assert_eq!(bgra.len(), (side * side * 4) as usize, "svg @{scale}x");
        let at = |x: u32, y: u32| &bgra[((y * side + x) * 4) as usize..][..4];
        assert_eq!(at(0, 0)[3], 0, "svg @{scale}x: corner is transparent");
        assert_eq!(
            at(side / 2, side / 2),
            [255, 255, 255, 255],
            "svg @{scale}x: centre is the white disc"
        );

        let page = actool::pdf_raster::rasterize_pdf(&pdf, scale).expect("pdf");
        assert_eq!((page.width, page.height, page.scale), (side, side, scale));
        assert_eq!(page.bgra.len(), (side * side * 4) as usize, "pdf @{scale}x");
        assert!(
            page.bgra.as_chunks::<4>().0.iter().any(|px| px[3] != 0),
            "pdf @{scale}x draws nothing"
        );
    }
}
