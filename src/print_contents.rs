//! `actool --print-contents`: the asset tree of a catalog, in the shape Apple's actool prints.

use crate::catalog::AssetCatalog;
use anyhow::{Context, Result, anyhow};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

pub const TOP_KEY: &str = "com.apple.actool.catalog-contents";

/// The plist subset `--print-contents` emits. Dictionaries are sorted by key, as Apple writes them.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Dict(BTreeMap<String, Value>),
    Array(Vec<Value>),
    String(String),
    Real(u32),
}

/// The contents tree: one array holding the catalog folder, whose children are its assets.
pub fn catalog_contents(catalog: &AssetCatalog) -> Result<Value> {
    if !catalog.path.is_dir() {
        return Err(anyhow!(
            "Asset catalog not found: {}",
            catalog.path.display()
        ));
    }
    Ok(Value::Array(vec![folder_node(&catalog.path)?]))
}

fn folder_node(dir: &Path) -> Result<Value> {
    let mut node = BTreeMap::new();
    node.insert("filename".to_string(), Value::String(file_name(dir)));

    let mut children = Vec::new();
    if let Some(contents) = read_contents(dir)? {
        if let Some(intent) = contents
            .pointer("/properties/template-rendering-intent")
            .and_then(|v| v.as_str())
        {
            node.insert(
                "template-rendering-intent".to_string(),
                Value::String(intent.to_string()),
            );
        }
        children = image_nodes(dir, &contents)?;
    }
    if children.is_empty() {
        children = sub_folder_nodes(dir)?;
    }
    if !children.is_empty() {
        node.insert("children".to_string(), Value::Array(children));
    }
    Ok(Value::Dict(node))
}

fn image_nodes(dir: &Path, contents: &serde_json::Value) -> Result<Vec<Value>> {
    let Some(images) = contents.get("images").and_then(|v| v.as_array()) else {
        return Ok(Vec::new());
    };
    let mut nodes = Vec::with_capacity(images.len());
    for image in images {
        // An appearance variant that names no file has nothing to print.
        let Some(filename) = image.get("filename").and_then(|v| v.as_str()) else {
            continue;
        };
        let path = dir.join(filename);
        let (width, height) = image_size(&path)?;

        let mut node = BTreeMap::new();
        node.insert("filename".to_string(), Value::String(filename.to_string()));
        for key in ["idiom", "platform", "scale", "size"] {
            if let Some(text) = image.get(key).and_then(|v| v.as_str()) {
                node.insert(key.to_string(), Value::String(text.to_string()));
            }
        }
        let mut size = BTreeMap::new();
        size.insert("height".to_string(), Value::Real(height));
        size.insert("width".to_string(), Value::Real(width));
        node.insert("image".to_string(), Value::Dict(size));
        nodes.push(Value::Dict(node));
    }
    Ok(nodes)
}

/// Pixel size of one image, from its own header or, for vector formats, its page or view box.
/// Points are rounded to whole pixels, the size Apple prints.
fn image_size(path: &Path) -> Result<(u32, u32)> {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let whole = |v: f64| {
        let r = v.round();
        (r.is_finite() && (1.0..=u32::MAX as f64).contains(&r)).then(|| r as u32)
    };
    match extension.as_str() {
        "png" => image::image_dimensions(path)
            .with_context(|| format!("reading image {}", path.display())),
        "jpg" | "jpeg" => {
            let bytes =
                std::fs::read(path).with_context(|| format!("reading image {}", path.display()))?;
            match crate::car::jpeg_dimensions(&bytes) {
                (0, 0) => Err(anyhow!("no JPEG size in {}", path.display())),
                size => Ok(size),
            }
        }
        "pdf" => {
            let bytes =
                std::fs::read(path).with_context(|| format!("reading image {}", path.display()))?;
            let media_box = bytes
                .windows(b"/MediaBox".len())
                .position(|w| w == b"/MediaBox")
                .and_then(|at| {
                    bytes[at + b"/MediaBox".len()..]
                        .trim_ascii_start()
                        .strip_prefix(b"[")
                })
                .and_then(|rest| rest.iter().position(|&b| b == b']').map(|end| &rest[..end]))
                .and_then(|body| std::str::from_utf8(body).ok())
                .and_then(|text| {
                    text.split_ascii_whitespace()
                        .map(|s| s.parse::<f64>().ok())
                        .collect::<Option<Vec<f64>>>()
                });
            match media_box.as_deref() {
                Some(&[x0, y0, x1, y1]) => whole(x1 - x0)
                    .zip(whole(y1 - y0))
                    .ok_or_else(|| anyhow!("empty /MediaBox in {}", path.display())),
                _ => Err(anyhow!("no /MediaBox [x0 y0 x1 y1] in {}", path.display())),
            }
        }
        "svg" => {
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("reading image {}", path.display()))?;
            let start = text
                .find("<svg")
                .ok_or_else(|| anyhow!("no <svg> element in {}", path.display()))?;
            let tag = text[start..].split('>').next().unwrap_or_default();
            let attr = |name: &str| -> Option<String> {
                tag.match_indices(name).find_map(|(at, _)| {
                    let bytes = tag.as_bytes();
                    let after = at + name.len();
                    let preceded = at > 0 && bytes[at - 1].is_ascii_whitespace();
                    if !preceded || bytes.get(after) != Some(&b'=') {
                        return None;
                    }
                    let quote = *bytes.get(after + 1)?;
                    if quote != b'"' && quote != b'\'' {
                        return None;
                    }
                    let value = &tag[after + 2..];
                    let end = value.as_bytes().iter().position(|&b| b == quote)?;
                    Some(value[..end].to_owned())
                })
            };
            let plain = |name: &str| {
                attr(name).and_then(|v| {
                    let s = v.trim();
                    s.strip_suffix("px").unwrap_or(s).trim().parse::<f64>().ok()
                })
            };
            let from_attrs = match (plain("width"), plain("height")) {
                (Some(w), Some(h)) => whole(w).zip(whole(h)),
                _ => None,
            };
            let from_view_box = attr("viewBox").and_then(|v| {
                let nums = v
                    .split(|c: char| c.is_ascii_whitespace() || c == ',')
                    .filter(|s| !s.is_empty())
                    .map(|s| s.parse::<f64>().ok())
                    .collect::<Option<Vec<f64>>>();
                match nums.as_deref() {
                    Some([.., w, h]) => whole(*w).zip(whole(*h)),
                    _ => None,
                }
            });
            from_attrs
                .or(from_view_box)
                .ok_or_else(|| anyhow!("no usable width, height or viewBox in {}", path.display()))
        }
        other => Err(anyhow!(
            "unsupported image format .{other} in {}",
            path.display()
        )),
    }
}

fn sub_folder_nodes(dir: &Path) -> Result<Vec<Value>> {
    let mut folders: Vec<(String, std::path::PathBuf)> = Vec::new();
    for entry in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || !entry.file_type()?.is_dir() {
            continue;
        }
        folders.push((name, entry.path()));
    }
    folders.sort_by(|a, b| a.0.cmp(&b.0));
    folders.iter().map(|(_, path)| folder_node(path)).collect()
}

fn read_contents(dir: &Path) -> Result<Option<serde_json::Value>> {
    let path = dir.join("Contents.json");
    match std::fs::read(&path) {
        Ok(bytes) => {
            let value = serde_json::from_slice(&bytes)
                .with_context(|| format!("parsing {}", path.display()))?;
            Ok(Some(value))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The XML plist Apple's actool writes, with the top key holding `value`.
pub fn to_plist_xml(top_key: &str, value: &Value) -> String {
    let mut out = String::new();
    out.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    out.push('\n');
    out.push_str(r#"<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">"#);
    out.push('\n');
    out.push_str(r#"<plist version="1.0">"#);
    out.push('\n');
    out.push_str("<dict>\n\t<key>");
    push_escaped(&mut out, top_key);
    out.push_str("</key>\n");
    write_value(&mut out, value, 1);
    out.push_str("</dict>\n</plist>\n");
    out
}

fn write_value(out: &mut String, value: &Value, depth: usize) {
    let indent = "\t".repeat(depth);
    match value {
        Value::Dict(map) => {
            let _ = writeln!(out, "{indent}<dict>");
            for (key, child) in map {
                let _ = write!(out, "{indent}\t<key>");
                push_escaped(out, key);
                out.push_str("</key>\n");
                write_value(out, child, depth + 1);
            }
            let _ = writeln!(out, "{indent}</dict>");
        }
        Value::Array(items) => {
            let _ = writeln!(out, "{indent}<array>");
            for item in items {
                write_value(out, item, depth + 1);
            }
            let _ = writeln!(out, "{indent}</array>");
        }
        Value::String(text) => {
            let _ = write!(out, "{indent}<string>");
            push_escaped(out, text);
            out.push_str("</string>\n");
        }
        Value::Real(number) => {
            let _ = writeln!(out, "{indent}<real>{number}</real>");
        }
    }
}

fn push_escaped(out: &mut String, text: &str) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
}

/// The same facts as the XML, one asset or image per line, indented by depth.
pub fn to_human_text(value: &Value) -> String {
    let mut out = format!("/* {TOP_KEY} */\n");
    write_human(&mut out, value, 0);
    out
}

fn write_human(out: &mut String, value: &Value, depth: usize) {
    match value {
        Value::Array(items) => {
            for item in items {
                write_human(out, item, depth);
            }
        }
        Value::Dict(map) => {
            out.push_str(&"  ".repeat(depth));
            if let Some(Value::String(name)) = map.get("filename") {
                out.push_str(name);
            }
            for (key, field) in map {
                match (key.as_str(), field) {
                    ("filename" | "children", _) => {}
                    (_, Value::String(text)) => {
                        let _ = write!(out, " {key}={text}");
                    }
                    ("image", Value::Dict(size)) => {
                        if let (Some(Value::Real(w)), Some(Value::Real(h))) =
                            (size.get("width"), size.get("height"))
                        {
                            let _ = write!(out, " image={w}x{h}");
                        }
                    }
                    _ => {}
                }
            }
            out.push('\n');
            if let Some(Value::Array(children)) = map.get("children") {
                for child in children {
                    write_human(out, child, depth + 1);
                }
            }
        }
        Value::String(_) | Value::Real(_) => {}
    }
}
