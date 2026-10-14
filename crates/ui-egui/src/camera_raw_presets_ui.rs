//! Camera Raw › Presets: the dialog's right column switches between the edit panels and the
//! Presets panel, as in Adobe Camera Raw. Presets are listed by group (favourites first);
//! clicking one applies its settings over the dialog's. Create Preset saves the chosen sections
//! of the current settings; each preset's context menu adds it to the favourites, updates it
//! with the current settings, renames, moves, exports (`.xmp`) or deletes it. The panel menu
//! imports Camera Raw preset files and makes groups.
//!
//! The list is the engine's (`cameraRaw.presets.*` commands, persisted with the preferences),
//! so every edit here is a command. Applying a preset changes only the dialog's settings, like
//! moving a slider; OK commits them.
//!
//! Control channel: `{"ui":{"presets":{…}}}` on `filter.cameraRaw`, one of
//! `{"panel":"presets"|"edit"}`, `{"apply":name,"group":name?}`,
//! `{"create":{"name":str,"group":name?,"sections":[section names]?}}` (default every section),
//! `{"update":name,"group":name?}`, `{"import":true}` and `{"export":name,"group":name?}` (file
//! dialogs). Other edits go straight to the `cameraRaw.presets.*` commands.

use egui::{Sense, vec2};
use photocraft_engine::camera_raw_preset_cmds::{CameraRawPreset, SECTIONS, USER_GROUP, subset};
use photocraft_engine::presets::Group;
use serde_json::{Value, json};

use crate::PhotocraftApp;
use crate::theme::Tokens;
use crate::widgets;

/// The Presets panel's view state (lives with the dialog).
#[derive(Default)]
pub(crate) struct PresetsView {
    /// The Presets panel is showing instead of the edit panels.
    pub(crate) shown: bool,
    filter: String,
    create: Option<CreateForm>,
    rename: Option<Rename>,
    new_group: Option<String>,
    /// The preset last applied in this dialog (highlighted in the list).
    pub(crate) applied: Option<(String, String)>,
}

struct CreateForm {
    /// Focus the name field on the form's first frame.
    focus: bool,
    name: String,
    group: String,
    sections: Vec<bool>,
}

enum Rename {
    Preset { group: String, name: String, text: String },
    Group { name: String, text: String },
}

/// What the panel asked for this frame; run after drawing, when the dialog isn't borrowed.
pub(crate) enum Action {
    Apply { group: String, name: String },
    Create { name: String, group: String, sections: Vec<&'static str> },
    Update { group: String, name: String },
    Command(&'static str, Value),
    Import,
    Export { group: String, name: String },
}

fn section_name(name: &str) -> &str {
    crate::i18n::tr_ctx(crate::i18n::current(), "cameraRaw", name)
}

/// Edit / Presets switch at the top of the right column.
pub(crate) fn tabs(ui: &mut egui::Ui, view: &mut PresetsView) {
    ui.horizontal(|ui| {
        for (presets, label) in [(false, "Edit"), (true, "Presets")] {
            let name = crate::i18n::tr_ctx(crate::i18n::current(), "cameraRaw", label);
            let selected = view.shown == presets;
            let response = widgets::pill_tab(ui, name, selected);
            response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, name));
            if response.clicked() {
                view.shown = presets;
            }
        }
    });
    ui.add_space(4.0);
}

/// The Presets panel body.
pub(crate) fn panel(ui: &mut egui::Ui, view: &mut PresetsView, groups: &[Group<CameraRawPreset>], acts: &mut Vec<Action>) {
    if let Some(form) = view.create.as_mut() {
        if create_form(ui, form, groups, acts) {
            view.create = None;
        }
        return;
    }
    ui.horizontal(|ui| {
        let width = (ui.available_width() - 56.0).max(60.0);
        widgets::text_field(ui, egui::Id::new("camera-raw-preset-search"), &mut view.filter, tl!("Search presets"), width, false);
        if ui.button("+").on_hover_text(tl!("Create Preset…")).clicked() {
            let group = groups.first().map_or(USER_GROUP.to_string(), |g| g.name.clone());
            view.create = Some(CreateForm { focus: true, name: String::new(), group, sections: vec![true; SECTIONS.len()] });
        }
        let menu = widgets::panel_menu_button(ui);
        egui::Popup::menu(&menu).show(|ui| {
            widgets::style_spectrum_popup_menu(ui);
            ui.set_min_width(180.0);
            if ui.button(tl!("Import Presets…")).clicked() {
                acts.push(Action::Import);
                ui.close();
            }
            if ui.button(tl!("New Group…")).clicked() {
                view.new_group = Some(String::new());
                ui.close();
            }
        });
    });
    if let Some(text) = view.new_group.as_mut() {
        match name_field(ui, "camera-raw-new-group", text, tl!("Group name")) {
            Some(true) => {
                acts.push(Action::Command("cameraRaw.presets.edit", json!({"action":"newGroup","name":text.trim()})));
                view.new_group = None;
            }
            Some(false) => view.new_group = None,
            None => {}
        }
    }
    ui.add_space(4.0);
    let needle = view.filter.trim().to_lowercase();
    let matches = |p: &CameraRawPreset| needle.is_empty() || p.name.to_lowercase().contains(&needle);
    let total: usize = groups.iter().map(|g| g.items.iter().filter(|p| matches(p)).count()).sum();
    if total == 0 {
        let t = Tokens::get(ui.ctx());
        let text = if needle.is_empty() { tl!("No presets yet. Use + to save the current settings as a preset.") } else { tl!("No presets match.") };
        ui.label(egui::RichText::new(text).color(t.text_faint));
    }
    let favorites: Vec<(&str, &CameraRawPreset)> = groups.iter().flat_map(|g| g.items.iter().filter(|p| p.favorite && matches(p)).map(|p| (g.name.as_str(), p))).collect();
    let group_names: Vec<&str> = groups.iter().map(|g| g.name.as_str()).collect();
    if !favorites.is_empty() {
        header(ui, egui::Id::new("camera-raw-favorites"), tl!("Favorites"), |ui| {
            for (group, p) in &favorites {
                row(ui, view, group, p, &group_names, acts);
            }
        });
    }
    for g in groups {
        if !needle.is_empty() && !g.items.iter().any(matches) {
            continue;
        }
        let renaming = matches!(&view.rename, Some(Rename::Group { name, .. }) if *name == g.name);
        if renaming {
            if let Some(Rename::Group { text, .. }) = view.rename.as_mut() {
                match name_field(ui, "camera-raw-rename-group", text, tl!("Group name")) {
                    Some(true) => {
                        acts.push(Action::Command("cameraRaw.presets.edit", json!({"action":"renameGroup","group":g.name,"name":text.trim()})));
                        view.rename = None;
                    }
                    Some(false) => view.rename = None,
                    None => {}
                }
            }
            continue;
        }
        let response = header(ui, egui::Id::new(("camera-raw-group", g.name.as_str())), &format!("{} ({})", g.name, g.items.len()), |ui| {
            for p in g.items.iter().filter(|p| matches(p)) {
                row(ui, view, &g.name, p, &group_names, acts);
            }
        });
        response.context_menu(|ui| {
            if ui.button(tl!("Rename Group…")).clicked() {
                view.rename = Some(Rename::Group { name: g.name.clone(), text: g.name.clone() });
                ui.close();
            }
            if ui.button(tl!("Delete Group")).clicked() {
                acts.push(Action::Command("cameraRaw.presets.edit", json!({"action":"deleteGroup","group":g.name})));
                ui.close();
            }
        });
    }
}

fn header(ui: &mut egui::Ui, id: egui::Id, title: &str, body: impl FnOnce(&mut egui::Ui)) -> egui::Response {
    egui::CollapsingHeader::new(egui::RichText::new(title).font(egui::FontId::proportional(13.0))).id_salt(id).default_open(true).show(ui, body).header_response
}

/// A one-line name editor, focused when it appears: `Some(true)` when it loses focus holding a
/// name (Enter or a click elsewhere), `Some(false)` on Escape or when left empty.
fn name_field(ui: &mut egui::Ui, id: &str, text: &mut String, hint: &str) -> Option<bool> {
    let id = egui::Id::new(id);
    let r = widgets::text_field_ex(ui, id, text, hint, ui.available_width(), false, Some(255));
    let seen = id.with("focused");
    if ui.data_mut(|d| d.get_temp::<bool>(seen).is_none()) {
        ui.data_mut(|d| d.insert_temp(seen, true));
        r.request_focus();
    }
    let done = if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        Some(false)
    } else if r.lost_focus() {
        Some(!text.trim().is_empty())
    } else {
        None
    };
    if done.is_some() {
        ui.data_mut(|d| d.remove::<bool>(seen));
    }
    done
}

fn row(ui: &mut egui::Ui, view: &mut PresetsView, group: &str, p: &CameraRawPreset, groups: &[&str], acts: &mut Vec<Action>) {
    if let Some(Rename::Preset { group: g, name, text }) = view.rename.as_mut()
        && g == group
        && *name == p.name
    {
        match name_field(ui, "camera-raw-rename-preset", text, tl!("Preset name")) {
            Some(true) => {
                acts.push(Action::Command("cameraRaw.presets.edit", json!({"action":"rename","group":group,"preset":p.name,"name":text.trim()})));
                view.rename = None;
            }
            Some(false) => view.rename = None,
            None => {}
        }
        return;
    }
    let t = Tokens::get(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click());
    let current = view.applied.as_ref().is_some_and(|(g, n)| g == group && *n == p.name);
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, current, &p.name));
    if ui.is_rect_visible(r) {
        if current {
            ui.painter().rect_filled(r, t.radius_sm, t.accent_soft);
        } else if resp.hovered() {
            ui.painter().rect_filled(r, t.radius_sm, t.hover);
        }
        let star = if p.favorite { "★" } else { "" };
        ui.painter().text(egui::pos2(r.left() + 10.0, r.center().y), egui::Align2::LEFT_CENTER, star, egui::FontId::proportional(11.0), t.accent);
        crate::layer_row_ui::label(ui.painter(), r.left() + 24.0, r.center().y, r.right() - 4.0, &p.name, egui::FontId::proportional(12.5), t.text);
    }
    let settings: Vec<&str> = SECTIONS.iter().filter(|(_, fields)| fields.iter().any(|f| p.settings.contains_key(*f))).map(|(n, _)| section_name(n)).collect();
    let resp = resp.on_hover_text(crate::i18n::fmt(tl!("Settings: {sections}"), &[("sections", &settings.join(", "))]));
    if resp.clicked() {
        acts.push(Action::Apply { group: group.into(), name: p.name.clone() });
    }
    resp.context_menu(|ui| {
        let favorite = if p.favorite { tl!("Remove from Favorites") } else { tl!("Add to Favorites") };
        if ui.button(favorite).clicked() {
            acts.push(Action::Command("cameraRaw.presets.edit", json!({"action":"favorite","group":group,"preset":p.name,"favorite":!p.favorite})));
            ui.close();
        }
        if ui.button(tl!("Update with Current Settings")).clicked() {
            acts.push(Action::Update { group: group.into(), name: p.name.clone() });
            ui.close();
        }
        if ui.button(tl!("Rename…")).clicked() {
            view.rename = Some(Rename::Preset { group: group.into(), name: p.name.clone(), text: p.name.clone() });
            ui.close();
        }
        let targets: Vec<&str> = groups.iter().copied().filter(|g| *g != group).collect();
        if targets.is_empty() {
            ui.add_enabled(false, egui::Button::new(tl!("Move to Group")));
        } else {
            ui.menu_button(tl!("Move to Group"), |ui| {
                for to in targets {
                    if ui.button(to).clicked() {
                        acts.push(Action::Command("cameraRaw.presets.edit", json!({"action":"move","group":group,"preset":p.name,"to":to})));
                        ui.close();
                    }
                }
            });
        }
        if ui.button(tl!("Export…")).clicked() {
            acts.push(Action::Export { group: group.into(), name: p.name.clone() });
            ui.close();
        }
        ui.separator();
        if ui.button(tl!("Delete")).clicked() {
            acts.push(Action::Command("cameraRaw.presets.edit", json!({"action":"delete","group":group,"preset":p.name})));
            ui.close();
        }
    });
}

/// Create Preset: name, group and the sections to include. True when it closes.
fn create_form(ui: &mut egui::Ui, form: &mut CreateForm, groups: &[Group<CameraRawPreset>], acts: &mut Vec<Action>) -> bool {
    widgets::section_label(ui, tl!("Create Preset"));
    let name = widgets::text_field_ex(ui, egui::Id::new("camera-raw-create-name"), &mut form.name, tl!("Preset name"), ui.available_width(), false, Some(255));
    if std::mem::take(&mut form.focus) {
        name.request_focus();
    }
    ui.add_space(4.0);
    ui.label(tl!("Group"));
    let mut options: Vec<(String, &str)> = groups.iter().map(|g| (g.name.clone(), g.name.as_str())).collect();
    if options.is_empty() {
        options.push((USER_GROUP.to_string(), USER_GROUP));
    }
    widgets::dropdown(ui, "camera-raw-create-group", &mut form.group, &options, ui.available_width().min(240.0));
    ui.add_space(6.0);
    widgets::section_label(ui, tl!("Settings"));
    ui.horizontal(|ui| {
        for (on, label) in [(true, tl!("Check All")), (false, tl!("Check None"))] {
            if ui.small_button(label).clicked() {
                form.sections.iter_mut().for_each(|s| *s = on);
            }
        }
    });
    for ((name, _), on) in SECTIONS.iter().zip(form.sections.iter_mut()) {
        widgets::checkbox(ui, on, section_name(name));
    }
    ui.add_space(8.0);
    let chosen: Vec<&'static str> = SECTIONS.iter().zip(&form.sections).filter(|(_, on)| **on).map(|((name, _), _)| *name).collect();
    let ready = !form.name.trim().is_empty() && !chosen.is_empty();
    let mut done = false;
    ui.horizontal(|ui| {
        if ui.add_enabled(ready, egui::Button::new(tl!("Create"))).clicked() || (ready && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
            acts.push(Action::Create { name: form.name.trim().to_string(), group: form.group.clone(), sections: chosen.clone() });
            done = true;
        }
        if ui.button(tl!("Cancel")).clicked() {
            done = true;
        }
    });
    done
}

fn fields_of(sections: &[&str]) -> Result<Vec<&'static str>, String> {
    let mut fields = Vec::new();
    for name in sections {
        let (_, f) = SECTIONS.iter().find(|(n, _)| n == name).ok_or_else(|| format!("unknown Camera Raw section {name}"))?;
        fields.extend_from_slice(f);
    }
    Ok(fields)
}

fn find_preset(app: &PhotocraftApp, group: Option<&str>, name: &str) -> Result<(String, CameraRawPreset), String> {
    app.session
        .presets
        .camera_raw
        .iter()
        .filter(|g| group.is_none_or(|n| g.name == n))
        .find_map(|g| g.items.iter().find(|p| p.name == name).map(|p| (g.name.clone(), p.clone())))
        .ok_or_else(|| crate::i18n::fmt(tl!("No Camera Raw preset named {name}"), &[("name", name)]))
}

/// Run what the panel asked for; errors go to the status bar.
pub(crate) fn run(app: &mut PhotocraftApp, acts: Vec<Action>) {
    for act in acts {
        if let Err(e) = run_one(app, act)
            && e != crate::file_dialog::CANCELLED
        {
            app.ui.status_error = true;
            app.ui.status = e;
        }
    }
}

fn run_one(app: &mut PhotocraftApp, act: Action) -> Result<Value, String> {
    match act {
        Action::Apply { group, name } => apply(app, Some(&group), &name),
        Action::Create { name, group, sections } => create(app, &name, Some(&group), &sections),
        Action::Update { group, name } => update(app, Some(&group), &name),
        Action::Command(id, params) => app.run(id, params),
        Action::Import => import(app),
        Action::Export { group, name } => export(app, Some(&group), &name),
    }
}

/// Apply a preset's settings over the dialog's.
fn apply(app: &mut PhotocraftApp, group: Option<&str>, name: &str) -> Result<Value, String> {
    let (group, preset) = find_preset(app, group, name)?;
    let d = app.camera_raw.as_mut().ok_or(tl!("Camera Raw isn't open"))?;
    let next = photocraft_engine::camera_raw_preset_cmds::apply(&d.params, &preset.settings)?;
    next.validate_changes(d.original())?;
    d.set_params(next);
    d.presets.applied = Some((group.clone(), preset.name.clone()));
    app.ui.status_error = false;
    app.ui.status = crate::i18n::fmt(tl!("Applied Camera Raw preset {name}"), &[("name", &preset.name)]);
    Ok(json!({"applied": preset.name, "group": group, "settings": preset.settings}))
}

fn create(app: &mut PhotocraftApp, name: &str, group: Option<&str>, sections: &[&str]) -> Result<Value, String> {
    let d = app.camera_raw.as_ref().ok_or(tl!("Camera Raw isn't open"))?;
    if sections.is_empty() {
        return Err(tl!("Choose at least one group of settings for the preset").into());
    }
    let settings = subset(&d.params, &fields_of(sections)?);
    let mut params = json!({"name": name, "settings": settings});
    if let Some(g) = group {
        params["group"] = json!(g);
    }
    let r = app.run("cameraRaw.presets.save", params)?;
    if let (Some(g), Some(n)) = (r["preset"]["group"].as_str(), r["preset"]["name"].as_str())
        && let Some(d) = app.camera_raw.as_mut()
    {
        d.presets.applied = Some((g.into(), n.into()));
    }
    Ok(r)
}

/// Update with Current Settings: the preset keeps the settings it holds, with the dialog's values.
fn update(app: &mut PhotocraftApp, group: Option<&str>, name: &str) -> Result<Value, String> {
    let (_, preset) = find_preset(app, group, name)?;
    let d = app.camera_raw.as_ref().ok_or(tl!("Camera Raw isn't open"))?;
    let fields: Vec<&str> = preset.settings.keys().map(String::as_str).collect();
    let settings = subset(&d.params, &fields);
    app.run("cameraRaw.presets.save", json!({"name": preset.name, "settings": settings, "replace": true}))
}

fn import(app: &mut PhotocraftApp) -> Result<Value, String> {
    app.pick_file_bytes_filtered(&["xmp"], |app, path, bytes| {
        let file = crate::file_open::display_name(&path);
        let text = String::from_utf8(bytes).map_err(|_| format!("{file}: {}", tl!("not a Camera Raw preset file")))?;
        let r = app.run("cameraRaw.presets.import", json!({"xmp": text, "fileName": file})).map_err(|e| format!("{file}: {e}"))?;
        let name = r["preset"]["name"].as_str().unwrap_or_default();
        let ignored = r["ignored"].as_array().map_or(0, Vec::len);
        app.ui.status_error = false;
        app.ui.status = if ignored == 0 {
            crate::i18n::fmt(tl!("Imported Camera Raw preset {name}"), &[("name", name)])
        } else {
            let template = "Imported Camera Raw preset {name} ({n} settings not supported)";
            crate::i18n::fmt(tl!(template), &[("name", name), ("n", &ignored.to_string())])
        };
        Ok(r)
    })
}

fn export(app: &mut PhotocraftApp, group: Option<&str>, name: &str) -> Result<Value, String> {
    let mut params = json!({"preset": name});
    if let Some(g) = group {
        params["group"] = json!(g);
    }
    let r = app.run("cameraRaw.presets.export", params)?;
    let bytes = r["xmp"].as_str().unwrap_or_default().as_bytes().to_vec();
    let file = r["fileName"].as_str().unwrap_or("Camera Raw Preset.xmp").to_string();
    app.pick_save(&file, move |app, path| {
        let write = app.services.write.as_mut().ok_or("no writer configured")?;
        write(&path, &bytes)?;
        Ok(json!({"path": path}))
    })
}

/// `{"ui":{"presets":{…}}}` (see the module docs).
pub(crate) fn control(app: &mut PhotocraftApp, request: &Value) -> Result<Value, String> {
    let fields = request.as_object().ok_or("Camera Raw presets must be an object")?;
    let d = app.camera_raw.as_mut().ok_or(tl!("Camera Raw isn't open"))?;
    let group = request.get("group").map(|g| g.as_str().ok_or("presets.group must be a string")).transpose()?;
    let action = ["panel", "apply", "create", "update", "import", "export"]
        .into_iter()
        .filter(|k| fields.contains_key(*k))
        .collect::<Vec<_>>();
    if let Some(k) = fields.keys().find(|k| !matches!(k.as_str(), "panel" | "apply" | "create" | "update" | "import" | "export" | "group")) {
        return Err(format!("unknown Camera Raw presets property {k}"));
    }
    let [action] = action[..] else { return Err("send one of panel, apply, create, update, import or export".into()) };
    let value = &request[action];
    let name = || value.as_str().map(str::trim).filter(|s| !s.is_empty()).ok_or(format!("presets.{action} must be a preset name"));
    match action {
        "panel" => {
            d.presets.shown = match value.as_str() {
                Some("presets") => true,
                Some("edit") => false,
                _ => return Err("presets.panel must be \"presets\" or \"edit\"".into()),
            };
            Ok(json!({"panel": value}))
        }
        "apply" => apply(app, group, name()?),
        "update" => update(app, group, name()?),
        "export" => export(app, group, name()?),
        "import" if value.as_bool() == Some(true) => import(app),
        "import" => Err("presets.import must be true".into()),
        _ => {
            let form = value.as_object().ok_or("presets.create must be an object")?;
            if let Some(k) = form.keys().find(|k| !matches!(k.as_str(), "name" | "group" | "sections")) {
                return Err(format!("unknown presets.create property {k}"));
            }
            let name = value.get("name").and_then(Value::as_str).ok_or("presets.create.name must be a string")?;
            let group = value.get("group").map(|g| g.as_str().ok_or("presets.create.group must be a string")).transpose()?;
            let sections: Vec<&str> = match value.get("sections") {
                None => SECTIONS.iter().map(|(n, _)| *n).collect(),
                Some(Value::Array(a)) => a.iter().map(|s| s.as_str().ok_or("presets.create.sections must be section names")).collect::<Result<_, _>>()?,
                Some(_) => return Err("presets.create.sections must be an array of section names".into()),
            };
            create(app, name, group, &sections)
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
