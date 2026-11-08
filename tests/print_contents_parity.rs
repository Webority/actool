//! `--print-contents` parity: the contents tree of each reference catalog, parsed as a plist,
//! equals the plist Apple's actool wrote for it. Compared as parsed values, not bytes.

use actool::catalog::AssetCatalog;
use actool::print_contents::{TOP_KEY, catalog_contents, to_plist_xml};
use std::path::PathBuf;

fn ref_samples() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/ref_samples")
}

fn assert_matches_fixture(catalog_name: &str, platform: &str, fixture: &str) {
    let root = ref_samples();
    let catalog = AssetCatalog::new(
        root.join(catalog_name),
        platform.to_string(),
        "15.1".to_string(),
        None,
        None,
        None,
    );
    let contents = catalog_contents(&catalog).expect("catalog contents");
    let ours: plist::Value =
        plist::from_bytes(to_plist_xml(TOP_KEY, &contents).as_bytes()).expect("our plist parses");
    let expected: plist::Value =
        plist::from_file(root.join("print_contents").join(fixture)).expect("fixture plist parses");
    assert_eq!(ours, expected, "{fixture}");
}

#[test]
fn print_contents_catalog_iphoneos() {
    assert_matches_fixture("Catalog.xcassets", "iphoneos", "Catalog-iphoneos.plist");
}

#[test]
fn print_contents_catalog_macosx() {
    assert_matches_fixture("Catalog.xcassets", "macosx", "Catalog-macosx.plist");
}

#[test]
fn print_contents_ios_single_icon_iphoneos() {
    assert_matches_fixture(
        "IosSingleIcon.xcassets",
        "iphoneos",
        "IosSingleIcon-iphoneos.plist",
    );
}

#[test]
fn print_contents_ios_single_icon_macosx() {
    assert_matches_fixture(
        "IosSingleIcon.xcassets",
        "macosx",
        "IosSingleIcon-macosx.plist",
    );
}

#[test]
fn print_contents_raster_iphoneos() {
    assert_matches_fixture("Raster.xcassets", "iphoneos", "Raster-iphoneos.plist");
}
