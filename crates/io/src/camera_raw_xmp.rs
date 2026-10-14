//! Camera Raw Filter › Save Settings… / Load Settings… (#4274): Adobe Camera Raw settings files
//! (`.xmp`), the `crs:` namespace (`http://ns.adobe.com/camera-raw-settings/1.0/`) on an
//! `x:xmpmeta/rdf:RDF/rdf:Description`, as attributes or as child elements.
//!
//! Loading applies only the settings the file holds, over the current ones, as Camera Raw's
//! Load Settings does; settings PhotoCraft has no control for (lens corrections, calibration,
//! geometry, defringe, …) are ignored and named in the report. Saving writes every supported
//! control, so loading the file in Photoshop reproduces the dialog.
//!
//! Camera Raw presets (Presets panel › Import and Export) are the same files with a
//! `crs:Name`, an optional `crs:Group` and `crs:PresetType="Normal"`; a preset holds only some
//! settings. [`LoadReport::fields`] names the PhotoCraft settings a file held and
//! [`save_preset`] writes only the chosen ones.
//!
//! White balance: a Camera Raw Filter on a rendered image uses the relative
//! `IncrementalTemperature` / `IncrementalTint` (−100..=100), which are PhotoCraft's Temperature
//! and Tint. The absolute `Temperature` (kelvin) / `Tint` of raw-file settings have no
//! counterpart and are ignored.

use photocraft_algo::camera_raw::{CameraRaw, validate_curve};

/// The Camera Raw settings namespace.
pub const CRS_NS: &str = "http://ns.adobe.com/camera-raw-settings/1.0/";
const RDF_NS: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
/// Largest settings file read; real ones are a few kilobytes.
pub const MAX_XMP_BYTES: usize = 4 * 1024 * 1024;

type Field = fn(&mut CameraRaw) -> &mut f32;

/// `crs:` scalar name, PhotoCraft field, the slider range values are clamped to, and the
/// PhotoCraft setting (the `CameraRaw` JSON key) the field belongs to.
const SCALARS: &[(&str, Field, f32, f32, &str)] = &[
    ("IncrementalTemperature", |p| &mut p.temperature, -100.0, 100.0, "temperature"),
    ("IncrementalTint", |p| &mut p.tint, -100.0, 100.0, "tint"),
    ("Exposure2012", |p| &mut p.exposure, -5.0, 5.0, "exposure"),
    ("Contrast2012", |p| &mut p.contrast, -100.0, 100.0, "contrast"),
    ("Highlights2012", |p| &mut p.highlights, -100.0, 100.0, "highlights"),
    ("Shadows2012", |p| &mut p.shadows, -100.0, 100.0, "shadows"),
    ("Whites2012", |p| &mut p.whites, -100.0, 100.0, "whites"),
    ("Blacks2012", |p| &mut p.blacks, -100.0, 100.0, "blacks"),
    ("Texture", |p| &mut p.texture, -100.0, 100.0, "texture"),
    ("Clarity2012", |p| &mut p.clarity, -100.0, 100.0, "clarity"),
    ("Dehaze", |p| &mut p.dehaze, -100.0, 100.0, "dehaze"),
    ("Vibrance", |p| &mut p.vibrance, -100.0, 100.0, "vibrance"),
    ("Saturation", |p| &mut p.saturation, -100.0, 100.0, "saturation"),
    ("ParametricShadows", |p| &mut p.curve_shadows, -100.0, 100.0, "curveShadows"),
    ("ParametricDarks", |p| &mut p.curve_darks, -100.0, 100.0, "curveDarks"),
    ("ParametricLights", |p| &mut p.curve_lights, -100.0, 100.0, "curveLights"),
    ("ParametricHighlights", |p| &mut p.curve_highlights, -100.0, 100.0, "curveHighlights"),
    ("ParametricShadowSplit", |p| &mut p.curve_splits[0], 0.0, 100.0, "curveSplits"),
    ("ParametricMidtoneSplit", |p| &mut p.curve_splits[1], 0.0, 100.0, "curveSplits"),
    ("ParametricHighlightSplit", |p| &mut p.curve_splits[2], 0.0, 100.0, "curveSplits"),
    ("HueAdjustmentRed", |p| &mut p.hsl_hue[0], -100.0, 100.0, "hslHue"),
    ("HueAdjustmentOrange", |p| &mut p.hsl_hue[1], -100.0, 100.0, "hslHue"),
    ("HueAdjustmentYellow", |p| &mut p.hsl_hue[2], -100.0, 100.0, "hslHue"),
    ("HueAdjustmentGreen", |p| &mut p.hsl_hue[3], -100.0, 100.0, "hslHue"),
    ("HueAdjustmentAqua", |p| &mut p.hsl_hue[4], -100.0, 100.0, "hslHue"),
    ("HueAdjustmentBlue", |p| &mut p.hsl_hue[5], -100.0, 100.0, "hslHue"),
    ("HueAdjustmentPurple", |p| &mut p.hsl_hue[6], -100.0, 100.0, "hslHue"),
    ("HueAdjustmentMagenta", |p| &mut p.hsl_hue[7], -100.0, 100.0, "hslHue"),
    ("SaturationAdjustmentRed", |p| &mut p.hsl_sat[0], -100.0, 100.0, "hslSat"),
    ("SaturationAdjustmentOrange", |p| &mut p.hsl_sat[1], -100.0, 100.0, "hslSat"),
    ("SaturationAdjustmentYellow", |p| &mut p.hsl_sat[2], -100.0, 100.0, "hslSat"),
    ("SaturationAdjustmentGreen", |p| &mut p.hsl_sat[3], -100.0, 100.0, "hslSat"),
    ("SaturationAdjustmentAqua", |p| &mut p.hsl_sat[4], -100.0, 100.0, "hslSat"),
    ("SaturationAdjustmentBlue", |p| &mut p.hsl_sat[5], -100.0, 100.0, "hslSat"),
    ("SaturationAdjustmentPurple", |p| &mut p.hsl_sat[6], -100.0, 100.0, "hslSat"),
    ("SaturationAdjustmentMagenta", |p| &mut p.hsl_sat[7], -100.0, 100.0, "hslSat"),
    ("LuminanceAdjustmentRed", |p| &mut p.hsl_lum[0], -100.0, 100.0, "hslLum"),
    ("LuminanceAdjustmentOrange", |p| &mut p.hsl_lum[1], -100.0, 100.0, "hslLum"),
    ("LuminanceAdjustmentYellow", |p| &mut p.hsl_lum[2], -100.0, 100.0, "hslLum"),
    ("LuminanceAdjustmentGreen", |p| &mut p.hsl_lum[3], -100.0, 100.0, "hslLum"),
    ("LuminanceAdjustmentAqua", |p| &mut p.hsl_lum[4], -100.0, 100.0, "hslLum"),
    ("LuminanceAdjustmentBlue", |p| &mut p.hsl_lum[5], -100.0, 100.0, "hslLum"),
    ("LuminanceAdjustmentPurple", |p| &mut p.hsl_lum[6], -100.0, 100.0, "hslLum"),
    ("LuminanceAdjustmentMagenta", |p| &mut p.hsl_lum[7], -100.0, 100.0, "hslLum"),
    // Color Grading keeps the split-toning names for the shadow and highlight wheels' hue and
    // saturation, and the balance.
    ("SplitToningShadowHue", |p| &mut p.grade_shadows.hue, 0.0, 360.0, "gradeShadows"),
    ("SplitToningShadowSaturation", |p| &mut p.grade_shadows.sat, 0.0, 100.0, "gradeShadows"),
    ("ColorGradeShadowLum", |p| &mut p.grade_shadows.lum, -100.0, 100.0, "gradeShadows"),
    ("ColorGradeMidtoneHue", |p| &mut p.grade_midtones.hue, 0.0, 360.0, "gradeMidtones"),
    ("ColorGradeMidtoneSat", |p| &mut p.grade_midtones.sat, 0.0, 100.0, "gradeMidtones"),
    ("ColorGradeMidtoneLum", |p| &mut p.grade_midtones.lum, -100.0, 100.0, "gradeMidtones"),
    ("SplitToningHighlightHue", |p| &mut p.grade_highlights.hue, 0.0, 360.0, "gradeHighlights"),
    ("SplitToningHighlightSaturation", |p| &mut p.grade_highlights.sat, 0.0, 100.0, "gradeHighlights"),
    ("ColorGradeHighlightLum", |p| &mut p.grade_highlights.lum, -100.0, 100.0, "gradeHighlights"),
    ("ColorGradeGlobalHue", |p| &mut p.grade_global.hue, 0.0, 360.0, "gradeGlobal"),
    ("ColorGradeGlobalSat", |p| &mut p.grade_global.sat, 0.0, 100.0, "gradeGlobal"),
    ("ColorGradeGlobalLum", |p| &mut p.grade_global.lum, -100.0, 100.0, "gradeGlobal"),
    ("ColorGradeBlending", |p| &mut p.grade_blending, 0.0, 100.0, "gradeBlending"),
    ("SplitToningBalance", |p| &mut p.grade_balance, -100.0, 100.0, "gradeBalance"),
    ("Sharpness", |p| &mut p.sharpen_amount, 0.0, 150.0, "sharpenAmount"),
    ("SharpenRadius", |p| &mut p.sharpen_radius, 0.5, 3.0, "sharpenRadius"),
    ("SharpenDetail", |p| &mut p.sharpen_detail, 0.0, 100.0, "sharpenDetail"),
    ("SharpenEdgeMasking", |p| &mut p.sharpen_masking, 0.0, 100.0, "sharpenMasking"),
    ("LuminanceSmoothing", |p| &mut p.noise_luminance, 0.0, 100.0, "noiseLuminance"),
    ("LuminanceNoiseReductionDetail", |p| &mut p.noise_luminance_detail, 0.0, 100.0, "noiseLuminanceDetail"),
    ("ColorNoiseReduction", |p| &mut p.noise_color, 0.0, 100.0, "noiseColor"),
    ("ColorNoiseReductionDetail", |p| &mut p.noise_color_detail, 0.0, 100.0, "noiseColorDetail"),
    ("GrainAmount", |p| &mut p.grain_amount, 0.0, 100.0, "grainAmount"),
    ("GrainSize", |p| &mut p.grain_size, 0.0, 100.0, "grainSize"),
    ("GrainFrequency", |p| &mut p.grain_roughness, 0.0, 100.0, "grainRoughness"),
    ("PostCropVignetteAmount", |p| &mut p.vignette_amount, -100.0, 100.0, "vignetteAmount"),
    ("PostCropVignetteMidpoint", |p| &mut p.vignette_midpoint, 0.0, 100.0, "vignetteMidpoint"),
    ("PostCropVignetteRoundness", |p| &mut p.vignette_roundness, -100.0, 100.0, "vignetteRoundness"),
    ("PostCropVignetteFeather", |p| &mut p.vignette_feather, 0.0, 100.0, "vignetteFeather"),
    ("PostCropVignetteHighlightContrast", |p| &mut p.vignette_highlights, 0.0, 100.0, "vignetteHighlights"),
];

type Curve = fn(&mut CameraRaw) -> &mut Vec<[f32; 2]>;
const CURVES: &[(&str, Curve, &str)] = &[
    ("ToneCurvePV2012", |p| &mut p.point_curve, "pointCurve"),
    ("ToneCurvePV2012Red", |p| &mut p.point_curve_red, "pointCurveRed"),
    ("ToneCurvePV2012Green", |p| &mut p.point_curve_green, "pointCurveGreen"),
    ("ToneCurvePV2012Blue", |p| &mut p.point_curve_blue, "pointCurveBlue"),
];
const STYLE_KEY: &str = "vignetteStyle";

/// Every PhotoCraft setting a Camera Raw settings file can carry, in file order.
pub fn supported_fields() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = Vec::new();
    for key in SCALARS.iter().map(|s| s.4).chain([STYLE_KEY]).chain(CURVES.iter().map(|c| c.2)) {
        if !v.contains(&key) {
            v.push(key);
        }
    }
    v
}

const STYLES: &[(&str, &str)] = &[("1", "highlightPriority"), ("2", "colorPriority"), ("3", "paintOverlay")];

/// Bookkeeping fields Camera Raw writes that carry no setting.
const METADATA: &[&str] = &[
    "Version",
    "ProcessVersion",
    "HasSettings",
    "HasCrop",
    "WhiteBalance",
    "ToneCurveName2012",
    "CameraProfile",
    "PresetType",
    "UUID",
    "SupportsAmount",
    "SupportsColor",
    "SupportsMonochrome",
    "SupportsHighDynamicRange",
    "SupportsNormalDynamicRange",
    "SupportsSceneReferred",
    "SupportsOutputReferred",
    "CameraModelRestriction",
    "Copyright",
    "ContactInfo",
    "Name",
    "ShortName",
    "SortName",
    "Group",
    "Description",
];

/// What [`load`] applied.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LoadReport {
    /// How many settings were applied.
    pub applied: usize,
    /// `crs:` settings without a PhotoCraft control (set to something other than their neutral
    /// value), or with an unusable value, in file order.
    pub ignored: Vec<String>,
    /// The PhotoCraft settings (`CameraRaw` JSON keys) the file set, each once, in file order.
    pub fields: Vec<&'static str>,
    /// A preset's `crs:Name` and `crs:Group` (the default language), when present.
    pub name: Option<String>,
    pub group: Option<String>,
}

impl LoadReport {
    fn applied(&mut self, key: &'static str) {
        self.applied += 1;
        if !self.fields.contains(&key) {
            self.fields.push(key);
        }
    }
}

/// Longest preset name or group read from a file, in characters.
const MAX_LABEL_CHARS: usize = 255;

/// A `crs:Name` / `crs:Group` value: its `rdf:Alt` item in the default language (else the
/// first), or the element's own text. Trimmed and bounded; `None` when empty.
fn label(node: roxmltree::Node<'_, '_>) -> Option<String> {
    let items: Vec<_> = node.descendants().filter(|n| n.is_element() && n.tag_name().namespace() == Some(RDF_NS) && n.tag_name().name() == "li").collect();
    let text = if items.is_empty() {
        node.text()
    } else {
        let default = items.iter().find(|li| li.attributes().any(|a| a.name() == "lang" && a.value() == "x-default"));
        default.or(items.first()).and_then(|li| li.text())
    };
    clean_label(text?)
}

fn clean_label(text: &str) -> Option<String> {
    let text: String = text.trim().chars().filter(|c| !c.is_control()).take(MAX_LABEL_CHARS).collect();
    (!text.is_empty()).then_some(text)
}

fn number(s: &str) -> Option<f32> {
    let s = s.trim();
    let v: f32 = s.strip_prefix('+').unwrap_or(s).parse().ok()?;
    v.is_finite().then_some(v)
}

/// `"0, 0"` items of an `rdf:Seq` → curve points.
fn curve(node: roxmltree::Node<'_, '_>) -> Option<Vec<[f32; 2]>> {
    let seq = node.children().find(|c| c.is_element() && c.tag_name().namespace() == Some(RDF_NS) && c.tag_name().name() == "Seq")?;
    let mut points = Vec::new();
    for li in seq.children().filter(|c| c.is_element() && c.tag_name().name() == "li") {
        let text = li.text().unwrap_or("");
        let (x, y) = text.split_once(',')?;
        points.push([number(x)?, number(y)?]);
    }
    validate_curve(&points).ok()?;
    // Camera Raw's linear curve is the identity's two end points; PhotoCraft's is empty.
    if points == [[0.0, 0.0], [255.0, 255.0]] {
        points.clear();
    }
    Some(points)
}

/// An unsupported setting at its neutral value changes nothing, so it isn't reported: zero,
/// `False`, and the few neutral values that aren't zero.
fn neutral(name: &str, value: &str) -> bool {
    let v = value.trim();
    if v.eq_ignore_ascii_case("false") || number(v) == Some(0.0) {
        return true;
    }
    match name {
        "GrainSeed" => true,
        "VignetteMidpoint" => number(v) == Some(50.0),
        "PerspectiveScale" => number(v) == Some(100.0),
        _ => name.starts_with("Defringe") && name.contains("Hue"),
    }
}

fn apply(params: &mut CameraRaw, report: &mut LoadReport, name: &str, value: &str) {
    if let Some((_, field, lo, hi, key)) = SCALARS.iter().find(|(n, ..)| *n == name) {
        match number(value) {
            Some(v) => {
                *field(params) = v.clamp(*lo, *hi);
                report.applied(key);
            }
            None => report.ignored.push(name.to_string()),
        }
    } else if name == "PostCropVignetteStyle" {
        match STYLES.iter().find(|(n, _)| *n == value.trim()) {
            Some((_, style)) => {
                params.vignette_style = (*style).into();
                report.applied(STYLE_KEY);
            }
            None => report.ignored.push(name.to_string()),
        }
    } else if name == "Name" || name == "Group" {
        let slot = if name == "Name" { &mut report.name } else { &mut report.group };
        if slot.is_none() {
            *slot = clean_label(value);
        }
    } else if !METADATA.contains(&name) && !neutral(name, value) {
        report.ignored.push(name.to_string());
    }
}

/// Apply a Camera Raw settings file over `params`. Fails on input that isn't XMP or holds no
/// Camera Raw settings; never panics on malformed input.
pub fn load(params: &mut CameraRaw, bytes: &[u8]) -> Result<LoadReport, String> {
    if bytes.len() > MAX_XMP_BYTES {
        return Err("the settings file is too large".into());
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "not a Camera Raw settings file (not UTF-8 text)".to_string())?;
    // Some writers put a byte order mark or an `<?xpacket?>` wrapper first; roxmltree takes both.
    let text = text.trim_start_matches('\u{feff}');
    let doc = roxmltree::Document::parse(text).map_err(|e| format!("not a Camera Raw settings file ({e})"))?;
    let mut next = params.clone();
    let mut report = LoadReport::default();
    let mut found = false;
    for desc in doc.descendants().filter(|n| n.is_element() && n.tag_name().namespace() == Some(RDF_NS) && n.tag_name().name() == "Description") {
        for a in desc.attributes().filter(|a| a.namespace() == Some(CRS_NS)) {
            found = true;
            apply(&mut next, &mut report, a.name(), a.value());
        }
        for c in desc.children().filter(|c| c.is_element() && c.tag_name().namespace() == Some(CRS_NS)) {
            found = true;
            let name = c.tag_name().name();
            if let Some((_, field, key)) = CURVES.iter().find(|(n, ..)| *n == name) {
                match curve(c) {
                    Some(points) => {
                        *field(&mut next) = points;
                        report.applied(key);
                    }
                    None => report.ignored.push(name.to_string()),
                }
            } else if name == "Name" || name == "Group" {
                let slot = if name == "Name" { &mut report.name } else { &mut report.group };
                if slot.is_none() {
                    *slot = label(c);
                }
            } else if c.children().any(|g| g.is_element()) {
                // Structured settings (masks, looks, lens profiles…) have no counterpart.
                if !METADATA.contains(&name) {
                    report.ignored.push(name.to_string());
                }
            } else {
                apply(&mut next, &mut report, name, c.text().unwrap_or(""));
            }
        }
    }
    if !found {
        return Err("no Camera Raw settings in the file".into());
    }
    *params = next;
    Ok(report)
}

fn signed(v: f32, decimals: usize) -> String {
    let s = format!("{v:.decimals$}");
    if v > 0.0 && !s.starts_with('+') { format!("+{s}") } else { s }
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Write `params` as a Camera Raw settings file Photoshop's Load Settings… reads.
pub fn save(params: &CameraRaw) -> String {
    write(params, |_| true, None)
}

/// Write a Camera Raw preset holding only the settings named in `fields` (`CameraRaw` JSON keys,
/// see [`supported_fields`]; others are skipped), with its name and optional group, as Camera
/// Raw's Presets panel exports one.
pub fn save_preset(params: &CameraRaw, fields: &[&str], name: &str, group: Option<&str>) -> String {
    write(params, |key| fields.contains(&key), Some((name, group)))
}

/// A stable `crs:UUID` for a preset (32 upper-case hex digits), from its name and settings.
fn preset_uuid(name: &str, body: &str) -> String {
    let fnv = |seed: u64| name.bytes().chain([0]).chain(body.bytes()).fold(seed, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3));
    format!("{:016X}{:016X}", fnv(0xcbf2_9ce4_8422_2325), fnv(0x6c62_272e_07bb_0142))
}

fn alt(tag: &str, text: &str) -> String {
    format!("   <crs:{tag}>\n    <rdf:Alt>\n     <rdf:li xml:lang=\"x-default\">{}</rdf:li>\n    </rdf:Alt>\n   </crs:{tag}>\n", escape(text))
}

fn write(params: &CameraRaw, keep: impl Fn(&str) -> bool, preset: Option<(&str, Option<&str>)>) -> String {
    let mut p = params.clone();
    let wb = if p.temperature != 0.0 || p.tint != 0.0 { "Custom" } else { "As Shot" };
    let mut out = String::from(
        "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\" x:xmptk=\"PhotoCraft\">\n <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n  <rdf:Description rdf:about=\"\"\n    xmlns:crs=\"http://ns.adobe.com/camera-raw-settings/1.0/\"\n",
    );
    let mut settings = String::new();
    if keep("temperature") || keep("tint") {
        settings.push_str(&format!("   crs:WhiteBalance=\"{wb}\"\n"));
    }
    for (name, field, lo, _, key) in SCALARS {
        if !keep(key) {
            continue;
        }
        let v = *field(&mut p);
        // Exposure has two decimals in Camera Raw, the sharpening radius one; the rest are levels.
        let text = match *name {
            "Exposure2012" => signed(v, 2),
            "SharpenRadius" => signed(v, 1),
            _ if *lo < 0.0 => signed(v.round(), 0),
            _ => format!("{:.0}", v.round()),
        };
        settings.push_str(&format!("   crs:{name}=\"{text}\"\n"));
    }
    if keep(STYLE_KEY) {
        let style = STYLES.iter().find(|(_, s)| *s == p.vignette_style).map_or("1", |(n, _)| n);
        settings.push_str(&format!("   crs:PostCropVignetteStyle=\"{}\"\n", escape(style)));
    }
    if keep("pointCurve") {
        settings.push_str("   crs:ToneCurveName2012=\"Custom\"\n");
    }
    out.push_str("   crs:Version=\"18.4\"\n   crs:ProcessVersion=\"15.4\"\n");
    if let Some((name, _)) = preset {
        out.push_str(&format!("   crs:PresetType=\"Normal\"\n   crs:UUID=\"{}\"\n   crs:SupportsAmount=\"False\"\n", preset_uuid(name, &settings)));
    }
    out.push_str(&settings);
    out.push_str("   crs:HasSettings=\"True\">\n");
    if let Some((name, group)) = preset {
        out.push_str(&alt("Name", name));
        if let Some(group) = group {
            out.push_str(&alt("Group", group));
        }
    }
    for (name, field, key) in CURVES {
        if !keep(key) {
            continue;
        }
        let points = field(&mut p);
        let points = if points.is_empty() { vec![[0.0, 0.0], [255.0, 255.0]] } else { points.clone() };
        out.push_str(&format!("   <crs:{name}>\n    <rdf:Seq>\n"));
        for [x, y] in points {
            out.push_str(&format!("     <rdf:li>{}, {}</rdf:li>\n", x.round(), y.round()));
        }
        out.push_str(&format!("    </rdf:Seq>\n   </crs:{name}>\n"));
    }
    out.push_str("  </rdf:Description>\n </rdf:RDF>\n</x:xmpmeta>\n");
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// A hand-written settings file laid out as Photoshop 26's Camera Raw Filter saves one (the
    /// field set and order follow Camera Raw 18; the values are synthetic), plus element-form
    /// settings, which XMP allows and some writers use.
    const FIXTURE: &[u8] = br#"<x:xmpmeta xmlns:x="adobe:ns:meta/" x:xmptk="Adobe XMP Core 7.0-c000 1.000000, 0000/00/00-00:00:00        ">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about=""
    xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/"
   crs:Version="18.1.1" crs:ProcessVersion="15.4" crs:WhiteBalance="Custom"
   crs:IncrementalTemperature="0" crs:IncrementalTint="+10" crs:Exposure2012="+0.05"
   crs:Contrast2012="-5" crs:Highlights2012="-20" crs:Shadows2012="+10" crs:Whites2012="+35"
   crs:Blacks2012="0" crs:Texture="+30" crs:Clarity2012="+5" crs:Vibrance="+20" crs:Saturation="-5"
   crs:ParametricShadowSplit="25" crs:ParametricMidtoneSplit="50" crs:ParametricHighlightSplit="75"
   crs:Sharpness="15" crs:SharpenRadius="+1.0" crs:LuminanceSmoothing="5"
   crs:LuminanceNoiseReductionContrast="0" crs:HueAdjustmentGreen="+25" crs:SaturationAdjustmentGreen="+20"
   crs:ColorGradeBlending="50" crs:LensProfileEnable="0" crs:VignetteAmount="-20"
   crs:PerspectiveScale="100" crs:GrainAmount="10" crs:RedHue="-5" crs:GrainSeed="2834497962"
   crs:ToneCurveName2012="Linear" crs:CameraProfile="Default Color" crs:HasSettings="True" crs:HasCrop="False">
   <crs:Dehaze>+12</crs:Dehaze>
   <crs:ToneCurvePV2012>
    <rdf:Seq>
     <rdf:li>0, 0</rdf:li>
     <rdf:li>255, 255</rdf:li>
    </rdf:Seq>
   </crs:ToneCurvePV2012>
   <crs:ToneCurvePV2012Red>
    <rdf:Seq>
     <rdf:li>0, 0</rdf:li>
     <rdf:li>128, 140</rdf:li>
     <rdf:li>255, 255</rdf:li>
    </rdf:Seq>
   </crs:ToneCurvePV2012Red>
  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta>"#;

    #[test]
    fn reads_a_photoshop_preset() {
        let mut p = CameraRaw::default();
        let r = load(&mut p, FIXTURE).unwrap();
        assert_eq!(p.tint, 10.0);
        assert_eq!(p.exposure, 0.05);
        assert_eq!((p.contrast, p.highlights, p.shadows, p.whites), (-5.0, -20.0, 10.0, 35.0));
        assert_eq!((p.texture, p.clarity, p.vibrance, p.saturation), (30.0, 5.0, 20.0, -5.0));
        assert_eq!((p.hsl_hue[3], p.hsl_sat[3]), (25.0, 20.0));
        assert_eq!((p.sharpen_amount, p.noise_luminance, p.grain_amount), (15.0, 5.0, 10.0));
        assert_eq!(p.grade_blending, 50.0);
        // The linear tone curve is PhotoCraft's empty one; an element-form setting is read too.
        assert!(p.point_curve.is_empty());
        assert_eq!(p.point_curve_red, vec![[0.0, 0.0], [128.0, 140.0], [255.0, 255.0]]);
        assert_eq!(p.dehaze, 12.0);
        // Lens vignetting, calibration and geometry have no counterpart.
        assert_eq!(r.ignored, vec!["VignetteAmount".to_string(), "RedHue".to_string()]);
    }

    #[test]
    fn save_then_load_round_trips() {
        let mut p = CameraRaw {
            temperature: -12.0,
            exposure: 1.25,
            contrast: 30.0,
            sharpen_radius: 1.4,
            vignette_amount: -40.0,
            vignette_style: "paintOverlay".into(),
            curve_splits: [20.0, 45.0, 80.0],
            point_curve: vec![[0.0, 10.0], [128.0, 150.0], [255.0, 240.0]],
            ..CameraRaw::default()
        };
        p.hsl_lum[5] = -33.0;
        p.grade_shadows.hue = 210.0;
        p.grade_shadows.sat = 40.0;
        p.grade_global.lum = 7.0;
        let text = save(&p);
        assert!(text.contains("crs:Exposure2012=\"+1.25\""));
        assert!(text.contains("crs:WhiteBalance=\"Custom\""));
        let mut back = CameraRaw::default();
        let r = load(&mut back, text.as_bytes()).unwrap();
        assert!(r.ignored.is_empty(), "{:?}", r.ignored);
        assert_eq!(back, p);
    }

    #[test]
    fn load_applies_only_the_settings_in_the_file() {
        let mut p = CameraRaw { contrast: 40.0, ..CameraRaw::default() };
        let xmp = br#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
            <rdf:Description xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/"><crs:Exposure2012>-9</crs:Exposure2012>
            <crs:Clarity2012>abc</crs:Clarity2012></rdf:Description></rdf:RDF></x:xmpmeta>"#;
        let r = load(&mut p, xmp).unwrap();
        assert_eq!((p.exposure, p.contrast, p.clarity), (-5.0, 40.0, 0.0));
        assert_eq!(r, LoadReport { applied: 1, ignored: vec!["Clarity2012".into()], fields: vec!["exposure"], ..Default::default() });
    }

    #[test]
    fn malformed_input_is_an_error_and_leaves_settings_alone() {
        let before = CameraRaw { exposure: 0.5, ..CameraRaw::default() };
        for bad in [
            &b""[..],
            b"\xff\xfe\x00garbage",
            b"<x:xmpmeta><unclosed",
            b"<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"/>",
            b"<?xml version=\"1.0\"?><!DOCTYPE a [<!ENTITY e \"x\">]><a>&e;</a>",
        ] {
            let mut p = before.clone();
            assert!(load(&mut p, bad).is_err());
            assert_eq!(p, before);
        }
        // A broken curve or NaN is skipped, not fatal.
        let xmp = br#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description
            xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/" crs:Contrast2012="NaN"><crs:ToneCurvePV2012><rdf:Seq>
            <rdf:li>10</rdf:li></rdf:Seq></crs:ToneCurvePV2012></rdf:Description></rdf:RDF>"#;
        let mut p = before.clone();
        let r = load(&mut p, xmp).unwrap();
        assert_eq!(p, before);
        assert_eq!(r.ignored, vec!["Contrast2012".to_string(), "ToneCurvePV2012".to_string()]);
    }

    #[test]
    fn supported_fields_are_camera_raw_settings() {
        let keys = serde_json::to_value(CameraRaw::default()).unwrap();
        let fields = supported_fields();
        for f in &fields {
            assert!(keys.get(f).is_some(), "{f}");
        }
        // Every user setting is carried; only the preview bookkeeping isn't.
        let missing: Vec<_> = keys.as_object().unwrap().keys().filter(|k| !fields.contains(&k.as_str())).cloned().collect();
        assert_eq!(missing, ["encodingGamma", "pixelScale", "seed"]);
    }

    #[test]
    fn a_preset_round_trips_only_its_settings_with_name_and_group() {
        let p =
            CameraRaw { exposure: -0.5, contrast: 25.0, clarity: 40.0, point_curve: vec![[0.0, 0.0], [100.0, 140.0], [255.0, 255.0]], ..CameraRaw::default() };
        let text = save_preset(&p, &["exposure", "pointCurve", "hslHue"], "Warm & <bright>", Some("Mine"));
        assert!(text.contains("crs:PresetType=\"Normal\"") && text.contains("Warm &amp; &lt;bright&gt;"), "{text}");
        assert!(!text.contains("Contrast2012") && !text.contains("Clarity2012") && !text.contains("WhiteBalance"), "{text}");
        let mut back = CameraRaw { contrast: -10.0, ..CameraRaw::default() };
        let r = load(&mut back, text.as_bytes()).unwrap();
        assert!(r.ignored.is_empty(), "{:?}", r.ignored);
        assert_eq!(r.fields, ["exposure", "hslHue", "pointCurve"]);
        assert_eq!((r.name.as_deref(), r.group.as_deref()), (Some("Warm & <bright>"), Some("Mine")));
        assert_eq!((back.exposure, back.contrast, back.clarity), (-0.5, -10.0, 0.0));
        assert_eq!(back.point_curve, p.point_curve);
        // The same preset always gets the same UUID; another name gets another.
        assert_eq!(text, save_preset(&p, &["exposure", "pointCurve", "hslHue"], "Warm & <bright>", Some("Mine")));
        assert_ne!(preset_uuid("a", ""), preset_uuid("b", ""));
        // A settings file is not a preset: no name.
        let r = load(&mut back, save(&p).as_bytes()).unwrap();
        assert_eq!((r.name, r.group), (None, None));
        assert_eq!(r.fields.len(), supported_fields().len());
    }

    #[test]
    fn preset_names_are_read_from_attributes_and_alternatives_and_bounded() {
        let long = "n".repeat(400);
        let xmp = format!(
            r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description
            xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/" crs:Group="  Attr group " crs:Exposure2012="+1">
            <crs:Name><rdf:Alt><rdf:li xml:lang="de">Warm</rdf:li><rdf:li xml:lang="x-default">{long}</rdf:li></rdf:Alt></crs:Name>
            </rdf:Description></rdf:RDF>"#
        );
        let r = load(&mut CameraRaw::default(), xmp.as_bytes()).unwrap();
        assert_eq!(r.name.unwrap().chars().count(), MAX_LABEL_CHARS);
        assert_eq!(r.group.as_deref(), Some("Attr group"));
        let xmp = br#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description
            xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/" crs:Name=" &#10; "><crs:Group><rdf:Alt><rdf:li>Only</rdf:li></rdf:Alt></crs:Group>
            </rdf:Description></rdf:RDF>"#;
        let r = load(&mut CameraRaw::default(), xmp).unwrap();
        assert_eq!((r.name, r.group.as_deref(), r.applied), (None, Some("Only"), 0));
    }
}
