use super::*;
use pretty_assertions::assert_eq;
use slide_builder::{
    config::Config,
    tui::{Message, TranscriptItem},
};
use std::path::PathBuf;

struct Fixture {
    directory: tempfile::TempDir,
    config: Config,
    store: SessionStore,
    deck: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let store = SessionStore::open(&directory.path().join("app.sqlite3")).unwrap();
        let deck = directory.path().join("deck.pptx");
        Self {
            config: Config {
                design_scan_dirs: vec![directory.path().join("designs")],
                ..Config::default()
            },
            directory,
            store,
            deck,
        }
    }

    fn package(&self, name: &str) -> PathBuf {
        let path = self.directory.path().join("designs").join(name);
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("DESIGN.md"), format!("# {name}\n\nRules.")).unwrap();
        path
    }

    fn sources(&self) -> Sources<'_> {
        Sources::new(&self.config, None)
    }
}

fn active(name: &str, path: &Path) -> Option<ActiveDesign> {
    Some(ActiveDesign {
        name: name.into(),
        path: path.into(),
    })
}

fn system_messages(app: &App) -> Vec<&str> {
    app.transcript
        .iter()
        .filter_map(|item| match item {
            TranscriptItem::Message(Message { text, .. }) => Some(text.as_str()),
            TranscriptItem::Tool(_) => None,
        })
        .collect()
}

#[test]
fn picking_a_design_remembers_it_so_a_fresh_session_restores_it() {
    let fixture = Fixture::new();
    let acme = fixture.package("Acme");
    let mut picked = App::default();
    let context = select(
        &mut picked,
        &fixture.store,
        &fixture.deck,
        fixture.sources(),
        &acme,
    )
    .unwrap();
    assert!(context.contains("The user explicitly selected design 'Acme'"));

    let mut fresh = App::default();
    let context = restore(
        &mut fresh,
        /*session*/ None,
        &fixture.store,
        &fixture.deck,
        fixture.sources(),
    )
    .unwrap();

    assert_eq!(fresh.design, active("Acme", &acme));
    assert!(context.contains("previously selected design 'Acme'"));
    assert_eq!(
        system_messages(&fresh),
        vec!["Using design 'Acme', last selected for this deck."]
    );
}

#[test]
fn unavailable_remembered_design_falls_back_without_being_forgotten() {
    let fixture = Fixture::new();
    let missing = fixture.directory.path().join("designs/missing");
    fixture
        .store
        .remember_deck_design(&fixture.deck, &missing)
        .unwrap();
    let mut app = App::default();

    let context = restore(
        &mut app,
        /*session*/ None,
        &fixture.store,
        &fixture.deck,
        fixture.sources(),
    );

    assert_eq!((context, app.design.clone()), (None, None));
    assert!(system_messages(&app)[0]
        .starts_with("The design last selected for this deck is unavailable"));
    assert_eq!(
        fixture.store.deck_design(&fixture.deck).unwrap(),
        Some(missing)
    );
}

#[test]
fn saved_sessions_keep_their_design_over_the_decks_remembered_one() {
    let fixture = Fixture::new();
    let acme = fixture.package("Acme");
    let studio = fixture.package("Studio");
    fixture
        .store
        .remember_deck_design(&fixture.deck, &studio)
        .unwrap();
    let state = SessionState {
        design_name: "Acme".into(),
        design: active("Acme", &acme),
        pending_design_context: Some("pending".into()),
        ..crate::sessions::initial_state(&fixture.deck, fixture.directory.path(), &fixture.config)
    };
    let mut app = App::default();

    let context = restore(
        &mut app,
        Some(&state),
        &fixture.store,
        &fixture.deck,
        fixture.sources(),
    );

    assert_eq!(
        (context, app.design),
        (Some("pending".into()), active("Acme", &acme))
    );
    assert!(app.transcript.is_empty());
}

#[test]
fn sessions_saved_with_only_a_design_name_find_the_package_by_name() {
    let fixture = Fixture::new();
    let acme = fixture.package("Acme");
    let state = SessionState {
        design_name: "Acme".into(),
        ..crate::sessions::initial_state(&fixture.deck, fixture.directory.path(), &fixture.config)
    };
    let mut app = App::default();

    restore(
        &mut app,
        Some(&state),
        &fixture.store,
        &fixture.deck,
        fixture.sources(),
    );

    assert_eq!(app.design, active("Acme", &acme));
}

#[test]
fn picker_marks_the_active_design_as_current() {
    let fixture = Fixture::new();
    fixture.package("Acme");
    let studio = fixture.package("Studio");
    let packages = fixture.sources().discover().unwrap();

    assert_eq!(
        picker_opened(packages, active("Studio", &studio).as_ref()),
        AppEvent::DesignPickerOpened {
            entries: vec![
                ("Acme".into(), fixture.directory.path().join("designs/Acme")),
                ("Studio".into(), studio),
            ],
            current: Some(1),
        }
    );
}
