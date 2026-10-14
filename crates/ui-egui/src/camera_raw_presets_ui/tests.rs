use super::*;
use crate::camera_raw_ui::{menu, show};
use crate::file_dialog::FileDialogAnswer;
use std::sync::{Arc, Mutex};

type Written = Arc<Mutex<Vec<(String, Vec<u8>)>>>;

fn app_with(answers: Vec<Option<FileDialogAnswer>>) -> (PhotocraftApp, Written) {
    let written: Written = Arc::default();
    let w = written.clone();
    let services = crate::Services {
        file_dialog: Some(crate::file_dialog::fake(answers).0),
        write: Some(Box::new(move |p: &str, b: &[u8]| {
            w.lock().unwrap().push((p.to_string(), b.to_vec()));
            Ok(())
        })),
        ..Default::default()
    };
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), services);
    app.run("file.new", json!({"width": 16, "height": 16})).unwrap();
    app.run("layer.new.layer", json!({})).unwrap();
    app.run("edit.fill", json!({"color": "#808080"})).unwrap();
    (app, written)
}

fn presets(app: &mut PhotocraftApp, ctx: &egui::Context, request: Value) -> Result<Value, String> {
    menu(app, ctx, "filter.cameraRaw", &json!({"ui": {"presets": request}})).unwrap()
}

fn set(app: &mut PhotocraftApp, ctx: &egui::Context, settings: Value) {
    menu(app, ctx, "filter.cameraRaw", &json!({"ui": {"set": settings}})).unwrap().unwrap();
}

#[test]
fn create_then_apply_sets_only_the_preset_sections() {
    let (mut app, _) = app_with(Vec::new());
    let ctx = egui::Context::default();
    set(&mut app, &ctx, json!({"exposure": 0.75, "contrast": 20, "vibrance": 30, "hslSat": [0, 0, 0, 30, 0, 0, 0, 0]}));
    let r = presets(&mut app, &ctx, json!({"create": {"name": "Bright", "sections": ["Light", "Color Mixer"]}})).unwrap();
    let saved = r["preset"]["settings"].as_object().unwrap();
    assert_eq!(saved.len(), SECTIONS[0].1.len() + 3, "{saved:?}");
    assert!(saved.contains_key("exposure") && saved.contains_key("hslSat") && !saved.contains_key("vibrance"));
    // Change everything, then apply: Light and Color Mixer come back, Color stays as it is now.
    set(&mut app, &ctx, json!({"exposure": -1.0, "contrast": 0, "vibrance": -50, "hslSat": [0, 0, 0, 0, 0, 0, 0, 0]}));
    let r = presets(&mut app, &ctx, json!({"apply": "Bright"})).unwrap();
    assert_eq!(r["applied"], "Bright");
    let d = app.camera_raw.as_ref().unwrap();
    assert_eq!((d.params.exposure, d.params.contrast, d.params.vibrance, d.params.hsl_sat[3]), (0.75, 20.0, -50.0, 30.0));
    let state = menu(&mut app, &ctx, "filter.cameraRaw", &json!({"ui": {}})).unwrap().unwrap();
    assert_eq!(state["presets"]["applied"], json!({"group": "User Presets", "name": "Bright"}));
    // OK commits the applied preset like any other edit.
    menu(&mut app, &ctx, "filter.cameraRaw", &json!({"ui": {"commit": true}})).unwrap().unwrap();
    assert!(app.camera_raw.is_none());
    // The preset outlives the dialog (it is in the engine's preferences).
    assert_eq!(app.session.presets.camera_raw[0].items[0].name, "Bright");
}

#[test]
fn update_keeps_the_preset_sections_with_current_values() {
    let (mut app, _) = app_with(Vec::new());
    let ctx = egui::Context::default();
    set(&mut app, &ctx, json!({"clarity": 10}));
    presets(&mut app, &ctx, json!({"create": {"name": "Fx", "group": "Mine", "sections": ["Effects"]}})).unwrap();
    set(&mut app, &ctx, json!({"clarity": 45, "exposure": 2}));
    presets(&mut app, &ctx, json!({"update": "Fx", "group": "Mine"})).unwrap();
    let p = &app.session.presets.camera_raw.iter().find(|g| g.name == "Mine").unwrap().items[0];
    assert_eq!(p.settings["clarity"], 45.0);
    assert!(!p.settings.contains_key("exposure"));
}

#[test]
fn presets_export_to_and_import_from_files() {
    let (mut app, written) = app_with(Vec::new());
    let ctx = egui::Context::default();
    set(&mut app, &ctx, json!({"dehaze": 33}));
    presets(&mut app, &ctx, json!({"create": {"name": "Haze", "sections": ["Effects"]}})).unwrap();
    let mut svc = crate::file_dialog::fake(vec![Some(FileDialogAnswer::SaveTo("/out/Haze.xmp".into()))]).0;
    std::mem::swap(app.services.file_dialog.as_mut().unwrap(), &mut svc);
    assert_eq!(presets(&mut app, &ctx, json!({"export": "Haze"})).unwrap()["fileDialog"], "save");
    app.poll_file_dialog(&ctx, None);
    let (path, bytes) = written.lock().unwrap()[0].clone();
    assert_eq!(path, "/out/Haze.xmp");
    let text = String::from_utf8(bytes.clone()).unwrap();
    assert!(text.contains("crs:Dehaze=\"+33\"") && text.contains("Haze"), "{text}");
    // Import it back (a second copy, uniquely named) into a fresh app.
    let (mut other, _) = app_with(vec![Some(FileDialogAnswer::Contents("Haze.xmp".into(), bytes))]);
    menu(&mut other, &ctx, "filter.cameraRaw", &json!({})).unwrap().unwrap();
    presets(&mut other, &ctx, json!({"import": true})).unwrap();
    other.poll_file_dialog(&ctx, None);
    assert!(!other.ui.status_error, "{}", other.ui.status);
    assert_eq!(other.session.presets.camera_raw[0].items[0].name, "Haze");
    presets(&mut other, &ctx, json!({"apply": "Haze"})).unwrap();
    assert_eq!(other.camera_raw.as_ref().unwrap().params.dehaze, 33.0);
}

#[test]
fn bad_requests_are_errors_and_change_nothing() {
    let (mut app, _) = app_with(Vec::new());
    let ctx = egui::Context::default();
    assert!(presets(&mut app, &ctx, json!({"panel": "presets"})).is_err(), "the dialog isn't open");
    set(&mut app, &ctx, json!({"exposure": 0.5}));
    let before = app.camera_raw.as_ref().unwrap().params.clone();
    for bad in [
        json!(null),
        json!([]),
        json!({}),
        json!({"panel": "nope"}),
        json!({"apply": "Missing"}),
        json!({"apply": 3}),
        json!({"apply": "  "}),
        json!({"apply": "x", "update": "y"}),
        json!({"explode": true}),
        json!({"import": false}),
        json!({"create": "x"}),
        json!({"create": {"name": ""}}),
        json!({"create": {"name": "x", "sections": []}}),
        json!({"create": {"name": "x", "sections": ["Nope"]}}),
        json!({"create": {"name": "x", "sections": "Light"}}),
        json!({"create": {"name": "x", "extra": 1}}),
        json!({"update": "Missing"}),
        json!({"export": "Missing"}),
        json!({"apply": "x", "group": 5}),
    ] {
        assert!(presets(&mut app, &ctx, bad.clone()).is_err(), "{bad}");
    }
    assert!(menu(&mut app, &ctx, "filter.cameraRaw", &json!({"ui": {"presets": {"panel": "presets"}, "commit": true}})).unwrap().is_err());
    assert_eq!(app.camera_raw.as_ref().unwrap().params, before);
    assert_eq!(app.session.presets.camera_raw.iter().map(|g| g.items.len()).sum::<usize>(), 0);
}

#[test]
fn the_presets_panel_draws_and_clicking_a_preset_applies_it() {
    use egui_kittest::kittest::Queryable;
    let (mut app, _) = app_with(Vec::new());
    let ctx = egui::Context::default();
    set(&mut app, &ctx, json!({"saturation": -100}));
    presets(&mut app, &ctx, json!({"create": {"name": "Mono", "sections": ["Color"]}})).unwrap();
    app.run("cameraRaw.presets.edit", json!({"action": "favorite", "preset": "Mono", "favorite": true})).unwrap();
    set(&mut app, &ctx, json!({"saturation": 0}));
    let mut h = egui_kittest::Harness::builder().with_size(vec2(1360.0, 900.0)).build_ui_state(
        |ui, app: &mut PhotocraftApp| {
            if ui.ctx().fonts(|f| f.families().contains(&egui::FontFamily::Name("medium".into()))) {
                show(app, ui.ctx());
            }
        },
        app,
    );
    PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::Pro);
    h.run_steps(2);
    // Switch to the Presets panel with its tab, as a user would.
    h.get_by_label("Presets").click();
    h.run_steps(2);
    assert!(h.state().camera_raw.as_ref().unwrap().presets.shown);
    // The favourite is listed twice (Favorites and its group); click the first row.
    let rows: Vec<_> = h.get_all_by_label("Mono").collect();
    assert_eq!(rows.len(), 2);
    rows[0].click();
    h.run_steps(2);
    let app = h.state();
    assert_eq!(app.camera_raw.as_ref().unwrap().params.saturation, -100.0, "{}", app.ui.status);
    assert!(app.ui.status.contains("Mono"), "{}", app.ui.status);
}
