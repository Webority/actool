//! SVG rasterization.
//!
//! On macOS it goes through the CoreSVG private framework, calling
//! `CGSVGDocumentCreateFromData` and `CGContextDrawSVGDocument` from
//! `PrivateFrameworks/CoreSVG.framework` as the system `actool` does. All of
//! that FFI lives in the macOS-only [`imp`] module.
//!
//! On every other platform the pure-Rust `resvg` renderer draws the image; it
//! agrees with CoreSVG within a small pixel tolerance (`tests/raster_parity.rs`)
//! but not byte for byte, and renders no text. Callers gate on
//! `has_coresvg()` (xcassets).

#[cfg(not(target_os = "macos"))]
use anyhow::Result;
use regex::Regex;
use std::sync::LazyLock;

static SVG_WIDTH_RE: LazyLock<Result<Regex, regex::Error>> =
    LazyLock::new(|| Regex::new(r#"width="(\d+(?:\.\d+)?)""#));
static SVG_HEIGHT_RE: LazyLock<Result<Regex, regex::Error>> =
    LazyLock::new(|| Regex::new(r#"height="(\d+(?:\.\d+)?)""#));
static SVG_VIEWBOX_RE: LazyLock<Result<Regex, regex::Error>> =
    LazyLock::new(|| Regex::new(r#"viewBox="[\d.]+\s+[\d.]+\s+([\d.]+)\s+([\d.]+)""#));

/// Extract (width, height) from SVG root element attributes. Falls back
/// to the viewBox when width/height aren't specified. Pure parsing — available
/// on every platform.
pub fn parse_svg_dimensions(svg: &[u8]) -> anyhow::Result<(u32, u32)> {
    let slice = &svg[..svg.len().min(2048)];
    let text = String::from_utf8_lossy(slice);

    let w_re = SVG_WIDTH_RE
        .as_ref()
        .map_err(|e| anyhow::anyhow!("invalid built-in pattern: {e}"))?;
    let h_re = SVG_HEIGHT_RE
        .as_ref()
        .map_err(|e| anyhow::anyhow!("invalid built-in pattern: {e}"))?;
    if let (Some(w), Some(h)) = (w_re.captures(&text), h_re.captures(&text)) {
        let wv = parse_dimension(&w[1], "width")?;
        let hv = parse_dimension(&h[1], "height")?;
        return Ok((wv as u32, hv as u32));
    }

    let vb_re = SVG_VIEWBOX_RE
        .as_ref()
        .map_err(|e| anyhow::anyhow!("invalid built-in pattern: {e}"))?;
    if let Some(cap) = vb_re.captures(&text) {
        let wv = parse_dimension(&cap[1], "viewBox width")?;
        let hv = parse_dimension(&cap[2], "viewBox height")?;
        return Ok((wv as u32, hv as u32));
    }
    Ok((0, 0))
}

// `\d` matches Unicode digits that `f64::from_str` rejects, so a bad match is a
// real input error, not a value to default to zero.
fn parse_dimension(text: &str, attr: &str) -> anyhow::Result<f64> {
    text.parse()
        .map_err(|e| anyhow::anyhow!("SVG {attr} \"{text}\" is not a number: {e}"))
}

/// Whether SVG rasterization is available (always `true` off macOS, where the
/// pure-Rust `resvg` renderer is compiled in).
#[cfg(not(target_os = "macos"))]
pub fn has_coresvg() -> bool {
    true
}

/// Rasterize SVG data with `resvg` into a premultiplied `BGRA` pixel buffer of
/// `(width*scale) x (height*scale)`, scaling the document's own size to fit.
/// No fonts are loaded, so SVG text renders nothing: a build machine's
/// installed fonts must not change what an icon compiles to.
#[cfg(not(target_os = "macos"))]
pub fn rasterize_svg(svg_data: &[u8], width: u32, height: u32, scale: u32) -> Result<Vec<u8>> {
    use resvg::tiny_skia::{Pixmap, Transform};
    use resvg::usvg::{Options, Tree};

    let pixel_w = width
        .checked_mul(scale)
        .filter(|w| *w > 0)
        .ok_or_else(|| anyhow::anyhow!("SVG raster width is zero or overflows"))?;
    let pixel_h = height
        .checked_mul(scale)
        .filter(|h| *h > 0)
        .ok_or_else(|| anyhow::anyhow!("SVG raster height is zero or overflows"))?;
    let tree = Tree::from_data(svg_data, &Options::default())
        .map_err(|e| anyhow::anyhow!("SVG parse failed: {e}"))?;
    let size = tree.size();
    let mut pixmap = Pixmap::new(pixel_w, pixel_h)
        .ok_or_else(|| anyhow::anyhow!("SVG raster size {pixel_w}x{pixel_h} is not allocatable"))?;
    let transform = Transform::from_scale(
        pixel_w as f32 / size.width(),
        pixel_h as f32 / size.height(),
    );
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    let mut bgra = pixmap.take();
    for px in bgra.as_chunks_mut::<4>().0 {
        px.swap(0, 2);
    }
    Ok(bgra)
}

#[cfg(target_os = "macos")]
pub use imp::{has_coresvg, rasterize_svg};

#[cfg(target_os = "macos")]
mod imp {
    use super::parse_svg_dimensions;
    use anyhow::{Result, anyhow};
    use std::os::raw::{c_char, c_double, c_void};
    use std::sync::OnceLock;

    unsafe extern "C" {
        fn dlopen(filename: *const c_char, flags: i32) -> *mut c_void;
        fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    }

    const RTLD_LAZY: i32 = 0x1;

    type CFDataCreateFn = unsafe extern "C" fn(*const c_void, *const u8, usize) -> *mut c_void;
    type CFReleaseFn = unsafe extern "C" fn(*mut c_void);

    type CGSVGDocCreateFn = unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void;
    type CGContextDrawSVGFn = unsafe extern "C" fn(*mut c_void, *mut c_void);

    type CGBitmapCreateFn = unsafe extern "C" fn(
        *mut c_void,
        usize,
        usize,
        usize,
        usize,
        *mut c_void,
        u32,
    ) -> *mut c_void;
    type CGColorSpaceCreateFn = unsafe extern "C" fn(*mut c_void) -> *mut c_void;
    type CGContextScaleFn = unsafe extern "C" fn(*mut c_void, c_double, c_double);
    type CGBitmapGetDataFn = unsafe extern "C" fn(*mut c_void) -> *mut u8;
    type CGContextReleaseFn = unsafe extern "C" fn(*mut c_void);

    struct CoreSvgSyms {
        cf_data_create: CFDataCreateFn,
        cf_release: CFReleaseFn,
        svg_create: CGSVGDocCreateFn,
        ctx_draw_svg: CGContextDrawSVGFn,
        bitmap_create: CGBitmapCreateFn,
        cs_create_named: CGColorSpaceCreateFn,
        ctx_scale: CGContextScaleFn,
        bitmap_get_data: CGBitmapGetDataFn,
        ctx_release: CGContextReleaseFn,
        cs_release: CGContextReleaseFn,
        srgb_name: *mut c_void,
    }

    // SAFETY: the fields are function pointers into framework code that stays mapped for the process lifetime, and `srgb_name` is an immutable CFString global, so sharing the table only reads.
    unsafe impl Sync for CoreSvgSyms {}
    // SAFETY: the fields hold no owned or mutable state, so moving the table between threads is sound.
    unsafe impl Send for CoreSvgSyms {}

    fn load_syms() -> Option<&'static CoreSvgSyms> {
        static CELL: OnceLock<Option<CoreSvgSyms>> = OnceLock::new();
        // SAFETY: each handle is null-checked before any `dlsym` call, each symbol is null-checked before it is transmuted, and the `kCGColorSpaceSRGB` address is null-checked before it is dereferenced; the handles are never closed.
        CELL.get_or_init(|| unsafe {
            let cg = dlopen(
                c"/System/Library/Frameworks/CoreGraphics.framework/CoreGraphics".as_ptr(),
                RTLD_LAZY,
            );
            let cf = dlopen(
                c"/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation".as_ptr(),
                RTLD_LAZY,
            );
            let svg = dlopen(
                c"/System/Library/PrivateFrameworks/CoreSVG.framework/CoreSVG".as_ptr(),
                RTLD_LAZY,
            );
            if cg.is_null() || cf.is_null() || svg.is_null() {
                return None;
            }
            macro_rules! sym {
                ($handle:expr_2021, $name:expr_2021, $ty:ty) => {{
                    let p = dlsym($handle, $name.as_ptr());
                    if p.is_null() {
                        return None;
                    }
                    std::mem::transmute::<*mut c_void, $ty>(p)
                }};
            }
            let srgb_name_ptr = {
                let p = dlsym(cg, c"kCGColorSpaceSRGB".as_ptr());
                if p.is_null() {
                    return None;
                }
                *(p as *mut *mut c_void)
            };
            Some(CoreSvgSyms {
                cf_data_create: sym!(cf, c"CFDataCreate", CFDataCreateFn),
                cf_release: sym!(cf, c"CFRelease", CFReleaseFn),
                svg_create: sym!(svg, c"CGSVGDocumentCreateFromData", CGSVGDocCreateFn),
                ctx_draw_svg: sym!(svg, c"CGContextDrawSVGDocument", CGContextDrawSVGFn),
                bitmap_create: sym!(cg, c"CGBitmapContextCreate", CGBitmapCreateFn),
                cs_create_named: sym!(cg, c"CGColorSpaceCreateWithName", CGColorSpaceCreateFn),
                ctx_scale: sym!(cg, c"CGContextScaleCTM", CGContextScaleFn),
                bitmap_get_data: sym!(cg, c"CGBitmapContextGetData", CGBitmapGetDataFn),
                ctx_release: sym!(cg, c"CGContextRelease", CGContextReleaseFn),
                cs_release: sym!(cg, c"CGColorSpaceRelease", CGContextReleaseFn),
                srgb_name: srgb_name_ptr,
            })
        })
        .as_ref()
    }

    pub fn has_coresvg() -> bool {
        load_syms().is_some()
    }

    /// Rasterize SVG data into a `BGRA` (little-endian ARGB premultiplied)
    /// pixel buffer of `(width*scale) x (height*scale)`.
    pub fn rasterize_svg(svg_data: &[u8], width: u32, height: u32, scale: u32) -> Result<Vec<u8>> {
        let syms = load_syms().ok_or_else(|| anyhow!("CoreSVG framework not available"))?;
        let pixel_w = (width * scale) as usize;
        let pixel_h = (height * scale) as usize;
        // Read before any CoreFoundation object exists, so a malformed size returns without leaking them.
        let (nw, nh) = parse_svg_dimensions(svg_data)?;

        // SAFETY: every object is null-checked before use; `cf_data` copies `svg_data` in `CFDataCreate`, so the borrowed slice only needs to live through that call; `ctx` is created with a null data pointer, so CoreGraphics owns a `pixel_w * pixel_h * 4` byte store with `pixel_w * 4` bytes per row, which is the length copied out below.
        unsafe {
            let cf_data =
                (syms.cf_data_create)(std::ptr::null(), svg_data.as_ptr(), svg_data.len());
            if cf_data.is_null() {
                return Err(anyhow!("CFDataCreate failed"));
            }
            let svg_doc = (syms.svg_create)(cf_data, std::ptr::null_mut());
            if svg_doc.is_null() {
                (syms.cf_release)(cf_data);
                return Err(anyhow!("CGSVGDocumentCreateFromData failed"));
            }

            let cs = (syms.cs_create_named)(syms.srgb_name);
            if cs.is_null() {
                (syms.cf_release)(svg_doc);
                (syms.cf_release)(cf_data);
                return Err(anyhow!("CGColorSpaceCreateWithName failed"));
            }

            const K_CG_IMAGE_ALPHA_PREMULTIPLIED_FIRST: u32 = 2;
            const K_CG_BITMAP_BYTE_ORDER_32_LITTLE: u32 = 2 << 12;
            let bitmap_info =
                K_CG_IMAGE_ALPHA_PREMULTIPLIED_FIRST | K_CG_BITMAP_BYTE_ORDER_32_LITTLE;
            let bpr = pixel_w * 4;
            let ctx = (syms.bitmap_create)(
                std::ptr::null_mut(),
                pixel_w,
                pixel_h,
                8,
                bpr,
                cs,
                bitmap_info,
            );
            if ctx.is_null() {
                (syms.cs_release)(cs);
                (syms.cf_release)(svg_doc);
                (syms.cf_release)(cf_data);
                return Err(anyhow!("CGBitmapContextCreate failed"));
            }

            // CGContextDrawSVGDocument draws the SVG at its intrinsic size, so to
            // fill a buffer of a different size we must scale the SVG to fit it
            // (otherwise a 1024-pt SVG asked for at 824 px is clipped to its
            // top-left corner instead of scaled). When the requested width matches
            // the SVG's native size this reduces to the integer `scale` and stays
            // byte-identical to the old behaviour.
            let (nw, nh) = if nw > 0 && nh > 0 {
                (nw, nh)
            } else {
                (width.max(1), height.max(1))
            };
            let sx = pixel_w as c_double / nw as c_double;
            let sy = pixel_h as c_double / nh as c_double;
            if sx != 1.0 || sy != 1.0 {
                (syms.ctx_scale)(ctx, sx, sy);
            }
            (syms.ctx_draw_svg)(ctx, svg_doc);
            let data_ptr = (syms.bitmap_get_data)(ctx);
            if data_ptr.is_null() {
                (syms.ctx_release)(ctx);
                (syms.cs_release)(cs);
                (syms.cf_release)(svg_doc);
                (syms.cf_release)(cf_data);
                return Err(anyhow!("CGBitmapContextGetData returned no buffer"));
            }
            let byte_len = pixel_w * pixel_h * 4;
            let data = std::slice::from_raw_parts(data_ptr, byte_len).to_vec();

            (syms.ctx_release)(ctx);
            (syms.cs_release)(cs);
            (syms.cf_release)(svg_doc);
            (syms.cf_release)(cf_data);

            Ok(data)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_width_height() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="48" viewBox="0 0 64 48"></svg>"#;
        assert_eq!(parse_svg_dimensions(svg).unwrap(), (64, 48));
    }

    #[test]
    fn parse_from_viewbox() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32"></svg>"#;
        assert_eq!(parse_svg_dimensions(svg).unwrap(), (32, 32));
    }

    #[test]
    fn parse_fractional_width() {
        let svg = br#"<svg width="10.5" height="20.75" viewBox="0 0 10 20"></svg>"#;
        assert_eq!(parse_svg_dimensions(svg).unwrap(), (10, 20));
    }

    #[test]
    fn parse_missing() {
        let svg = br#"<svg></svg>"#;
        assert_eq!(parse_svg_dimensions(svg).unwrap(), (0, 0));
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn rasterize_simple_circle() {
        if !has_coresvg() {
            return;
        }
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8" viewBox="0 0 8 8"><rect width="8" height="8" fill="#FF0000"/></svg>"##;
        let data = rasterize_svg(svg, 8, 8, 1).unwrap();
        assert_eq!(data.len(), 8 * 8 * 4);
        // Center pixel should be red (BGRA = 00 00 FF FF premultiplied with alpha=255)
        let center = 4 * 4 + 4 * 4 * 8; // y=4, x=4
        assert_eq!(data[center + 2], 0xFF); // R
        assert_eq!(data[center + 3], 0xFF); // A
    }
}
