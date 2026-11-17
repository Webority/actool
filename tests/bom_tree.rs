//! Multi-leaf BOM trees: the interior root must match Apple's layout, or `assetutil`
//! crashes on the car (a RENDITIONS tree of more than 510 renditions is two leaves).

use actool::bom::BomWriter;
use std::collections::HashMap;

const BLOCK_SIZE: usize = 4096;
/// Entries per RENDITIONS leaf: (4096 - 12) / 8.
const LEAF_CAPACITY: usize = 510;

fn be16(b: &[u8], at: usize) -> u16 {
    u16::from_be_bytes([b[at], b[at + 1]])
}

fn be32(b: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

fn keyed_entries(n: usize, key_size: usize) -> Vec<(Vec<u8>, Vec<u8>)> {
    (0..n)
        .map(|i| {
            let mut key = (i as u32).to_be_bytes().to_vec();
            key.resize(key_size, 0);
            (key, (i as u32).to_le_bytes().to_vec())
        })
        .collect()
}

/// Reads a BOM through its index table (block number to offset) and its variables
/// table (tree name to header block), as `target_device_parity.rs` does.
struct Bom {
    data: Vec<u8>,
    blocks: Vec<(usize, usize)>,
    named: HashMap<String, usize>,
}

impl Bom {
    fn parse(data: Vec<u8>) -> Self {
        assert_eq!(&data[..8], b"BOMStore");
        let index_at = be32(&data, 16) as usize;
        let index_len = be32(&data, 20) as usize;
        let vars_at = be32(&data, 24) as usize;
        let index = &data[index_at..index_at + index_len];
        let blocks = (0..be32(index, 0) as usize)
            .map(|i| {
                (
                    be32(index, 4 + 8 * i) as usize,
                    be32(index, 8 + 8 * i) as usize,
                )
            })
            .collect();
        let vars = &data[vars_at..];
        let mut named = HashMap::new();
        let mut at = 4;
        for _ in 0..be32(vars, 0) {
            let block = be32(vars, at) as usize;
            let len = vars[at + 4] as usize;
            let name = String::from_utf8_lossy(&vars[at + 5..at + 5 + len]).into_owned();
            named.insert(name, block);
            at += 5 + len;
        }
        Self {
            data,
            blocks,
            named,
        }
    }

    fn block(&self, idx: usize) -> &[u8] {
        let (at, len) = self.blocks[idx];
        &self.data[at..at + len]
    }

    fn tree_header(&self, name: &str) -> &[u8] {
        self.block(self.named[name])
    }

    fn tree_root(&self, name: &str) -> &[u8] {
        self.block(be32(self.tree_header(name), 8) as usize)
    }
}

/// `entries` split into full leaves of `LEAF_CAPACITY` and a remainder, under one
/// interior root whose separators are stored inline (RENDITIONS layout).
fn check_fixed_key_tree(bom: &Bom, tree: &str, entries: &[(Vec<u8>, Vec<u8>)], key_size: usize) {
    let header = bom.tree_header(tree);
    assert_eq!(
        be32(header, 16) as usize,
        entries.len(),
        "{tree} path count"
    );
    assert_eq!(be32(header, 21) as usize, key_size, "{tree} key size");

    let root = bom.tree_root(tree);
    let leaves = entries.len().div_ceil(LEAF_CAPACITY);
    let separators = leaves - 1;
    assert_eq!(be16(root, 0), 0, "{tree} root is interior");
    assert_eq!(be16(root, 2) as usize, separators, "{tree} separator count");
    assert_eq!(
        root.len(),
        BLOCK_SIZE + separators * key_size,
        "{tree} root node size"
    );
    let inline_keys = 16 + 8 * separators;

    for leaf_no in 0..leaves {
        let leaf = bom.block(be32(root, 12 + 8 * leaf_no) as usize);
        let first = leaf_no * LEAF_CAPACITY;
        let count = (entries.len() - first).min(LEAF_CAPACITY);
        assert_eq!(be16(leaf, 0), 1, "leaf {leaf_no} is a leaf");
        assert_eq!(be16(leaf, 2) as usize, count, "leaf {leaf_no} entry count");
        assert_eq!(
            bom.block(be32(leaf, 16) as usize),
            entries[first].0.as_slice(),
            "leaf {leaf_no} starts at its first key"
        );
        if leaf_no < separators {
            let last = first + count - 1;
            let separator = be32(root, 16 + 8 * leaf_no);
            let last_key_block = be32(leaf, 12 + 8 * (count - 1) + 4);
            assert_eq!(
                separator, last_key_block,
                "separator of leaf {leaf_no} is its last key block"
            );
            assert_eq!(
                bom.block(separator as usize),
                entries[last].0.as_slice(),
                "separator of leaf {leaf_no} holds its last key"
            );
            let inline_at = inline_keys + key_size * leaf_no;
            assert_eq!(
                &root[inline_at..inline_at + key_size],
                entries[last].0.as_slice(),
                "inline separator {leaf_no}"
            );
        }
    }
}

#[test]
fn renditions_tree_of_700_entries_has_one_separator() {
    let entries = keyed_entries(700, 18);
    let mut bom = BomWriter::new();
    bom.set_inline_key_size(Some(18));
    bom.add_tree("RENDITIONS", &entries, BLOCK_SIZE as u32)
        .expect("700 entries fit two leaves");
    check_fixed_key_tree(&Bom::parse(bom.to_bytes()), "RENDITIONS", &entries, 18);
}

#[test]
fn renditions_tree_of_1100_entries_has_two_separators() {
    let entries = keyed_entries(1100, 18);
    let mut bom = BomWriter::new();
    bom.set_inline_key_size(Some(18));
    bom.add_tree("RENDITIONS", &entries, BLOCK_SIZE as u32)
        .expect("1100 entries fit three leaves");
    check_fixed_key_tree(&Bom::parse(bom.to_bytes()), "RENDITIONS", &entries, 18);
}

#[test]
fn tree_beyond_one_interior_node_is_an_error() {
    // A 64-byte block holds 6 entries per leaf, and one interior node indexes 7 leaves:
    // 42 entries fit, 43 do not.
    let mut fits = BomWriter::new();
    fits.add_tree("SMALL", &keyed_entries(42, 4), 64)
        .expect("42 entries fit one interior node");

    let err = BomWriter::new()
        .add_tree("SMALL", &keyed_entries(43, 4), 64)
        .expect_err("43 entries need a second interior level");
    assert!(err.to_string().contains("43 entries"), "{err}");
}

#[cfg(target_os = "macos")]
mod assetutil {
    use actool::compiler::{self, CatalogOptions};
    use std::path::Path;
    use std::process::Command;

    const IMAGE_SETS: usize = 520;

    fn one_pixel_png() -> anyhow::Result<Vec<u8>> {
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, 1, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(&[255, 0, 0, 255])?;
        writer.finish()?;
        Ok(out)
    }

    #[test]
    fn catalog_of_520_image_sets_passes_assetutil() {
        let assetutil = Path::new("/usr/bin/assetutil");
        if !assetutil.exists() {
            eprintln!("Skipping: {} not found", assetutil.display());
            return;
        }

        let dir = tempfile::tempdir().expect("temp dir");
        let xcassets = dir.path().join("Catalog.xcassets");
        std::fs::create_dir_all(&xcassets).expect("catalog dir");
        std::fs::write(
            xcassets.join("Contents.json"),
            serde_json::json!({"info": {"author": "xcode", "version": 1}}).to_string(),
        )
        .expect("catalog Contents.json");

        let png = one_pixel_png().expect("encode one-pixel PNG");
        for i in 0..IMAGE_SETS {
            let set = xcassets.join(format!("Image{i:03}.imageset"));
            std::fs::create_dir_all(&set).expect("imageset dir");
            let file = format!("image{i:03}.png");
            std::fs::write(set.join(&file), &png).expect("write png");
            let contents = serde_json::json!({
                "images": [{"idiom": "universal", "filename": file}],
                "info": {"author": "xcode", "version": 1}
            });
            std::fs::write(set.join("Contents.json"), contents.to_string())
                .expect("imageset Contents.json");
        }

        let out = dir.path().join("out");
        let files = compiler::compile_catalog_with_icons(
            &xcassets,
            &out,
            &CatalogOptions {
                platform: "macosx",
                min_deploy: "11.0",
                ..Default::default()
            },
        )
        .expect("compile 520 image sets");
        assert!(files.iter().any(|p| p.ends_with("Assets.car")));

        let run = Command::new(assetutil)
            .arg("-I")
            .arg(out.join("Assets.car"))
            .output()
            .expect("run assetutil");
        assert!(
            run.status.success(),
            "assetutil -I exit {:?}: {}",
            run.status.code(),
            String::from_utf8_lossy(&run.stderr)
        );
    }
}
