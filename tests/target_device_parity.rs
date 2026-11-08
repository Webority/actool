//! `--target-device` drops the imageset images of the device idioms that are not targeted, as
//! Xcode 26.5's actool does, and never filters app icon sets.
//!
//! References: `ref_samples/target_device_output/{iphone,iphone-ipad}`, captured from Xcode 26.5
//! over `ref_samples/DeviceSplit.xcassets`.
#![cfg(test)]

use actool::compiler::{self, IconSelection};
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
fn read_u16_le(b: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([b[off], b[off + 1]])
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

    /// (key, value) pairs of a tree, left to right.
    fn tree_pairs(&self, name: &str) -> Vec<(&[u8], &[u8])> {
        fn walk<'a>(bom: &'a Bom, idx: u32, out: &mut Vec<(&'a [u8], &'a [u8])>) {
            let b = bom.block(idx);
            let count = read_u16_be(b, 2) as usize;
            if read_u16_be(b, 0) != 0 {
                for i in 0..count {
                    let value = bom.block(read_u32_be(b, 12 + i * 8));
                    let key = bom.block(read_u32_be(b, 12 + i * 8 + 4));
                    out.push((key, value));
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

/// An image rendition as (asset name, idiom, scale).
type Shape = (String, String, u32);

fn idiom_name(idiom: u16) -> &'static str {
    match idiom {
        0 => "universal",
        1 => "phone",
        2 => "pad",
        6 => "marketing",
        _ => "other",
    }
}

fn car_shapes(car: &Path) -> Vec<Shape> {
    let bom = Bom::open(car);
    let kf = bom.block(bom.named["KEYFORMAT"]);
    let attrs: Vec<u32> = (0..read_u32_le(kf, 8) as usize)
        .map(|i| read_u32_le(kf, 12 + i * 4))
        .collect();
    let column = |attr: u32| attrs.iter().position(|a| *a == attr).expect("key column");
    let (idiom_col, scale_col, ident_col) = (column(15), column(12), column(17));

    let names: HashMap<u16, String> = bom
        .tree_pairs("FACETKEYS")
        .into_iter()
        .filter_map(|(key, value)| {
            let pairs = read_u16_le(value, 4) as usize;
            (0..pairs)
                .find(|p| read_u16_le(value, 6 + p * 4) == 17)
                .map(|p| {
                    (
                        read_u16_le(value, 8 + p * 4),
                        String::from_utf8_lossy(key).into_owned(),
                    )
                })
        })
        .collect();

    let mut shapes: Vec<Shape> = bom
        .tree_pairs("RENDITIONS")
        .into_iter()
        .filter(|(_, csi)| {
            assert_eq!(&csi[..4], b"ISTC", "rendition is not a CSI record");
            !csi[40..168].starts_with(b"ZZZZPackedAsset")
        })
        .map(|(key, _)| {
            let ident = read_u16_le(key, ident_col * 2);
            (
                names.get(&ident).cloned().unwrap_or_default(),
                idiom_name(read_u16_le(key, idiom_col * 2)).to_string(),
                u32::from(read_u16_le(key, scale_col * 2)),
            )
        })
        .collect();
    shapes.sort();
    shapes
}

fn reference_shapes(info: &Path) -> Vec<Shape> {
    let text = std::fs::read_to_string(info).expect("read assets-info.json");
    let rows: Vec<serde_json::Value> = serde_json::from_str(&text).expect("assets-info.json");
    let mut shapes: Vec<Shape> = rows
        .iter()
        .filter(|row| row.get("AssetType").is_some_and(|t| t != "PackedImage"))
        .map(|row| {
            (
                row["Name"].as_str().unwrap_or_default().to_string(),
                row["Idiom"].as_str().unwrap_or("universal").to_string(),
                row["Scale"].as_u64().unwrap_or(0) as u32,
            )
        })
        .collect();
    shapes.sort();
    shapes
}

/// File names in `compiled-files.txt`: the text before the first tab on each line.
fn reference_files(list: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(list).expect("read compiled-files.txt");
    let mut names: Vec<String> = text
        .lines()
        .filter_map(|line| line.split('\t').next())
        .map(str::to_string)
        .collect();
    names.sort();
    names
}

fn loose_files(out: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(out)
        .expect("read output dir")
        .map(|e| {
            e.expect("dir entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

fn check(reference: &str, target_devices: &[&str]) {
    let reference = Path::new("tests/ref_samples/target_device_output").join(reference);
    let out = tempfile::TempDir::new().expect("tempdir");
    let plist_dir = tempfile::TempDir::new().expect("tempdir");
    let devices: Vec<String> = target_devices.iter().map(|d| d.to_string()).collect();
    compiler::compile_catalog_with_icons(
        Path::new("tests/ref_samples/DeviceSplit.xcassets"),
        out.path(),
        "iphoneos",
        "15.1",
        Some("AppIcon"),
        &IconSelection::default(),
        Some(&plist_dir.path().join("partial.plist")),
        None,
        None,
        "default",
        None,
        None,
        true,
        true,
        &devices,
    )
    .expect("compile");

    let want = reference_shapes(&reference.join("assets-info.json"));
    assert_eq!(
        car_shapes(&reference.join("compiled/Assets.car")),
        want,
        "the reader disagrees with assets-info.json on the Xcode car"
    );
    assert_eq!(car_shapes(&out.path().join("Assets.car")), want);
    assert_eq!(
        loose_files(out.path()),
        reference_files(&reference.join("compiled-files.txt"))
    );
}

#[test]
fn iphone_only_drops_pad_imageset_images() {
    check("iphone", &["iphone"]);
}

#[test]
fn iphone_and_ipad_keep_both() {
    check("iphone-ipad", &["iphone", "ipad"]);
}
