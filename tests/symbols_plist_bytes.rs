//! Byte-for-byte check of the symbols plist written by `generate_symbol_index`.

#![cfg(test)]

use actool::symbols::generate_symbol_index;
use std::fs;
use std::path::Path;

const EXPECTED: &str = include_str!("fixtures/symbols-plist.expected");
const CATALOG_PLACEHOLDER: &str = "CATALOG";

fn write_set(root: &Path, dir_name: &str, json_key: &str) {
    let dir = root.join(dir_name);
    fs::create_dir_all(&dir).expect("create asset dir");
    let contents = format!(r#"{{"{json_key}":[{{"idiom":"universal"}}]}}"#);
    fs::write(dir.join("Contents.json"), contents).expect("write Contents.json");
}

#[test]
fn symbols_plist_matches_fixture_bytes() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let catalog = tmp.path().join("Assets.xcassets");
    write_set(&catalog, "Amp&Name.colorset", "colors");
    write_set(&catalog, "Ctl\u{1}Name.colorset", "colors");
    write_set(&catalog, "Lt<Name.colorset", "colors");
    write_set(&catalog, "Icon&One.imageset", "images");
    write_set(&catalog, "Pic<Two.imageset", "images");

    let output = tmp.path().join("out").join("symbols.plist");
    generate_symbol_index(&catalog, &output, "macosx").expect("generate symbol index");

    let canonical = dunce::canonicalize(&catalog)
        .expect("canonical catalog")
        .to_string_lossy()
        .into_owned();
    let actual = fs::read_to_string(&output)
        .expect("read symbols plist")
        .replace(&canonical, CATALOG_PLACEHOLDER);
    assert_eq!(actual, EXPECTED);
}
