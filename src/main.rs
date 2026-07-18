use anyhow::{Context, Result};
use crossterm::{
    event::EventStream,
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures::StreamExt;
use slide_builder::{
    agent::deck_engine::DeckEngine,
    tui::{App, AppAction, AppEvent},
};
use std::{
    io::{self, IsTerminal},
    path::PathBuf,
    time::Duration,
};

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let first = args.next().map(PathBuf::from);
    if first.as_deref() == Some(std::path::Path::new("--help")) {
        print_help();
        return Ok(());
    }
    if first.as_deref() == Some(std::path::Path::new("new")) {
        let p = args
            .next()
            .map(PathBuf::from)
            .context("new requires a destination .pptx")?;
        DeckEngine::create(&p, None).await?;
        println!("created {}", p.display());
        return Ok(());
    }
    if first.as_deref() == Some(std::path::Path::new("inspect")) {
        let p = args
            .next()
            .map(PathBuf::from)
            .context("inspect requires a .pptx")?;
        let e = DeckEngine::new(p)?;
        println!("{}", serde_json::to_string_pretty(&e.inspect(None).await?)?);
        return Ok(());
    }
    let Some(deck) = first else {
        print_help();
        return Ok(());
    };
    let engine = if deck.exists() {
        DeckEngine::new(&deck)?
    } else {
        DeckEngine::create(&deck, None).await?
    };
    if !io::stdout().is_terminal() {
        println!("{}", engine.snapshot().await?.outline);
        return Ok(());
    }
    run_tui(engine).await
}
fn print_help() {
    println!("slide-builder\n\nUSAGE:\n  slide-builder new DECK.pptx\n  slide-builder inspect DECK.pptx\n  slide-builder DECK.pptx\n\nThe interactive UI requires Kitty or Ghostty and Chromium for previews.")
}
async fn run_tui(engine: DeckEngine) -> Result<()> {
    enable_raw_mode()?;
    let mut out = io::stdout();
    execute!(out, EnterAlternateScreen)?;
    let backend = ratatui::backend::CrosstermBackend::new(out);
    let mut terminal = ratatui::Terminal::new(backend)?;
    let mut app = App::default();
    app.deck_name = engine
        .path()
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into();
    let snapshot = engine.snapshot().await?;
    app.preview.status = slide_builder::tui::PreviewStatus::Unavailable {
        reason: "Press Ctrl+R after configuring a Chromium renderer".into(),
    };
    app.transcript
        .push(slide_builder::tui::TranscriptItem::Message(
            slide_builder::tui::Message {
                role: slide_builder::tui::Role::System,
                text: format!("Deck opened and validated.\n{}", snapshot.outline),
                complete: true,
            },
        ));
    let result=async {let mut input=EventStream::new();loop{terminal.draw(|f|slide_builder::tui::render(f,&app))?;tokio::select!{event=input.next()=>if let Some(Ok(event))=event{for action in app.apply(AppEvent::Input(event)){match action{AppAction::Quit=>return Ok(()),AppAction::RequestRender=>app.preview.status=slide_builder::tui::PreviewStatus::Unavailable{reason:"Renderer is not configured".into()},_=>{}}}},_=tokio::time::sleep(Duration::from_millis(100))=>{}}}}.await;
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}
