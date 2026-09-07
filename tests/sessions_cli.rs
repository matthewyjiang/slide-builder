use serde_json::Value;
use std::process::{Command, Output};

fn run(home: &std::path::Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_slide-builder"))
        .args(args)
        .current_dir(home)
        .env("HOME", home)
        .env("XDG_DATA_HOME", home.join("data"))
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_CACHE_HOME", home.join("cache"))
        .output()
        .unwrap()
}

fn success(output: Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn create_list_rename_continue_and_delete_without_credentials() {
    let home = tempfile::tempdir().unwrap();
    assert_eq!(
        success(run(home.path(), &["sessions", "list"])).trim(),
        "[]"
    );
    let id = success(run(home.path(), &["sessions", "new", "example.pptx"]))
        .trim()
        .to_owned();
    let deck = home.path().join("example.pptx");
    #[cfg(target_os = "macos")]
    assert!(home
        .path()
        .join("Library/Application Support/slide-builder/slide-builder.sqlite3")
        .is_file());
    assert!(deck.is_file());
    let original_deck = std::fs::read(&deck).unwrap();
    let listed: Value =
        serde_json::from_str(&success(run(home.path(), &["sessions", "list"]))).unwrap();
    assert_eq!(listed[0]["id"], id);
    assert_eq!(listed[0]["name"], "example.pptx");
    success(run(
        home.path(),
        &["sessions", "rename", &id, "Quarterly review"],
    ));
    let listed: Value =
        serde_json::from_str(&success(run(home.path(), &["sessions", "list"]))).unwrap();
    assert_eq!(listed[0]["name"], "Quarterly review");
    let resume = run(home.path(), &["sessions", "continue", &id]);
    assert!(!resume.status.success());
    assert!(String::from_utf8_lossy(&resume.stderr).contains("interactive terminal"));
    let latest = run(home.path(), &["sessions", "continue"]);
    assert!(String::from_utf8_lossy(&latest.stderr).contains("interactive terminal"));
    let missing = run(home.path(), &["sessions", "continue", "missing"]);
    assert!(String::from_utf8_lossy(&missing.stderr).contains("not found"));
    success(run(home.path(), &["sessions", "delete", &id]));
    assert_eq!(std::fs::read(&deck).unwrap(), original_deck);
    assert_eq!(
        success(run(home.path(), &["sessions", "list"])).trim(),
        "[]"
    );
    assert!(!run(home.path(), &["sessions", "continue"]).status.success());
    assert!(!run(home.path(), &["sessions", "delete", &id])
        .status
        .success());
    assert!(!run(home.path(), &["sessions", "list", "extra"])
        .status
        .success());
}

#[test]
fn missing_deck_is_not_recreated_by_resume() {
    let home = tempfile::tempdir().unwrap();
    let id = success(run(home.path(), &["sessions", "new", "original.pptx"]))
        .trim()
        .to_owned();
    std::fs::rename(
        home.path().join("original.pptx"),
        home.path().join("moved.pptx"),
    )
    .unwrap();
    let result = run(home.path(), &["sessions", "continue", &id]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("saved deck is missing"));
    assert!(!home.path().join("original.pptx").exists());
}
