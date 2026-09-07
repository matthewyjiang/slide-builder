use super::*;

fn candidate() -> CreateAsset {
    CreateAsset {
        name: "rover marker".into(),
        brief: AssetBrief { purpose: "Identify conceptual rover".into(), style: "Solid blue, transparent background".into(), alt_text: "Conceptual rover marker".into() },
        svg: r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 50"><rect x="10" y="10" width="80" height="30" fill="#1256ab"/></svg>"##.into(),
        revises: None,
    }
}

#[test]
fn candidates_survive_reopening_and_revisions_preserve_originals() {
    let directory = tempfile::tempdir().unwrap();
    let deck = directory.path().join("deck.pptx");
    let store = AssetStore::for_deck(&deck);
    assert!(store.list().unwrap().assets.is_empty());
    let (first, _) = store.create(candidate()).unwrap();
    let original = fs::read(store.path(first.id)).unwrap();
    let store = AssetStore::for_deck(&deck);
    let mut next = candidate();
    next.revises = Some(first.id);
    next.svg = next.svg.replace("#1256ab", "#aa5511");
    let (second, _) = store.create(next).unwrap();
    assert_eq!(store.load(first.id).unwrap().summary(), first.summary());
    assert_eq!(store.load(second.id).unwrap().revises, Some(first.id));
    assert_eq!(fs::read(store.path(first.id)).unwrap(), original);
    assert_eq!(store.list().unwrap().assets.len(), 2);
    assert!(AssetStore::for_deck(&directory.path().join("other.pptx"))
        .list()
        .unwrap()
        .assets
        .is_empty());
}

#[test]
fn invalid_candidates_and_missing_revision_parent_do_not_create_store() {
    let directory = tempfile::tempdir().unwrap();
    let store = AssetStore::for_deck(&directory.path().join("deck.pptx"));
    let mut request = candidate();
    request.svg = "<svg><script/></svg>".into();
    assert!(store.create(request).is_err());
    let mut request = candidate();
    request.brief.purpose = " ".into();
    assert!(store.create(request).is_err());
    let mut request = candidate();
    request.revises = Some(Uuid::new_v4());
    assert!(store.create(request).is_err());
    let mut request = candidate();
    request.name = "x".repeat(MAX_TEXT_BYTES);
    let error = store.create(request).unwrap_err().to_string();
    assert!(error.contains("asset metadata text byte budget"), "{error}");
    assert!(error.contains(&MAX_TEXT_BYTES.to_string()), "{error}");
    assert!(!store.root().exists());
}

#[test]
fn modified_source_fails_integrity_check() {
    let directory = tempfile::tempdir().unwrap();
    let store = AssetStore::for_deck(&directory.path().join("deck.pptx"));
    let (mut record, _) = store.create(candidate()).unwrap();
    record.svg = record.svg.replace("#1256ab", "#aa5511");
    fs::write(store.path(record.id), serde_json::to_vec(&record).unwrap()).unwrap();
    assert!(store
        .load(record.id)
        .unwrap_err()
        .to_string()
        .contains("integrity check"));
    let (valid, _) = store.create(candidate()).unwrap();
    fs::write(store.root().join("notes.json"), "{}").unwrap();
    let listing = store.list().unwrap();
    assert_eq!(listing.assets, vec![valid.summary()]);
    assert_eq!(listing.warnings.len(), 2);
    assert!(listing
        .warnings
        .iter()
        .any(|warning| warning.contains("integrity check")));
    assert!(listing
        .warnings
        .iter()
        .any(|warning| warning.contains("notes.json")));
}

#[cfg(unix)]
#[test]
fn redirected_store_and_asset_files_are_rejected() {
    use std::os::unix::fs::symlink;
    let directory = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let store = AssetStore::for_deck(&directory.path().join("deck.pptx"));
    symlink(outside.path(), store.root()).unwrap();
    assert!(store.create(candidate()).is_err());
    assert!(store.list().is_err());
    assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);

    let safe_store = AssetStore::for_deck(&directory.path().join("safe.pptx"));
    fs::create_dir(safe_store.root()).unwrap();
    let id = Uuid::new_v4();
    let target = outside.path().join("record.json");
    fs::write(&target, "{}").unwrap();
    symlink(target, safe_store.path(id)).unwrap();
    assert!(safe_store
        .load(id)
        .unwrap_err()
        .to_string()
        .contains("regular file"));
}
