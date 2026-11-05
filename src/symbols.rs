//! Objective-C / Swift asset symbol generation.

use anyhow::Result;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

fn platform_idioms(platform: &str) -> HashSet<&'static str> {
    match platform {
        "macosx" => ["mac", "universal"].into_iter().collect(),
        "iphoneos" | "iphonesimulator" => ["iphone", "ipad", "ios-marketing", "car", "universal"]
            .into_iter()
            .collect(),
        "watchos" | "watchsimulator" => ["watch", "universal"].into_iter().collect(),
        "appletvos" | "appletvsimulator" => ["tv", "universal"].into_iter().collect(),
        _ => ["universal"].into_iter().collect(),
    }
}

#[derive(Debug, Clone)]
pub enum AssetKind {
    Image,
    Color,
}

impl AssetKind {
    fn as_str(&self) -> &'static str {
        match self {
            AssetKind::Image => "image",
            AssetKind::Color => "color",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Asset {
    pub kind: AssetKind,
    pub leaf_name: String,
    pub namespaced_name: String,
    pub relative_path: String,
}

fn entry_applies(contents_path: &Path, platform: &str, keys: &[&str]) -> bool {
    let content = match fs::read_to_string(contents_path) {
        Ok(s) => s,
        Err(_) => return false,
    };
    let parsed: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return false,
    };
    let allowed = platform_idioms(platform);
    for key in keys {
        if let Some(arr) = parsed.get(key).and_then(|v| v.as_array()) {
            for entry in arr {
                let idiom = entry
                    .get("idiom")
                    .and_then(|v| v.as_str())
                    .unwrap_or("universal");
                if allowed.contains(idiom) {
                    return true;
                }
            }
        }
    }
    false
}

pub fn walk_assets(xcassets_path: &Path, platform: &str) -> Vec<Asset> {
    let mut results = Vec::new();
    walk_assets_inner(xcassets_path, platform, "", "", &mut results);
    results
}

fn walk_assets_inner(
    root: &Path,
    platform: &str,
    rel_prefix: &str,
    namespace: &str,
    out: &mut Vec<Asset>,
) {
    if !root.is_dir() {
        return;
    }
    let mut entries: Vec<PathBuf> = match fs::read_dir(root) {
        Ok(rd) => rd.filter_map(|e| e.ok().map(|e| e.path())).collect(),
        Err(_) => return,
    };
    entries.sort();
    for item in entries {
        if !item.is_dir() {
            continue;
        }
        let contents = item.join("Contents.json");
        let name = item
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let namespaced = format!("{namespace}{name}");
        let file_name = item
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let rel = if rel_prefix.is_empty() {
            file_name.clone()
        } else {
            format!("{rel_prefix}{file_name}")
        };

        let suffix = item.extension().and_then(|s| s.to_str()).unwrap_or("");
        match suffix {
            "imageset" => {
                if entry_applies(&contents, platform, &["images"]) {
                    out.push(Asset {
                        kind: AssetKind::Image,
                        leaf_name: name,
                        namespaced_name: namespaced,
                        relative_path: rel,
                    });
                }
            }
            "colorset" => {
                if entry_applies(&contents, platform, &["colors"]) {
                    out.push(Asset {
                        kind: AssetKind::Color,
                        leaf_name: name,
                        namespaced_name: namespaced,
                        relative_path: rel,
                    });
                }
            }
            "" => {
                let mut child_ns = namespace.to_string();
                if contents.exists()
                    && let Ok(raw) = fs::read_to_string(&contents)
                    && let Ok(json) = serde_json::from_str::<serde_json::Value>(&raw)
                    && json
                        .get("properties")
                        .and_then(|p| p.get("provides-namespace"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
                {
                    child_ns = format!("{namespace}{name}/");
                }
                let child_rel = format!("{rel_prefix}{file_name}/");
                walk_assets_inner(&item, platform, &child_rel, &child_ns, out);
            }
            _ => {}
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Kind {
    Upper,
    Lower,
    Digit,
    Sep,
}

fn kind_of(ch: char) -> Kind {
    if ch.is_alphabetic() {
        if ch.is_uppercase() {
            Kind::Upper
        } else {
            Kind::Lower
        }
    } else if ch.is_ascii_digit() {
        Kind::Digit
    } else {
        Kind::Sep
    }
}

pub fn split_words(name: &str) -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut prev: Option<Kind> = None;
    for ch in name.chars() {
        let k = kind_of(ch);
        if let Kind::Sep = k {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            prev = Some(Kind::Sep);
            continue;
        }
        match prev {
            None | Some(Kind::Sep) => {
                current.push(ch);
            }
            Some(ref pk) if pk == &k => {
                current.push(ch);
            }
            Some(Kind::Lower) if k == Kind::Upper => {
                words.push(std::mem::take(&mut current));
                current.push(ch);
            }
            Some(Kind::Upper) if k == Kind::Lower => {
                // UPPERLower: last upper belongs with the lower run
                if current.chars().count() > 1
                    && let Some(last) = current.pop()
                {
                    words.push(std::mem::take(&mut current));
                    current.push(last);
                }
                current.push(ch);
            }
            _ => {
                words.push(std::mem::take(&mut current));
                current.push(ch);
            }
        }
        prev = Some(k);
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

pub fn objc_identifier(name: &str) -> String {
    let words = split_words(name);
    let mut out = String::new();
    for w in &words {
        if w.is_empty() {
            continue;
        }
        let mut chars = w.chars();
        if let Some(first) = chars.next() {
            if first.is_alphabetic() {
                for c in first.to_uppercase() {
                    out.push(c);
                }
                for c in chars {
                    out.push(c);
                }
            } else {
                out.push_str(w);
            }
        }
    }
    out
}

pub fn swift_identifier(name: &str, kind: &str) -> String {
    let mut words = split_words(name);
    let strip = match kind {
        "image" => Some("image"),
        "color" => Some("color"),
        "symbol" => Some("symbol"),
        _ => None,
    };
    if let Some(s) = strip
        && words.len() > 1
        && words.last().map(|w| w.to_lowercase()).as_deref() == Some(s)
    {
        words.pop();
    }

    let mut out = String::new();
    for (i, w) in words.iter().enumerate() {
        if w.is_empty() {
            continue;
        }
        if i == 0 {
            let all_upper_alpha =
                w.chars().all(|c| c.is_alphabetic()) && w.chars().all(|c| c.is_uppercase());
            if all_upper_alpha {
                out.push_str(&w.to_lowercase());
            } else {
                let mut chars = w.chars();
                if let Some(first) = chars.next() {
                    if first.is_alphabetic() {
                        for c in first.to_lowercase() {
                            out.push(c);
                        }
                        for c in chars {
                            out.push(c);
                        }
                    } else {
                        out.push_str(w);
                    }
                }
            }
        } else {
            let mut chars = w.chars();
            if let Some(first) = chars.next() {
                if first.is_alphabetic() {
                    for c in first.to_uppercase() {
                        out.push(c);
                    }
                    for c in chars {
                        out.push(c);
                    }
                } else {
                    out.push_str(w);
                }
            }
        }
    }
    if out
        .chars()
        .next()
        .map(|c| c.is_ascii_digit())
        .unwrap_or(false)
    {
        out.insert(0, '_');
    }
    out
}

pub fn objc_symbol_name(kind: &AssetKind, leaf_name: &str) -> String {
    let prefix = match kind {
        AssetKind::Color => "ACColorName",
        AssetKind::Image => "ACImageName",
    };
    format!("{prefix}{}", objc_identifier(leaf_name))
}

pub fn generate_symbols_header(
    xcassets_path: &Path,
    output_path: &Path,
    bundle_identifier: &str,
    platform: &str,
) -> Result<()> {
    let assets = walk_assets(xcassets_path, platform);
    let mut images: Vec<_> = assets
        .iter()
        .filter(|a| matches!(a.kind, AssetKind::Image))
        .collect();
    let mut colors: Vec<_> = assets
        .iter()
        .filter(|a| matches!(a.kind, AssetKind::Color))
        .collect();
    images.sort_by(|a, b| a.namespaced_name.cmp(&b.namespaced_name));
    colors.sort_by(|a, b| a.namespaced_name.cmp(&b.namespaced_name));

    let mut s = String::new();
    s.push_str("#import <Foundation/Foundation.h>\n\n");
    s.push_str("#if __has_attribute(swift_private)\n");
    s.push_str("#define AC_SWIFT_PRIVATE __attribute__((swift_private))\n");
    s.push_str("#else\n");
    s.push_str("#define AC_SWIFT_PRIVATE\n");
    s.push_str("#endif\n\n");

    if !colors.is_empty() {
        s.push_str("/// The resource bundle ID.\n");
        s.push_str(&format!(
            "static NSString * const ACBundleID AC_SWIFT_PRIVATE = @\"{bundle_identifier}\";\n\n"
        ));
    }

    for c in &colors {
        let ident = objc_identifier(&c.namespaced_name);
        s.push_str(&format!(
            "/// The \"{}\" asset catalog color resource.\n",
            c.namespaced_name
        ));
        s.push_str(&format!(
            "static NSString * const ACColorName{ident} AC_SWIFT_PRIVATE = @\"{}\";\n\n",
            c.namespaced_name
        ));
    }

    for i in &images {
        let ident = objc_identifier(&i.namespaced_name);
        s.push_str(&format!(
            "/// The \"{}\" asset catalog image resource.\n",
            i.namespaced_name
        ));
        s.push_str(&format!(
            "static NSString * const ACImageName{ident} AC_SWIFT_PRIVATE = @\"{}\";\n\n",
            i.namespaced_name
        ));
    }

    s.push_str("#undef AC_SWIFT_PRIVATE\n");

    if let Some(parent) = output_path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(output_path, s)?;
    Ok(())
}

pub fn generate_symbol_index(
    xcassets_path: &Path,
    output_path: &Path,
    platform: &str,
) -> Result<()> {
    let assets = walk_assets(xcassets_path, platform);
    let catalog_abs =
        dunce::canonicalize(xcassets_path).unwrap_or_else(|_| xcassets_path.to_path_buf());
    let catalog_str = catalog_abs.to_string_lossy().into_owned();

    let mut colors: Vec<_> = assets
        .iter()
        .filter(|a| matches!(a.kind, AssetKind::Color))
        .collect();
    let mut images: Vec<_> = assets
        .iter()
        .filter(|a| matches!(a.kind, AssetKind::Image))
        .collect();
    colors.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    images.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));

    let mut out =
        String::with_capacity(512 + (colors.len() + images.len()) * (catalog_str.len() + 256));
    write_plist(&mut out, &catalog_str, &colors, &images);

    if let Some(parent) = output_path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(output_path, out)?;
    Ok(())
}

fn write_plist(out: &mut String, catalog: &str, colors: &[&Asset], images: &[&Asset]) {
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n");
    out.push_str("<plist version=\"1.0\">\n<dict>\n");
    for (key, assets) in [("colors", colors), ("images", images)] {
        out.push_str("\t<key>");
        out.push_str(key);
        out.push_str("</key>\n");
        if assets.is_empty() {
            out.push_str("\t<array/>\n");
            continue;
        }
        out.push_str("\t<array>\n");
        for asset in assets {
            let objc = objc_symbol_name(&asset.kind, &asset.leaf_name);
            let relative = format!("./{}", asset.relative_path);
            let swift = swift_identifier(&asset.leaf_name, asset.kind.as_str());
            out.push_str("\t\t<dict>\n");
            for (k, v) in [
                ("catalogPath", catalog),
                ("objcSymbol", objc.as_str()),
                ("relativePath", relative.as_str()),
                ("swiftSymbol", swift.as_str()),
            ] {
                out.push_str("\t\t\t<key>");
                out.push_str(k);
                out.push_str("</key>\n\t\t\t<string>");
                out.push_str(&escape(v));
                out.push_str("</string>\n");
            }
            out.push_str("\t\t</dict>\n");
        }
        out.push_str("\t</array>\n");
    }
    out.push_str("\t<key>symbols</key>\n\t<array/>\n</dict>\n</plist>");
}

// Matches the plist XML writer, which also escapes quotes.
fn escape(s: &str) -> std::borrow::Cow<'_, str> {
    if !s
        .bytes()
        .any(|b| matches!(b, b'&' | b'<' | b'>' | b'\'' | b'"'))
    {
        return std::borrow::Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len() + 16);
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\'' => out.push_str("&apos;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
    std::borrow::Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn objc_identifier_simple() {
        assert_eq!(objc_identifier("Img001"), "Img001");
        assert_eq!(objc_identifier("TestAccent"), "TestAccent");
    }

    #[test]
    fn objc_identifier_separators() {
        assert_eq!(objc_identifier("my-image"), "MyImage");
        assert_eq!(objc_identifier("test_color"), "TestColor");
        assert_eq!(objc_identifier("foo.bar"), "FooBar");
        assert_eq!(objc_identifier("All Caps"), "AllCaps");
    }

    #[test]
    fn objc_identifier_all_uppercase_word_preserved() {
        assert_eq!(objc_identifier("IMAGE_test"), "IMAGETest");
    }

    #[test]
    fn objc_identifier_digit_letter_boundary() {
        assert_eq!(objc_identifier("123num"), "123Num");
    }

    #[test]
    fn swift_simple() {
        assert_eq!(swift_identifier("Img001", "image"), "img001");
        assert_eq!(swift_identifier("TestAccent", "color"), "testAccent");
    }

    #[test]
    fn swift_strip_trailing_type_suffix() {
        assert_eq!(swift_identifier("foo_image", "image"), "foo");
        assert_eq!(swift_identifier("bar_color", "color"), "bar");
        assert_eq!(swift_identifier("my-image", "image"), "my");
        assert_eq!(swift_identifier("myImage", "image"), "my");
    }

    #[test]
    fn swift_do_not_strip_non_trailing() {
        assert_eq!(swift_identifier("image_foo", "image"), "imageFoo");
    }

    #[test]
    fn swift_leading_acronym_lowercased() {
        assert_eq!(swift_identifier("IMAGE_test", "image"), "imageTest");
    }

    #[test]
    fn swift_digit_prefix_underscored() {
        assert_eq!(swift_identifier("123num", "image"), "_123Num");
    }
}
