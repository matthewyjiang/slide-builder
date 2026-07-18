use anyhow::{bail, Context, Result};
use base64::Engine as _;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, EventStream},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures::StreamExt;
use notify::{EventKind, Watcher};
use slide_builder::{
    agent::{
        deck_engine::DeckEngine,
        policy::{PermissionMode, SlidePolicy},
        runtime::{build_rho, AgentHandle},
        tools::UiToolCommand,
    },
    config::{Config, PermissionMode as ConfigPermissionMode},
    design_import_workflow::{
        DesignImportRequest, DesignImportWorkflow, DesignImportWorkflowEvent,
        DesignImportWorkflowStage,
    },
    paths::AppPaths,
    prompt::{self, PromptContext},
    render::{
        browser::{Browser, CaptureOptions},
        cache::{CacheKey, RenderCache},
        pipeline::{handler_slide_count, BrowserPipeline, HANDLER_REVISION, RENDERER_VERSION},
        RenderEvent, RenderRequest, RenderService,
    },
    tui::{
        App, AppAction, AppEvent, ApprovalDecision, ApprovalRequest, ImportDesignStage,
        PreviewImage, RenderManifest, SlideRender,
    },
};
use std::{
    collections::HashMap,
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let first = args.next().map(PathBuf::from);
    if first.as_deref() == Some(Path::new("--help")) {
        print_help();
        return Ok(());
    }
    if first.as_deref() == Some(Path::new("new")) {
        let p = args
            .next()
            .map(PathBuf::from)
            .context("new requires a destination .pptx")?;
        DeckEngine::create(&p, None).await?;
        println!("created {}", p.display());
        return Ok(());
    }
    if first.as_deref() == Some(Path::new("inspect")) {
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
        engine.snapshot().await?;
        println!("Deck loaded successfully.");
        return Ok(());
    }
    run_tui(engine).await
}

fn print_help() {
    println!("slide-builder\n\nUSAGE:\n  slide-builder new DECK.pptx\n  slide-builder inspect DECK.pptx\n  slide-builder DECK.pptx\n\nThe interactive UI requires Kitty or Ghostty and Chromium for previews.")
}

fn run_model_setup(provider: &str) -> Result<String> {
    use crossterm::event::{read, Event, KeyCode, KeyEventKind};
    use ratatui::{
        layout::{Constraint, Flex, Layout},
        style::{Color, Style},
        text::{Line, Text},
        widgets::{Block, Borders, Clear, Paragraph, Wrap},
    };

    let mut model =
        rho_providers::model::catalog::default_model_for_provider(provider).unwrap_or_default();
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let mut terminal = ratatui::Terminal::new(ratatui::backend::CrosstermBackend::new(stdout))?;
    let result = (|| -> Result<String> {
        loop {
            terminal.draw(|frame| {
                let area = frame.area();
                let vertical = Layout::vertical([Constraint::Length(11)])
                    .flex(Flex::Center)
                    .split(area)[0];
                let popup = Layout::horizontal([Constraint::Length(72)])
                    .flex(Flex::Center)
                    .split(vertical)[0];
                frame.render_widget(Clear, popup);
                let body = Text::from(vec![
                    Line::from(format!("Choose a model for provider {provider}:")),
                    Line::from(""),
                    Line::styled(model.as_str(), Style::default().fg(Color::Cyan)),
                    Line::from(""),
                    Line::styled(
                        "Enter: save and continue · Esc: cancel",
                        Style::default().fg(Color::DarkGray),
                    ),
                ]);
                frame.render_widget(
                    Paragraph::new(body).wrap(Wrap { trim: true }).block(
                        Block::default()
                            .title(" model setup ")
                            .borders(Borders::ALL),
                    ),
                    popup,
                );
            })?;
            if let Event::Key(key) = read()? {
                if key.kind == KeyEventKind::Release {
                    continue;
                }
                match key.code {
                    KeyCode::Enter if !model.trim().is_empty() => {
                        break Ok(model.trim().to_string());
                    }
                    KeyCode::Esc => break Err(anyhow::anyhow!("model setup cancelled")),
                    KeyCode::Backspace => {
                        model.pop();
                    }
                    KeyCode::Char(character) if !character.is_control() => model.push(character),
                    _ => {}
                }
            }
        }
    })();
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

fn provider_descriptor(
    provider: &str,
) -> Option<&'static rho_providers::provider::ProviderDescriptor> {
    rho_providers::provider::provider_descriptor(provider)
}

fn missing_provider_credential(error: &anyhow::Error) -> bool {
    use rho_providers::ModelError;

    matches!(
        error.downcast_ref::<ModelError>(),
        Some(
            ModelError::MissingApiKey
                | ModelError::MissingCodexAuth
                | ModelError::MissingAnthropicApiKey
                | ModelError::MissingGithubCopilotAuth
                | ModelError::MissingXaiApiKey
                | ModelError::MissingXaiAuth
                | ModelError::MissingMoonshotApiKey
                | ModelError::MissingOpenRouterApiKey
                | ModelError::MissingKimiAuth
        )
    )
}

async fn run_provider_login(provider: &str, diagnostic: &str) -> Result<()> {
    use rho_providers::{
        auth::{codex_oauth, github_copilot_device, kimi_oauth, xai_oauth},
        provider::ProviderAuthKind,
    };

    let descriptor = provider_descriptor(provider)
        .with_context(|| format!("unsupported provider {provider}"))?;
    match descriptor.auth_kind {
        ProviderAuthKind::ApiKey { .. } => run_api_key_login(provider, diagnostic),
        ProviderAuthKind::CodexOAuth { .. } => {
            let login = codex_oauth::start_codex_device_login().await?;
            let verification_uri = login.verification_uri.clone();
            let user_code = login.user_code.clone();
            let tokens = show_device_login(
                descriptor.login_label,
                diagnostic,
                &verification_uri,
                &user_code,
                codex_oauth::complete_codex_device_login(login),
            )
            .await?;
            slide_builder::credentials::save_codex_tokens(&tokens)
        }
        ProviderAuthKind::GithubCopilotDevice { .. } => {
            let login = github_copilot_device::start_github_copilot_device_login().await?;
            let verification_uri = login.verification_uri.clone();
            let user_code = login.user_code.clone();
            let tokens = show_device_login(
                descriptor.login_label,
                diagnostic,
                &verification_uri,
                &user_code,
                github_copilot_device::complete_github_copilot_device_login(login),
            )
            .await?;
            slide_builder::credentials::save_github_copilot_tokens(&tokens)
        }
        ProviderAuthKind::KimiOAuth { .. } => {
            let login = kimi_oauth::start_kimi_device_login().await?;
            let verification_uri = login.verification_uri.clone();
            let user_code = login.user_code.clone();
            let tokens = show_device_login(
                descriptor.login_label,
                diagnostic,
                &verification_uri,
                &user_code,
                kimi_oauth::complete_kimi_device_login(login),
            )
            .await?;
            slide_builder::credentials::save_kimi_tokens(&tokens)
        }
        ProviderAuthKind::XaiOAuth { .. } => {
            let login = xai_oauth::start_xai_device_login().await?;
            let verification_uri = login.verification_uri.clone();
            let user_code = login.user_code.clone();
            let tokens = show_device_login(
                descriptor.login_label,
                diagnostic,
                &verification_uri,
                &user_code,
                xai_oauth::complete_xai_device_login(login),
            )
            .await?;
            slide_builder::credentials::save_xai_tokens(&tokens)
        }
    }
}

async fn show_device_login<T, E, F>(
    label: &str,
    diagnostic: &str,
    verification_uri: &str,
    user_code: &str,
    completion: F,
) -> Result<T>
where
    E: std::error::Error + Send + Sync + 'static,
    F: std::future::Future<Output = std::result::Result<T, E>>,
{
    use ratatui::{
        layout::{Constraint, Flex, Layout},
        style::{Color, Style},
        text::{Line, Text},
        widgets::{Block, Borders, Clear, Paragraph, Wrap},
    };

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let mut terminal = ratatui::Terminal::new(ratatui::backend::CrosstermBackend::new(stdout))?;
    terminal.draw(|frame| {
        let area = frame.area();
        let vertical = Layout::vertical([Constraint::Length(14)])
            .flex(Flex::Center)
            .split(area)[0];
        let popup = Layout::horizontal([Constraint::Length(72)])
            .flex(Flex::Center)
            .split(vertical)[0];
        frame.render_widget(Clear, popup);
        let body = Text::from(vec![
            Line::styled(
                "No slide-builder credential is available.",
                Style::default().fg(Color::Yellow),
            ),
            Line::from(diagnostic),
            Line::from(""),
            Line::from("Open this URL in a browser:"),
            Line::styled(verification_uri, Style::default().fg(Color::Cyan)),
            Line::from(""),
            Line::from("Enter this code:"),
            Line::styled(user_code, Style::default().fg(Color::Cyan)),
            Line::from(""),
            Line::styled(
                "Waiting for authorization...",
                Style::default().fg(Color::DarkGray),
            ),
        ]);
        frame.render_widget(
            Paragraph::new(body).wrap(Wrap { trim: true }).block(
                Block::default()
                    .title(format!(" {label} "))
                    .borders(Borders::ALL),
            ),
            popup,
        );
    })?;

    let result = completion.await.map_err(anyhow::Error::new);
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

/// Credential bootstrap deliberately runs inside a terminal UI instead of
/// delegating to `rho login`; slide-builder owns a separate keyring namespace.
fn run_api_key_login(provider: &str, diagnostic: &str) -> Result<()> {
    use crossterm::event::{read, Event, KeyCode, KeyEventKind};
    use ratatui::{
        layout::{Constraint, Flex, Layout},
        style::{Color, Style},
        text::{Line, Text},
        widgets::{Block, Borders, Clear, Paragraph, Wrap},
    };
    use zeroize::Zeroize;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let mut terminal = ratatui::Terminal::new(ratatui::backend::CrosstermBackend::new(stdout))?;
    let mut secret = String::new();
    let result = (|| -> Result<()> {
        loop {
            terminal.draw(|frame| {
                let area = frame.area();
                let vertical = Layout::vertical([Constraint::Length(12)])
                    .flex(Flex::Center)
                    .split(area)[0];
                let popup = Layout::horizontal([Constraint::Length(72)])
                    .flex(Flex::Center)
                    .split(vertical)[0];
                frame.render_widget(Clear, popup);
                let body = Text::from(vec![
                    Line::styled(
                        "No slide-builder credential is available.",
                        Style::default().fg(Color::Yellow),
                    ),
                    Line::from(diagnostic),
                    Line::from(""),
                    Line::from(format!("{provider} API key:")),
                    Line::styled(
                        "•".repeat(secret.chars().count()),
                        Style::default().fg(Color::Cyan),
                    ),
                    Line::from(""),
                    Line::styled(
                        "Enter: save securely · Esc: cancel",
                        Style::default().fg(Color::DarkGray),
                    ),
                ]);
                frame.render_widget(
                    Paragraph::new(body).wrap(Wrap { trim: true }).block(
                        Block::default()
                            .title(" slide-builder login ")
                            .borders(Borders::ALL),
                    ),
                    popup,
                );
            })?;
            if let Event::Key(key) = read()? {
                if key.kind == KeyEventKind::Release {
                    continue;
                }
                match key.code {
                    KeyCode::Enter if !secret.is_empty() => {
                        slide_builder::credentials::save_api_key(provider, &secret)?;
                        break Ok(());
                    }
                    KeyCode::Esc => break Err(anyhow::anyhow!("login cancelled")),
                    KeyCode::Backspace => {
                        secret.pop();
                    }
                    KeyCode::Char(character) if !character.is_control() => secret.push(character),
                    _ => {}
                }
            }
        }
    })();
    secret.zeroize();
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

async fn run_tui(engine: DeckEngine) -> Result<()> {
    let mut config = Config::load()?;
    if config.model.trim().is_empty() {
        config.model = run_model_setup(&config.provider)?;
        config.save()?;
    }
    let cwd = std::env::current_dir()?;
    let paths = AppPaths::discover()?;
    paths.create_app_dirs()?;
    let managed_design_packages = paths.design_packages_dir();
    std::fs::create_dir_all(&managed_design_packages)?;
    let skills = slide_builder::skills::discover(
        &cwd,
        &paths.skills_dir(),
        slide_builder::paths::home_dir().as_deref(),
    )?;
    let (ui_tool_tx, ui_tool_rx) = mpsc::unbounded_channel();
    let snapshot = engine.snapshot().await?;
    let slide_count = handler_slide_count(&snapshot.html) as usize;
    let deck_parent = engine.path().parent().context("deck has no parent")?;
    let prompt = prompt::assemble(&PromptContext {
        active_deck: engine.path(),
        decks_dir: deck_parent,
        repo_cwd: &cwd,
        app_data_dir: &paths.data_dir,
        design: None,
        skills: &skills,
        slide_index: 1,
        slide_count,
        deck_generation: snapshot.generation,
    })?;
    let policy_mode = match config.permission_mode {
        ConfigPermissionMode::Auto => PermissionMode::Auto,
        ConfigPermissionMode::Plan => PermissionMode::Plan,
        ConfigPermissionMode::Supervised => PermissionMode::Supervised,
    };
    let render_cache_dir = paths.render_cache_dir(&cwd)?;
    let legacy_render_cache_dir = paths.legacy_render_cache_dir(&cwd)?;
    if let Err(error) = std::fs::remove_dir_all(&legacy_render_cache_dir) {
        if error.kind() != io::ErrorKind::NotFound {
            return Err(error).context("remove legacy preview cache");
        }
    }
    let policy = SlidePolicy::new(policy_mode, deck_parent, &render_cache_dir);
    let (rho, approvals) = match build_rho(
        &config.provider,
        &config.model,
        prompt.clone(),
        &cwd,
        deck_parent,
        Some(&managed_design_packages),
        &skills,
        ui_tool_tx.clone(),
        engine.clone(),
        policy.clone(),
    ) {
        Ok(runtime) => runtime,
        Err(error) if missing_provider_credential(&error) => {
            run_provider_login(&config.provider, &error.to_string()).await?;
            build_rho(
                &config.provider,
                &config.model,
                prompt,
                &cwd,
                deck_parent,
                Some(&managed_design_packages),
                &skills,
                ui_tool_tx,
                engine.clone(),
                policy,
            )?
        }
        Err(error) => return Err(error),
    };
    let agent = AgentHandle::new(rho).await?;

    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let design_import = DesignImportWorkflow::default();
    let (import_tx, mut import_rx) = mpsc::unbounded_channel::<DesignImportWorkflowEvent>();
    pump_ui_tool_commands(ui_tool_rx, event_tx.clone());
    let _deck_watcher = watch_deck(engine.path(), event_tx.clone())?;
    let pending_approvals = Arc::new(Mutex::new(HashMap::new()));
    pump_approvals(approvals, event_tx.clone(), pending_approvals.clone());

    let render_service = make_render_service(&config, &render_cache_dir)?;
    if let Some(service) = &render_service {
        pump_render_events(service, event_tx.clone());
    }

    enable_raw_mode()?;
    let mut out = io::stdout();
    execute!(out, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = ratatui::backend::CrosstermBackend::new(out);
    let mut terminal = ratatui::Terminal::new(backend)?;
    let mut preview_image = PreviewImage::detect(&config.preview.protocol);
    let mut app = App {
        deck_name: engine
            .path()
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into(),
        model: if config.model.is_empty() {
            config.provider.clone()
        } else {
            format!("{}/{}", config.provider, config.model)
        },
        mode: format!("{:?}", config.permission_mode).to_lowercase(),
        config: config.clone(),
        ..App::default()
    };
    app.mouse.viewport = terminal.size()?.into();
    app.transcript
        .push(slide_builder::tui::TranscriptItem::Message(
            slide_builder::tui::Message {
                role: slide_builder::tui::Role::System,
                text: "Deck loaded successfully.".into(),
                complete: true,
            },
        ));
    if render_service.is_some() {
        queue_render(
            render_service.clone().unwrap(),
            engine.clone(),
            &config,
            event_tx.clone(),
        );
    } else {
        app.apply(AppEvent::RendererUnavailable(
            "No supported Chromium renderer was found".into(),
        ));
    }

    let mut pending_design_context: Option<String> = None;
    let mut import_picker_directory = cwd.clone();
    let result = async {
        let mut input = EventStream::new();
        loop {
            terminal.draw(|frame| {
                slide_builder::tui::render_with_preview(frame, &app, Some(&mut preview_image))
            })?;
            let event = tokio::select! {
                input = input.next() => match input {
                    Some(Ok(event)) => Some(AppEvent::Input(event)),
                    Some(Err(error)) => return Err(error.into()),
                    None => return Ok(()),
                },
                event = event_rx.recv() => event,
                imported = import_rx.recv() => imported.map(import_workflow_app_event),
                _ = tokio::time::sleep(Duration::from_millis(100)) => {
                    Some(AppEvent::Tick(std::time::Instant::now()))
                }
            };
            let Some(event) = event else { return Ok(()) };
            let preload_paths = match &event {
                AppEvent::RenderDone {
                    generation,
                    manifest,
                } if *generation >= app.preview.generation() => Some(
                    manifest
                        .slides
                        .iter()
                        .map(|slide| slide.image_path.clone())
                        .collect(),
                ),
                _ => None,
            };
            let actions = app.apply(event);
            if let Some(paths) = preload_paths {
                preview_image.preload_deck(paths);
            }
            for action in actions {
                match action {
                    AppAction::Quit => return Ok(()),
                    AppAction::SendMessage {
                        text,
                        attach_active_slide,
                    } => {
                        let text = match pending_design_context.take() {
                            Some(context) => format!("{context}{text}"),
                            None => text,
                        };
                        let image_path = attach_active_slide
                            .then(|| app.preview.slides.get(app.preview.active))
                            .flatten()
                            .and_then(|slide| slide.image_path.clone());
                        let handle = agent.clone();
                        let tx = event_tx.clone();
                        tokio::spawn(async move {
                            if let Err(error) = handle.send(text, image_path, tx.clone()).await {
                                let _ = tx.send(AppEvent::Run(
                                    slide_builder::tui::AgentEvent::RunFailed(format!("{error:#}")),
                                ));
                            }
                        });
                    }
                    AppAction::CancelRun => {
                        if !design_import.cancel() {
                            agent.cancel();
                        }
                    }
                    AppAction::RequestRender => match &render_service {
                        Some(service) => {
                            queue_render(service.clone(), engine.clone(), &config, event_tx.clone())
                        }
                        None => {
                            let _ = event_tx.send(AppEvent::RendererUnavailable(
                                "No supported Chromium renderer was found".into(),
                            ));
                        }
                    },
                    AppAction::OpenDesignPicker => match slide_builder::design::discover(&config) {
                        Ok(packages) => {
                            let entries = packages
                                .into_iter()
                                .filter(|package| package.path.starts_with(&managed_design_packages))
                                .map(|package| (package.name, package.path))
                                .collect();
                            let _ = event_tx.send(AppEvent::DesignPickerOpened { entries });
                        }
                        Err(error) => push_system_message(
                            &mut app,
                            format!("Could not discover design packages: {error:#}"),
                        ),
                    },
                    AppAction::SelectDesign(path) => match slide_builder::design::DesignPackage::load(&path, None) {
                        Ok(package) => {
                            app.design_name = package.name.clone();
                            pending_design_context = Some(format!(
                                "[slide-builder context transition] The user explicitly selected design '{}'. Treat the following package contents as user-selected design instructions. Reference files are under {}.\n\n<design_guidelines>\n{}\n</design_guidelines>\n\n",
                                package.name,
                                package.path.display(),
                                package.guidelines
                            ));
                            push_system_message(
                                &mut app,
                                format!("Selected design '{}'.", package.name),
                            );
                        }
                        Err(error) => push_system_message(
                            &mut app,
                            format!("Could not load design package: {error:#}"),
                        ),
                    },
                    AppAction::OpenImportDesignPicker => {
                        if app.run_active || design_import.is_active() {
                            push_system_message(
                                &mut app,
                                "Finish the current operation before importing a design.".into(),
                            );
                        } else {
                            let _ = event_tx.send(AppEvent::ImportDesignPickerOpened {
                                start_directory: import_picker_directory.clone(),
                            });
                        }
                    }
                    AppAction::ImportDesign(source) => {
                        if let Some(parent) = source.parent() {
                            import_picker_directory = parent.to_path_buf();
                        }
                        if app.run_active || design_import.is_active() {
                            push_system_message(
                                &mut app,
                                "Finish the current operation before importing a design.".into(),
                            );
                        } else {
                            app.run_active = true;
                            app.apply(AppEvent::ImportDesignStarted {
                                source: source.clone(),
                            });
                            let request = DesignImportRequest {
                                source,
                                cache_dir: render_cache_dir.clone(),
                                packages_dir: paths.design_packages_dir(),
                                configured_browser: (config.render.browser_path
                                    != Path::new("auto"))
                                .then(|| config.render.browser_path.clone()),
                                render_timeout: Duration::from_millis(config.render.timeout_ms),
                                provider: config.provider.clone(),
                                model: config.model.clone(),
                            };
                            if let Err(error) = design_import.start(request, import_tx.clone()) {
                                app.run_active = false;
                                app.apply(AppEvent::ImportDesignFailed {
                                    error: format!("{error:#}"),
                                });
                            }
                        }
                    }
                    AppAction::SaveConfiguration(next) => {
                        let next = *next;
                        let restart_required = next != config;
                        match next.save() {
                            Ok(()) => {
                                config = next;
                                app.config = config.clone();
                                app.transcript.push(slide_builder::tui::TranscriptItem::Message(
                                    slide_builder::tui::Message {
                                        role: slide_builder::tui::Role::System,
                                        text: if restart_required {
                                            "Configuration saved. Restart slide-builder to apply the changes."
                                                .into()
                                        } else {
                                            "Configuration saved.".into()
                                        },
                                        complete: true,
                                    },
                                ));
                            }
                            Err(error) => app.transcript.push(
                                slide_builder::tui::TranscriptItem::Message(
                                    slide_builder::tui::Message {
                                        role: slide_builder::tui::Role::System,
                                        text: format!("Could not save configuration: {error:#}"),
                                        complete: true,
                                    },
                                ),
                            ),
                        }
                    }
                    AppAction::RespondApproval { id, decision } => {
                        if let Some(mut pending) = pending_approvals
                            .lock()
                            .expect("approval map poisoned")
                            .remove(&id)
                        {
                            let decision = match decision {
                                ApprovalDecision::AllowOnce => rho_sdk::ApprovalDecision::AllowOnce,
                                ApprovalDecision::AllowForSession => {
                                    rho_sdk::ApprovalDecision::AllowForSession
                                }
                                ApprovalDecision::Deny => rho_sdk::ApprovalDecision::Deny {
                                    reason: "denied by user".into(),
                                },
                            };
                            let _ = pending.respond(decision);
                        }
                    }
                    AppAction::CopyText(text) => {
                        let encoded = base64::engine::general_purpose::STANDARD.encode(text);
                        write!(terminal.backend_mut(), "\x1b]52;c;{encoded}\x07")?;
                        terminal.backend_mut().flush()?;
                    }
                    AppAction::None
                    | AppAction::OpenDeckPicker
                    | AppAction::SetActiveSlide(_) => {}
                }
            }
        }
    }
    .await;
    design_import.shutdown().await;
    agent.cancel();
    if let Some(service) = &render_service {
        service.shutdown().await;
    }
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        DisableMouseCapture,
        LeaveAlternateScreen
    )?;
    terminal.show_cursor()?;
    if let Err(error) = std::fs::remove_dir_all(&render_cache_dir) {
        if error.kind() != io::ErrorKind::NotFound {
            return Err(error).context("remove temporary preview cache");
        }
    }
    result
}

fn import_workflow_app_event(event: DesignImportWorkflowEvent) -> AppEvent {
    match event {
        DesignImportWorkflowEvent::Stage(stage) => AppEvent::ImportDesignProgress {
            stage: match stage {
                DesignImportWorkflowStage::Validating | DesignImportWorkflowStage::Copying => {
                    ImportDesignStage::Reading
                }
                DesignImportWorkflowStage::Extracting
                | DesignImportWorkflowStage::RenderingPreviews
                | DesignImportWorkflowStage::Analyzing => ImportDesignStage::Analyzing,
                DesignImportWorkflowStage::ValidatingPackage => ImportDesignStage::Building,
                DesignImportWorkflowStage::Publishing => ImportDesignStage::Installing,
            },
            percent: None,
        },
        DesignImportWorkflowEvent::Completed { package_name, .. } => {
            AppEvent::ImportDesignCompleted {
                design_name: package_name,
            }
        }
        DesignImportWorkflowEvent::Failed(error) => AppEvent::ImportDesignFailed { error },
        DesignImportWorkflowEvent::Cancelled => AppEvent::ImportDesignCancelled,
    }
}

fn push_system_message(app: &mut App, text: String) {
    app.transcript
        .push(slide_builder::tui::TranscriptItem::Message(
            slide_builder::tui::Message {
                role: slide_builder::tui::Role::System,
                text,
                complete: true,
            },
        ));
}

fn pump_ui_tool_commands(
    mut receiver: mpsc::UnboundedReceiver<UiToolCommand>,
    events: mpsc::UnboundedSender<AppEvent>,
) {
    tokio::spawn(async move {
        while let Some(command) = receiver.recv().await {
            let event = match command {
                UiToolCommand::Render => AppEvent::AgentRenderRequested,
                UiToolCommand::SetActiveSlide(index) => AppEvent::AgentSetActiveSlide(index),
            };
            if events.send(event).is_err() {
                break;
            }
        }
    });
}

fn watch_deck(
    deck: &Path,
    events: mpsc::UnboundedSender<AppEvent>,
) -> Result<notify::RecommendedWatcher> {
    let deck = deck.to_path_buf();
    let directory = deck.parent().context("deck has no parent")?.to_path_buf();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if let Ok(event) = event {
            if deck_content_changed(&event, &deck) {
                let _ = events.send(AppEvent::DeckFileChanged);
            }
        }
    })?;
    // The transactional deck engine replaces the file with rename(2), so watch
    // its parent rather than an inode that disappears after the first edit.
    watcher.watch(&directory, notify::RecursiveMode::NonRecursive)?;
    Ok(watcher)
}

fn deck_content_changed(event: &notify::Event, deck: &Path) -> bool {
    matches!(
        event.kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    ) && event.paths.iter().any(|path| path == deck)
}

fn make_render_service(
    config: &Config,
    render_cache_dir: &Path,
) -> Result<Option<Arc<RenderService>>> {
    if !config.preview.enabled {
        return Ok(None);
    }
    let browser_path = if config.render.browser_path == Path::new("auto") {
        None
    } else {
        Some(config.render.browser_path.as_path())
    };
    let Ok(browser) = Browser::probe(browser_path) else {
        return Ok(None);
    };
    let width = config.preview.width;
    let height = width.saturating_mul(9) / 16;
    let options = CaptureOptions {
        width,
        height,
        scale: config.preview.scale as f32,
        timeout: Duration::from_millis(config.render.timeout_ms),
    };
    let cache = RenderCache::new(
        render_cache_dir.to_path_buf(),
        config.render.keep_generations,
    )?;
    cache.cleanup()?;
    let pipeline = BrowserPipeline::new(browser, cache, options, 4)?;
    Ok(Some(Arc::new(RenderService::new(
        Arc::new(pipeline),
        Duration::from_millis(config.render.debounce_ms),
    ))))
}

fn queue_render(
    service: Arc<RenderService>,
    engine: DeckEngine,
    config: &Config,
    events: mpsc::UnboundedSender<AppEvent>,
) {
    let width = config.preview.width;
    let height = width.saturating_mul(9) / 16;
    let scale = config.preview.scale as f32;
    tokio::spawn(async move {
        let result = async {
            let snapshot = engine.snapshot().await?;
            let deck_bytes = tokio::fs::read(engine.path()).await?;
            let slide_count = handler_slide_count(&snapshot.html);
            if slide_count == 0 {
                bail!("pptx-handler HTML contains no recognized slides");
            }
            let request = RenderRequest {
                generation: snapshot.generation,
                deck_identity: engine.path().as_os_str().as_encoded_bytes().to_vec(),
                cache_key: CacheKey::new(
                    &deck_bytes,
                    HANDLER_REVISION,
                    RENDERER_VERSION,
                    width,
                    height,
                    scale,
                )?,
                html: snapshot.html.into(),
                slide_count,
            };
            service.request(request)?;
            Ok::<_, anyhow::Error>(())
        }
        .await;
        if let Err(error) = result {
            let generation = engine.generation();
            let _ = events.send(AppEvent::RenderFailed {
                generation,
                error: format!("{error:#}"),
            });
        }
    });
}

fn pump_render_events(service: &Arc<RenderService>, events: mpsc::UnboundedSender<AppEvent>) {
    let Some(mut receiver) = service.take_events() else {
        return;
    };
    tokio::spawn(async move {
        while let Some(event) = receiver.recv().await {
            let event = match event {
                RenderEvent::Started { generation } => AppEvent::RenderStarted { generation },
                RenderEvent::Failed { generation, error } => {
                    AppEvent::RenderFailed { generation, error }
                }
                RenderEvent::Done {
                    generation,
                    product,
                } => AppEvent::RenderDone {
                    generation,
                    manifest: RenderManifest {
                        slides: product
                            .manifest
                            .slides
                            .into_iter()
                            .map(|slide| SlideRender {
                                index: slide.index.saturating_sub(1) as usize,
                                image_path: product.directory.join(slide.file),
                            })
                            .collect(),
                    },
                },
            };
            let _ = events.send(event);
        }
    });
}

#[cfg(test)]
#[path = "main_tests.rs"]
mod tests;

fn pump_approvals(
    mut receiver: rho_sdk::ApprovalRequestReceiver,
    events: mpsc::UnboundedSender<AppEvent>,
    pending: Arc<Mutex<HashMap<String, rho_sdk::PendingApproval>>>,
) {
    tokio::spawn(async move {
        while let Some(approval) = receiver.recv().await {
            let id = uuid::Uuid::new_v4().to_string();
            let request = approval.request();
            let event = AppEvent::Approval(ApprovalRequest {
                id: id.clone(),
                title: format!("Approve {:?}", request.capability().kind()),
                detail: format!(
                    "{}\n{:?}",
                    request.reason(),
                    request.capability().operation()
                ),
                allow_for_session: true,
            });
            pending
                .lock()
                .expect("approval map poisoned")
                .insert(id.clone(), approval);
            let _ = events.send(event);
            // The TUI presents one modal at a time. Do not consume another SDK
            // request until this response has been removed by the event loop.
            while pending
                .lock()
                .expect("approval map poisoned")
                .contains_key(&id)
            {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        }
    });
}
