use super::*;

fn reload(s: &Session) -> Session {
    let mut next = Session::new();
    next.load_prefs_json(&s.prefs_to_json()).unwrap();
    next
}

fn save(s: &mut Session, p: Value) -> Result<Value> {
    s.execute("cameraRaw.presets.save", p)
}

fn names(s: &Session) -> Vec<(String, Vec<String>)> {
    s.presets.camera_raw.iter().map(|g| (g.name.clone(), g.items.iter().map(|p| p.name.clone()).collect())).collect()
}

#[test]
fn sections_cover_every_supported_setting_once() {
    let mut covered: Vec<&str> = SECTIONS.iter().flat_map(|(_, f)| f.iter().copied()).collect();
    let n = covered.len();
    covered.sort_unstable();
    covered.dedup();
    assert_eq!(covered.len(), n, "a setting is in two sections");
    let mut supported = camera_raw_xmp::supported_fields();
    supported.sort_unstable();
    assert_eq!(covered, supported);
}

#[test]
fn a_preset_applies_only_its_settings_and_survives_a_restart() {
    let mut s = Session::new();
    let r = save(&mut s, json!({"name":" Punchy ","settings":{"exposure":0.5,"hslSat":[0,0,0,30,0,0,0,0],"pointCurve":[[0,0],[128,150],[255,255]]}})).unwrap();
    assert_eq!(r["preset"]["name"], "Punchy");
    assert_eq!(r["preset"]["group"], USER_GROUP);
    let mut s = reload(&s);
    let got = s.execute("cameraRaw.presets.get", json!({"preset":"Punchy"})).unwrap();
    let settings = got["preset"]["settings"].as_object().unwrap().clone();
    assert_eq!(settings.len(), 3);
    let current = CameraRaw { exposure: -1.0, contrast: 40.0, ..CameraRaw::default() };
    let next = apply(&current, &settings).unwrap();
    assert_eq!((next.exposure, next.contrast, next.hsl_sat[3]), (0.5, 40.0, 30.0));
    assert_eq!(next.point_curve, vec![[0.0, 0.0], [128.0, 150.0], [255.0, 255.0]]);
    // Settings sent as filter.cameraRaw params run the filter with the preset.
    s.execute("file.new", json!({"width":4,"height":4})).unwrap();
    s.execute("filter.cameraRaw", Value::Object(settings)).unwrap();
}

#[test]
fn update_favorite_rename_move_and_delete() {
    let mut s = Session::new();
    save(&mut s, json!({"name":"A","settings":{"contrast":10}})).unwrap();
    save(&mut s, json!({"name":"B","group":"Looks","settings":{"clarity":20}})).unwrap();
    assert!(save(&mut s, json!({"name":"A","settings":{"contrast":11}})).is_err(), "duplicate names are refused");
    assert!(save(&mut s, json!({"name":"Z","settings":{"contrast":11},"replace":true})).is_err(), "nothing to update");
    let r = save(&mut s, json!({"name":"A","settings":{"contrast":12,"dehaze":5},"replace":true})).unwrap();
    assert_eq!(r["replaced"], true);
    assert_eq!(s.presets.camera_raw[0].items[0].settings.len(), 2);
    let edit = |s: &mut Session, p: Value| s.execute("cameraRaw.presets.edit", p);
    edit(&mut s, json!({"action":"favorite","preset":"B","favorite":true})).unwrap();
    edit(&mut s, json!({"action":"rename","preset":"A","name":"Crisp"})).unwrap();
    edit(&mut s, json!({"action":"move","preset":"Crisp","to":"Looks","index":0})).unwrap();
    let mut s = reload(&s);
    assert_eq!(names(&s), [(USER_GROUP.into(), vec![]), ("Looks".into(), vec!["Crisp".into(), "B".into()])]);
    let list = s.execute("cameraRaw.presets.list", json!({})).unwrap();
    assert_eq!(list["favorites"], json!([{"name":"B","group":"Looks"}]));
    assert_eq!(list["sections"].as_array().unwrap().len(), SECTIONS.len());
    assert!(edit(&mut s, json!({"action":"renameGroup","group":"Looks","name":USER_GROUP})).is_err(), "group names stay unique");
    edit(&mut s, json!({"action":"delete","preset":"B"})).unwrap();
    edit(&mut s, json!({"action":"deleteGroup","group":"Looks"})).unwrap();
    assert_eq!(names(&s), [(USER_GROUP.into(), vec![])]);
    // With every group gone, a new preset recreates "User Presets".
    edit(&mut s, json!({"action":"deleteGroup","group":USER_GROUP})).unwrap();
    save(&mut s, json!({"name":"New","settings":{"tint":3}})).unwrap();
    assert_eq!(names(&s), [(USER_GROUP.into(), vec!["New".into()])]);
}

#[test]
fn presets_import_from_and_export_to_camera_raw_files() {
    let mut s = Session::new();
    save(&mut s, json!({"name":"Film / Warm","group":"Looks","settings":{"temperature":15,"grainAmount":30,"pointCurveRed":[[0,10],[255,245]]}})).unwrap();
    let out = s.execute("cameraRaw.presets.export", json!({"preset":"Film / Warm"})).unwrap();
    assert_eq!(out["fileName"], "Film _ Warm.xmp");
    let xmp = out["xmp"].as_str().unwrap();
    assert!(xmp.contains("crs:IncrementalTemperature=\"+15\"") && !xmp.contains("Contrast2012"), "{xmp}");
    let mut t = Session::new();
    let r = t.execute("cameraRaw.presets.import", json!({"xmp": xmp})).unwrap();
    assert_eq!((r["preset"]["name"].as_str(), r["preset"]["group"].as_str()), (Some("Film / Warm"), Some("Looks")));
    assert_eq!(t.presets.camera_raw[1].items[0].settings, s.presets.camera_raw[1].items[0].settings);
    // Importing again keeps both under unique names; an explicit name and group win.
    let r = t.execute("cameraRaw.presets.import", json!({"xmp": xmp})).unwrap();
    assert_eq!(r["preset"]["name"], "Film / Warm 2");
    let r = t.execute("cameraRaw.presets.import", json!({"xmp": xmp, "name":"Mine", "group":"Other"})).unwrap();
    assert_eq!((r["preset"]["name"].as_str(), r["preset"]["group"].as_str()), (Some("Mine"), Some("Other")));
    // A settings file (no crs:Name) is named after its file.
    let settings = camera_raw_xmp::save(&CameraRaw { clarity: 9.0, ..CameraRaw::default() });
    let r = t.execute("cameraRaw.presets.import", json!({"xmp": settings, "fileName": "C:\\looks\\Crisp.v2.xmp"})).unwrap();
    assert_eq!(r["preset"]["name"], "Crisp.v2");
    assert_eq!(r["preset"]["settings"].as_object().unwrap().len(), camera_raw_xmp::supported_fields().len());
    let r = t.execute("cameraRaw.presets.import", json!({"xmp": settings})).unwrap();
    assert_eq!(r["preset"]["name"], "Imported Preset");
    // A file with only unsupported settings, or no XMP at all, adds nothing.
    let lens = r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description
        xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/" crs:LensProfileEnable="1"/></rdf:RDF>"#;
    let before = names(&t);
    for bad in [json!({"xmp": lens}), json!({"xmp": "junk"}), json!({"xmp": 5}), json!({})] {
        assert!(t.execute("cameraRaw.presets.import", bad).is_err());
    }
    assert_eq!(names(&t), before);
}

#[test]
fn bad_params_are_errors_and_change_nothing() {
    let mut s = Session::new();
    save(&mut s, json!({"name":"Keep","settings":{"contrast":10}})).unwrap();
    let before = s.prefs_value();
    let rev = s.prefs.rev();
    for p in [
        Value::Null,
        json!([]),
        json!({"name":"x"}),
        json!({"name":"x","settings":{}}),
        json!({"name":"x","settings":[1]}),
        json!({"name":"","settings":{"contrast":1}}),
        json!({"name":"a\u{7}b","settings":{"contrast":1}}),
        json!({"name":"x".repeat(256),"settings":{"contrast":1}}),
        json!({"name":"x","settings":{"bogus":1}}),
        json!({"name":"x","settings":{"seed":1}}),
        json!({"name":"x","settings":{"pixelScale":4}}),
        json!({"name":"x","settings":{"contrast":"high"}}),
        json!({"name":"x","settings":{"contrast":1e300}}),
        json!({"name":"x","settings":{"hslHue":[1,2]}}),
        json!({"name":"x","settings":{"vignetteStyle":"v".repeat(100)}}),
        json!({"name":"x","settings":{"pointCurve":[[0,0],[0,10],[255,255]]}}),
        json!({"name":"x","settings":{"contrast":1},"replace":"yes"}),
        json!({"name":"x","settings":{"contrast":1},"group":"g".repeat(300)}),
    ] {
        assert!(save(&mut s, p.clone()).is_err(), "{p}");
    }
    for p in [
        json!({}),
        json!({"action":"favorite","preset":"Keep"}),
        json!({"action":"favorite","preset":"Nope","favorite":true}),
        json!({"action":"rename","preset":"Keep","name":""}),
        json!({"action":"rename","preset":"Keep","name":7}),
        json!({"action":"move","preset":"Keep","to":"t".repeat(300)}),
        json!({"action":"delete","preset":["Keep","Nope"]}),
        json!({"action":"deleteGroup","group":"Nope"}),
        json!({"action":"explode"}),
    ] {
        assert!(s.execute("cameraRaw.presets.edit", p.clone()).is_err(), "{p}");
    }
    for id in ["cameraRaw.presets.get", "cameraRaw.presets.export"] {
        for p in [json!({}), json!({"preset":"Nope"}), json!({"preset":3}), Value::Null] {
            assert!(s.execute(id, p).is_err());
        }
    }
    assert_eq!(s.prefs_value(), before);
    assert_eq!(s.prefs.rev(), rev);
}

#[test]
fn caps_bound_presets_and_groups() {
    let mut s = Session::new();
    for i in 1..MAX_GROUPS {
        s.execute("cameraRaw.presets.edit", json!({"action":"newGroup","name":format!("G{i}")})).unwrap();
    }
    assert!(s.execute("cameraRaw.presets.edit", json!({"action":"newGroup","name":"One more"})).is_err());
    assert!(save(&mut s, json!({"name":"x","group":"One more","settings":{"contrast":1}})).is_err());
    s.presets.camera_raw[0].items =
        (0..MAX_PRESETS).map(|i| CameraRawPreset { name: format!("P{i}"), favorite: false, settings: subset(&CameraRaw::default(), &["contrast"]) }).collect();
    assert!(save(&mut s, json!({"name":"x","settings":{"contrast":1}})).is_err());
}

#[test]
fn damaged_stored_presets_are_dropped_and_the_rest_kept() {
    let mut s = Session::new();
    s.edit_prefs(|p| p.general.beep_when_done = true);
    save(&mut s, json!({"name":"Good","settings":{"contrast":10}})).unwrap();
    let good = serde_json::to_value(&s.presets.camera_raw[0].items[0]).unwrap();
    let mut value = s.prefs_value();
    value["presets"]["camera_raw"] = json!([
        {"name": USER_GROUP, "items": [good, good, {"name":"Bad","settings":{"contrast":"x"}}, {"name":"Unknown","settings":{"warp":1}}, 7, {"name":"","settings":{"tint":1}}]},
        {"name": USER_GROUP, "items": []},
        {"name": "", "items": []},
        "junk",
    ]);
    let mut next = Session::new();
    next.load_prefs_json(&value.to_string()).unwrap();
    assert!(next.prefs().general.beep_when_done);
    assert_eq!(names(&next), [(USER_GROUP.into(), vec!["Good".into()])]);
    // Old preferences without Camera Raw presets start with an empty "User Presets".
    value["presets"].as_object_mut().unwrap().remove("camera_raw");
    let mut old = Session::new();
    old.load_prefs_json(&value.to_string()).unwrap();
    assert_eq!(names(&old), [(USER_GROUP.into(), vec![])]);
}
