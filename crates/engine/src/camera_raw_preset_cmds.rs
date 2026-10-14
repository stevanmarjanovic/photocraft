//! Camera Raw › Presets panel: named Camera Raw settings in groups, like Adobe Camera Raw's
//! Presets. A preset holds only the settings chosen when it was created (Create Preset's
//! checkboxes, by panel section); applying it sets those over the dialog's current settings and
//! leaves the rest alone. Presets can be favourites, renamed, moved between groups, updated with
//! the current settings, and exchanged with Photoshop as Camera Raw preset files (`.xmp`, see
//! [`photocraft_io::camera_raw_xmp`]).
//!
//! The engine owns the list, so the dialog, the CLI and MCP see the same presets through the
//! `cameraRaw.presets.*` commands. They persist with the preferences document (`presets`
//! › `camera_raw`); a damaged entry is dropped on load, the rest kept. The dialog applies a
//! preset itself ([`apply`]), since its settings are dialog state until OK.

use photocraft_algo::camera_raw::CameraRaw;
use photocraft_io::camera_raw_xmp;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::commands::CommandSpec;
use crate::presets::{Group, Named, always, bad, edit_groups, find, req_str, str_param, unique_name};
use crate::{Result, Session};

/// The group new presets go to when none is named, as in Camera Raw.
pub const USER_GROUP: &str = "User Presets";
/// Most presets kept (all groups together).
pub const MAX_PRESETS: usize = 2000;
/// Most groups kept.
pub const MAX_GROUPS: usize = 200;
/// Largest absolute value a stored setting may hold; every slider is far inside it.
const MAX_MAGNITUDE: f64 = 1.0e6;

/// The dialog's panel sections and the settings each covers: Create Preset's checkboxes.
pub const SECTIONS: &[(&str, &[&str])] = &[
    ("Light", &["exposure", "contrast", "highlights", "shadows", "whites", "blacks"]),
    ("Color", &["temperature", "tint", "vibrance", "saturation"]),
    (
        "Effects",
        &[
            "texture",
            "clarity",
            "dehaze",
            "vignetteStyle",
            "vignetteAmount",
            "vignetteMidpoint",
            "vignetteRoundness",
            "vignetteFeather",
            "vignetteHighlights",
            "grainAmount",
            "grainSize",
            "grainRoughness",
        ],
    ),
    (
        "Curve",
        &["curveHighlights", "curveLights", "curveDarks", "curveShadows", "curveSplits", "pointCurve", "pointCurveRed", "pointCurveGreen", "pointCurveBlue"],
    ),
    ("Color Mixer", &["hslHue", "hslSat", "hslLum"]),
    ("Color Grading", &["gradeShadows", "gradeMidtones", "gradeHighlights", "gradeGlobal", "gradeBlending", "gradeBalance"]),
    (
        "Detail",
        &["sharpenAmount", "sharpenRadius", "sharpenDetail", "sharpenMasking", "noiseLuminance", "noiseLuminanceDetail", "noiseColor", "noiseColorDetail"],
    ),
];

/// A named subset of Camera Raw settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CameraRawPreset {
    pub name: String,
    #[serde(default)]
    pub favorite: bool,
    /// `CameraRaw` JSON keys → values; only these are applied.
    pub settings: Map<String, Value>,
}

impl Named for CameraRawPreset {
    fn name(&self) -> &str {
        &self.name
    }
    fn set_name(&mut self, n: String) {
        self.name = n;
    }
}

impl CameraRawPreset {
    /// The settings over the defaults, for exporting or describing the preset.
    pub fn full_settings(&self) -> Result<CameraRaw> {
        apply(&CameraRaw::default(), &self.settings).map_err(|e| bad("cameraRaw.presets", e))
    }
}

/// The groups a fresh install starts with: one empty "User Presets".
pub fn builtin() -> Vec<Group<CameraRawPreset>> {
    vec![Group::new(USER_GROUP, Vec::new())]
}

fn bounded(v: &Value) -> bool {
    match v {
        Value::Number(n) => n.as_f64().is_some_and(|x| x.is_finite() && x.abs() <= MAX_MAGNITUDE),
        Value::Array(a) => a.iter().all(bounded),
        Value::Object(o) => o.values().all(bounded),
        // The one string setting is the vignette style's short name.
        Value::String(s) => s.len() <= 64,
        Value::Bool(_) | Value::Null => true,
    }
}

/// `settings` over `current`: the preset's settings replace the current ones, the rest stay.
/// Fails (changing nothing) on an unknown setting, a value of the wrong type, an out-of-range
/// magnitude or a malformed curve.
pub fn apply(current: &CameraRaw, settings: &Map<String, Value>) -> std::result::Result<CameraRaw, String> {
    let supported = camera_raw_xmp::supported_fields();
    let mut merged = serde_json::to_value(current).map_err(|e| e.to_string())?;
    let Value::Object(fields) = &mut merged else { return Err("Camera Raw settings are not an object".into()) };
    for (key, value) in settings {
        if !supported.contains(&key.as_str()) {
            return Err(format!("unknown Camera Raw setting {key}"));
        }
        if !bounded(value) {
            return Err(format!("{key}: values must be finite numbers within ±{MAX_MAGNITUDE}"));
        }
        fields.insert(key.clone(), value.clone());
    }
    let next: CameraRaw = serde_json::from_value(merged).map_err(|e| format!("bad Camera Raw settings: {e}"))?;
    next.validate_changes(current)?;
    Ok(next)
}

/// `params` keeps only `fields`, in canonical form (as `CameraRaw` serializes them).
pub fn subset(params: &CameraRaw, fields: &[&str]) -> Map<String, Value> {
    let Ok(Value::Object(all)) = serde_json::to_value(params) else { return Map::new() };
    all.into_iter().filter(|(k, _)| fields.contains(&k.as_str())).collect()
}

/// Validated, canonical settings from command params.
fn checked_settings(v: Option<&Value>, cmd: &str) -> Result<Map<String, Value>> {
    let Some(Value::Object(settings)) = v else { return Err(bad(cmd, "`settings` must be an object of Camera Raw settings")) };
    if settings.is_empty() {
        return Err(bad(cmd, "a preset needs at least one setting"));
    }
    let full = apply(&CameraRaw::default(), settings).map_err(|e| bad(cmd, e))?;
    full.validate().map_err(|e| bad(cmd, e))?;
    let keys: Vec<&str> = settings.keys().map(String::as_str).collect();
    Ok(subset(&full, &keys))
}

fn checked_name(name: &str, cmd: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > crate::brush_preset_cmds::MAX_NAME || name.chars().any(char::is_control) {
        return Err(bad(cmd, "names must have 1–255 characters and no control characters"));
    }
    Ok(name.into())
}

fn count(groups: &[Group<CameraRawPreset>]) -> usize {
    groups.iter().map(|g| g.items.len()).sum()
}

/// Index of the named group, created at the end when missing (bounded by [`MAX_GROUPS`]); the
/// first group (or a new "User Presets") when none is named.
fn target_group(groups: &mut Vec<Group<CameraRawPreset>>, group: Option<&str>, cmd: &str) -> Result<usize> {
    let name = match group {
        Some(g) => checked_name(g, cmd)?,
        None if !groups.is_empty() => return Ok(0),
        None => USER_GROUP.to_string(),
    };
    if let Some(i) = groups.iter().position(|g| g.name == name) {
        return Ok(i);
    }
    if groups.len() >= MAX_GROUPS {
        return Err(bad(cmd, format!("at most {MAX_GROUPS} preset groups")));
    }
    groups.push(Group::new(&name, Vec::new()));
    Ok(groups.len() - 1)
}

/// Read stored groups entry by entry: a damaged preset or group is dropped, the rest kept.
pub(crate) fn load(value: Value) -> Vec<Group<CameraRawPreset>> {
    let Some(entries) = value.as_array() else { return builtin() };
    let mut groups: Vec<Group<CameraRawPreset>> = Vec::new();
    let mut total = 0;
    for entry in entries.iter().take(MAX_GROUPS) {
        let Some(name) = entry.get("name").and_then(Value::as_str).and_then(|n| checked_name(n, "load").ok()) else { continue };
        if groups.iter().any(|g| g.name == name) {
            continue;
        }
        let mut group = Group::new(&name, Vec::new());
        for item in entry.get("items").and_then(Value::as_array).into_iter().flatten() {
            if total >= MAX_PRESETS {
                break;
            }
            let Ok(mut preset) = serde_json::from_value::<CameraRawPreset>(item.clone()) else { continue };
            let Ok(name) = checked_name(&preset.name, "load") else { continue };
            let Ok(settings) = checked_settings(Some(&Value::Object(preset.settings)), "load") else { continue };
            if groups.iter().chain([&group]).any(|g| g.items.iter().any(|p| p.name == name)) {
                continue;
            }
            preset.name = name;
            preset.settings = settings;
            group.items.push(preset);
            total += 1;
        }
        groups.push(group);
    }
    groups
}

fn describe(p: &CameraRawPreset, group: &str) -> Value {
    json!({"name": p.name, "group": group, "favorite": p.favorite, "settings": p.settings})
}

fn list(s: &mut Session, _: &Value) -> Result<Value> {
    let groups: Vec<Value> =
        s.presets.camera_raw.iter().map(|g| json!({"name": g.name, "presets": g.items.iter().map(|p| describe(p, &g.name)).collect::<Vec<_>>()})).collect();
    let favorites: Vec<Value> =
        s.presets.camera_raw.iter().flat_map(|g| g.items.iter().filter(|p| p.favorite).map(|p| json!({"name": p.name, "group": g.name}))).collect();
    let sections: Vec<Value> = SECTIONS.iter().map(|(name, fields)| json!({"name": name, "fields": fields})).collect();
    Ok(json!({"groups": groups, "favorites": favorites, "sections": sections}))
}

fn lookup<'a>(s: &'a Session, p: &Value, cmd: &str) -> Result<(&'a Group<CameraRawPreset>, &'a CameraRawPreset)> {
    let name = req_str(p, "preset", cmd)?;
    let groups = &s.presets.camera_raw;
    let (gi, ii) = find(groups, name, str_param(p, "group")).ok_or_else(|| bad(cmd, format!("no Camera Raw preset \"{name}\"")))?;
    let group = groups.get(gi).ok_or_else(|| bad(cmd, "preset group vanished"))?;
    let preset = group.items.get(ii).ok_or_else(|| bad(cmd, "preset vanished"))?;
    Ok((group, preset))
}

fn get(s: &mut Session, p: &Value) -> Result<Value> {
    let (group, preset) = lookup(s, p, "cameraRaw.presets.get")?;
    Ok(json!({"preset": describe(preset, &group.name)}))
}

fn save(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "cameraRaw.presets.save";
    let name = checked_name(req_str(p, "name", CMD)?, CMD)?;
    let settings = checked_settings(p.get("settings"), CMD)?;
    let replace = match p.get("replace") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(_) => return Err(bad(CMD, "`replace` must be a boolean")),
    };
    let groups = &mut s.presets.camera_raw;
    if let Some((gi, ii)) = find(groups, &name, None) {
        if !replace {
            return Err(bad(CMD, format!("a Camera Raw preset named \"{name}\" already exists")));
        }
        let group = groups.get_mut(gi).ok_or_else(|| bad(CMD, "preset group vanished"))?;
        let preset = group.items.get_mut(ii).ok_or_else(|| bad(CMD, "preset vanished"))?;
        preset.settings = settings;
        let result = json!({"preset": describe(preset, &group.name), "replaced": true});
        s.presets_changed();
        return Ok(result);
    }
    if replace {
        return Err(bad(CMD, format!("no Camera Raw preset \"{name}\" to update")));
    }
    if count(groups) >= MAX_PRESETS {
        return Err(bad(CMD, format!("at most {MAX_PRESETS} Camera Raw presets can be saved")));
    }
    let gi = target_group(groups, str_param(p, "group"), CMD)?;
    let group = groups.get_mut(gi).ok_or_else(|| bad(CMD, "preset group vanished"))?;
    let preset = CameraRawPreset { name, favorite: false, settings };
    let result = json!({"preset": describe(&preset, &group.name), "replaced": false});
    group.items.push(preset);
    s.presets_changed();
    Ok(result)
}

fn edit(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "cameraRaw.presets.edit";
    let action = req_str(p, "action", CMD)?;
    let groups = &mut s.presets.camera_raw;
    let result = match action {
        "favorite" => {
            let name = req_str(p, "preset", CMD)?;
            let on = p.get("favorite").and_then(Value::as_bool).ok_or_else(|| bad(CMD, "`favorite` must be a boolean"))?;
            let (gi, ii) = find(groups, name, str_param(p, "group")).ok_or_else(|| bad(CMD, format!("no Camera Raw preset \"{name}\"")))?;
            let preset = groups.get_mut(gi).and_then(|g| g.items.get_mut(ii)).ok_or_else(|| bad(CMD, "preset vanished"))?;
            preset.favorite = on;
            json!({"name": name, "favorite": on})
        }
        "rename" | "renameGroup" | "newGroup" | "move" => {
            // Names follow the same rules as new presets and groups.
            if let Some(n) = p.get("name").filter(|_| action != "move") {
                checked_name(n.as_str().ok_or_else(|| bad(CMD, "`name` must be a string"))?, CMD)?;
            }
            if action == "move" {
                checked_name(req_str(p, "to", CMD)?, CMD)?;
                if groups.len() >= MAX_GROUPS && !groups.iter().any(|g| Some(g.name.as_str()) == str_param(p, "to")) {
                    return Err(bad(CMD, format!("at most {MAX_GROUPS} preset groups")));
                }
            }
            if action == "newGroup" && groups.len() >= MAX_GROUPS {
                return Err(bad(CMD, format!("at most {MAX_GROUPS} preset groups")));
            }
            if action == "renameGroup"
                && let Some(to) = str_param(p, "name")
                && str_param(p, "group") != Some(to)
                && groups.iter().any(|g| g.name == to)
            {
                return Err(bad(CMD, format!("a group named \"{to}\" already exists")));
            }
            edit_groups(groups, action, p, CMD)?
        }
        "delete" | "deleteGroup" => edit_groups(groups, action, p, CMD)?,
        _ => return Err(bad(CMD, format!("unknown action {action}"))),
    };
    s.presets_changed();
    Ok(result)
}

fn import(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "cameraRaw.presets.import";
    let xmp = p.get("xmp").and_then(Value::as_str).ok_or_else(|| bad(CMD, "`xmp` must be the preset file's text"))?;
    let mut full = CameraRaw::default();
    let report = camera_raw_xmp::load(&mut full, xmp.as_bytes()).map_err(|e| bad(CMD, e))?;
    if report.fields.is_empty() {
        return Err(bad(CMD, "the file holds no settings PhotoCraft supports"));
    }
    // A settings file has no name of its own: fall back to its file name.
    let stem = str_param(p, "fileName").map(|f| f.rsplit(['/', '\\']).next().unwrap_or(f)).map(|f| f.rsplit_once('.').map_or(f, |(stem, _)| stem));
    let base = match str_param(p, "name").or(report.name.as_deref()).or(stem.filter(|s| !s.trim().is_empty())) {
        Some(n) => checked_name(n, CMD)?,
        None => "Imported Preset".to_string(),
    };
    let groups = &mut s.presets.camera_raw;
    if count(groups) >= MAX_PRESETS {
        return Err(bad(CMD, format!("at most {MAX_PRESETS} Camera Raw presets can be saved")));
    }
    let name = unique_name(groups, &base);
    let gi = target_group(groups, str_param(p, "group").or(report.group.as_deref()).or(Some(USER_GROUP)), CMD)?;
    let group = groups.get_mut(gi).ok_or_else(|| bad(CMD, "preset group vanished"))?;
    let preset = CameraRawPreset { name, favorite: false, settings: subset(&full, &report.fields) };
    let result = json!({"preset": describe(&preset, &group.name), "ignored": report.ignored});
    group.items.push(preset);
    s.presets_changed();
    Ok(result)
}

fn export(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "cameraRaw.presets.export";
    let (group, preset) = lookup(s, p, CMD)?;
    let full = preset.full_settings()?;
    let fields: Vec<&str> = preset.settings.keys().map(String::as_str).collect();
    let xmp = camera_raw_xmp::save_preset(&full, &fields, &preset.name, Some(&group.name));
    let file: String = preset.name.chars().map(|c| if c.is_alphanumeric() || " -_".contains(c) { c } else { '_' }).collect();
    Ok(json!({"xmp": xmp, "fileName": format!("{}.xmp", file.trim())}))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "cameraRaw.presets.list",
            label: "List Camera Raw Presets",
            menu: &[],
            shortcut: None,
            params: "{} -> {groups:[{name,presets:[{name,group,favorite,settings}]}],favorites:[{name,group}],sections:[{name,fields}]} (sections: Create Preset's setting groups)",
            enabled: always,
            run: list,
            journal: false,
        },
        CommandSpec {
            id: "cameraRaw.presets.get",
            label: "Get Camera Raw Preset",
            menu: &[],
            shortcut: None,
            params: r#"{"preset":name,"group":name?} -> {preset:{name,group,favorite,settings}} (send settings as filter.cameraRaw params, or ui.set in the dialog, to apply it)"#,
            enabled: always,
            run: get,
            journal: false,
        },
        CommandSpec {
            id: "cameraRaw.presets.save",
            label: "Create Camera Raw Preset",
            menu: &[],
            shortcut: None,
            params: r#"{"name":str (1..255 chars, unique),"group":name? (created when missing; default the first group),"settings":{Camera Raw settings to include, e.g. "exposure":0.5,"hslSat":[…]},"replace":bool? (true: update an existing preset's settings)}"#,
            enabled: always,
            run: save,
            journal: true,
        },
        CommandSpec {
            id: "cameraRaw.presets.edit",
            label: "Edit Camera Raw Presets",
            menu: &[],
            shortcut: None,
            params: EDIT_PARAMS,
            enabled: always,
            run: edit,
            journal: true,
        },
        CommandSpec {
            id: "cameraRaw.presets.import",
            label: "Import Camera Raw Preset",
            menu: &[],
            shortcut: None,
            params: r#"{"xmp":str (a Camera Raw preset or settings .xmp file's text),"name":str? (default the file's crs:Name, else fileName without its extension),"fileName":str?,"group":name? (default the file's crs:Group, else "User Presets")} -> {preset,ignored:[crs names PhotoCraft has no control for]}"#,
            enabled: always,
            run: import,
            journal: true,
        },
        CommandSpec {
            id: "cameraRaw.presets.export",
            label: "Export Camera Raw Preset",
            menu: &[],
            shortcut: None,
            params: r#"{"preset":name,"group":name?} -> {xmp,fileName} (a Camera Raw preset file Photoshop imports)"#,
            enabled: always,
            run: export,
            journal: false,
        },
    ]
}

/// The shared preset group edits (`crate::presets::GROUP_EDIT_PARAMS`) plus `favorite`.
const EDIT_PARAMS: &str = r##"{"action":"favorite","preset":name,"favorite":bool} or {"action":"rename|delete|move|newGroup|renameGroup|deleteGroup","preset":name|[names] (rename/delete/move),"group":name? (narrows the lookup; the group for renameGroup/deleteGroup),"name":str (rename: new name; newGroup/renameGroup: group name),"to":group (move),"index":n? (move)}"##;

#[cfg(test)]
mod tests;
