use std::io::{self, Write};

use anyhow::{Context, Result};
use crossterm::{
    event::{read, Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures::StreamExt;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Flex, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Terminal,
};
use rho_providers::{
    auth::login_dispatch::{
        AuthenticationFuture, AuthenticationMethod, CompletedAuthentication,
        InteractiveLoginCompletion, InteractiveLoginMode, InteractiveUserAction,
        ProviderAuthentication,
    },
    model::{catalog, provider_models, provider_models::ProviderModel},
    provider::{AuthMode, ProviderAuthKind, ProviderDescriptor, ProviderRuntime},
};
use slide_builder::{config::Config, credentials::SlideCredentialStore};
use zeroize::Zeroize;

type CrosstermTerminal = Terminal<CrosstermBackend<io::Stdout>>;

/// Owns the alt-screen session for first-run setup and reauthentication.
///
/// Interactive login, including browser-based flows, stays inside this session
/// so steps do not flicker in and out of the alternate screen.
struct TerminalSession {
    terminal: CrosstermTerminal,
}

impl TerminalSession {
    fn open() -> Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        if let Err(error) = execute!(stdout, EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(error.into());
        }
        match Terminal::new(CrosstermBackend::new(stdout)) {
            Ok(terminal) => Ok(Self { terminal }),
            Err(error) => {
                let _ = disable_raw_mode();
                let mut stdout = io::stdout();
                let _ = execute!(stdout, LeaveAlternateScreen);
                Err(error.into())
            }
        }
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
        let _ = self.terminal.backend_mut().flush();
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ModelChoice {
    id: String,
    label: String,
}

impl From<ProviderModel> for ModelChoice {
    fn from(model: ProviderModel) -> Self {
        Self {
            id: model.model,
            label: model.display_name,
        }
    }
}

impl From<&catalog::ModelCatalogEntry> for ModelChoice {
    fn from(model: &catalog::ModelCatalogEntry) -> Self {
        Self {
            id: model.model.clone(),
            label: model.display_name.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Navigation<T> {
    Selected(T),
    Back,
}

pub async fn run(config: &mut Config) -> Result<()> {
    let mut session = TerminalSession::open()?;
    'providers: loop {
        let provider = match choose_provider(&mut session.terminal)? {
            Navigation::Selected(provider) => provider,
            Navigation::Back => anyhow::bail!("setup cancelled"),
        };
        let has_auth_picker = provider_has_auth_picker(&provider)?;

        'authentication: loop {
            let auth = match choose_auth(&mut session.terminal, &provider)? {
                Navigation::Selected(auth) => auth,
                Navigation::Back => continue 'providers,
            };
            if authenticate_mode(&mut session.terminal, &provider, auth, true, None).await?
                == Navigation::Back
            {
                if has_auth_picker {
                    continue 'authentication;
                }
                continue 'providers;
            }

            let models = discover_models(&provider, auth.id).await?;
            let model = match choose_model(&mut session.terminal, &provider, &models)? {
                Navigation::Selected(model) => model,
                Navigation::Back if has_auth_picker => continue 'authentication,
                Navigation::Back => continue 'providers,
            };

            config.provider = provider;
            config.auth = auth.id.to_owned();
            config.model = model;
            config.save()?;
            return Ok(());
        }
    }
}

fn choose_provider(terminal: &mut CrosstermTerminal) -> Result<Navigation<String>> {
    let providers = rho_providers::provider::providers();
    let rows = providers
        .iter()
        .map(|provider| {
            let method = auth_summary(provider);
            format!("{:<22} {method}", provider.display_name)
        })
        .collect::<Vec<_>>();
    let selected = select(
        terminal,
        " Welcome to slide-builder ",
        &[
            "Build and refine native PowerPoint decks with an AI provider you trust.",
            "Choose a provider to connect. You will select a detected model next.",
        ],
        &rows,
        0,
        "Enter connect  ·  ↑/↓ move  ·  Esc cancel",
    )?;
    Ok(match selected {
        Navigation::Selected(index) => Navigation::Selected(providers[index].name.to_owned()),
        Navigation::Back => Navigation::Back,
    })
}

fn provider_has_auth_picker(provider: &str) -> Result<bool> {
    let descriptor = rho_providers::provider::provider_descriptor(provider)
        .with_context(|| format!("unsupported provider {provider}"))?;
    Ok(descriptor.auth_modes().count() > 1)
}

fn auth_summary(provider: &ProviderDescriptor) -> String {
    let modes = provider.auth_modes().collect::<Vec<_>>();
    match modes.as_slice() {
        [mode] => auth_kind_summary(mode.auth_kind).to_owned(),
        _ => format!("{} connection options", modes.len()),
    }
}

fn auth_kind_summary(auth_kind: ProviderAuthKind) -> &'static str {
    match auth_kind {
        ProviderAuthKind::None => "local · no sign-in",
        ProviderAuthKind::ApiKey { .. } => "API key",
        ProviderAuthKind::CodexOAuth { .. }
        | ProviderAuthKind::GithubCopilotDevice { .. }
        | ProviderAuthKind::KimiOAuth { .. }
        | ProviderAuthKind::XaiOAuth { .. }
        | ProviderAuthKind::BearerCredential { .. } => "browser sign-in",
        ProviderAuthKind::OllamaDeviceKey { .. } => "device sign-in",
    }
}

fn choose_auth(terminal: &mut CrosstermTerminal, provider: &str) -> Result<Navigation<AuthMode>> {
    let descriptor = rho_providers::provider::provider_descriptor(provider)
        .with_context(|| format!("unsupported provider {provider}"))?;
    let modes = descriptor.auth_modes().collect::<Vec<_>>();
    if let [mode] = modes.as_slice() {
        return Ok(Navigation::Selected(*mode));
    }
    let rows = modes
        .iter()
        .map(|mode| {
            format!(
                "{:<28} {}",
                mode.login_label,
                auth_kind_summary(mode.auth_kind)
            )
        })
        .collect::<Vec<_>>();
    let selected = select(
        terminal,
        " Choose how to connect ",
        &[
            descriptor.display_name,
            "This choice only affects authentication for this provider.",
        ],
        &rows,
        0,
        "Enter continue  ·  ↑/↓ move  ·  Esc back",
    )?;
    Ok(match selected {
        Navigation::Selected(index) => Navigation::Selected(modes[index]),
        Navigation::Back => Navigation::Back,
    })
}

pub async fn reauthenticate(provider: &str, auth: &str, diagnostic: Option<&str>) -> Result<()> {
    let mut session = TerminalSession::open()?;
    let descriptor = rho_providers::provider::provider_descriptor(provider)
        .with_context(|| format!("unsupported provider {provider}"))?;
    let mode = descriptor
        .auth_mode(auth)
        .with_context(|| format!("unsupported authentication mode {auth} for {provider}"))?;
    match authenticate_mode(&mut session.terminal, provider, mode, false, diagnostic).await? {
        Navigation::Selected(()) => Ok(()),
        Navigation::Back => anyhow::bail!("login cancelled"),
    }
}

async fn authenticate_mode(
    terminal: &mut CrosstermTerminal,
    provider: &str,
    auth: AuthMode,
    reuse_existing: bool,
    diagnostic: Option<&str>,
) -> Result<Navigation<()>> {
    let method = ProviderAuthentication::method(auth.id).map_err(anyhow::Error::new)?;
    match method {
        AuthenticationMethod::None => Ok(Navigation::Selected(())),
        AuthenticationMethod::ApiKey { entry_label } => {
            if reuse_existing
                && ProviderAuthentication::has_credentials(&SlideCredentialStore, auth.id)?
            {
                return Ok(Navigation::Selected(()));
            }
            api_key_login(terminal, provider, entry_label, diagnostic)
        }
        AuthenticationMethod::Interactive { .. } => {
            if reuse_existing
                && ProviderAuthentication::has_credentials(&SlideCredentialStore, auth.id)?
            {
                return Ok(Navigation::Selected(()));
            }
            interactive_login(terminal, auth.id, diagnostic).await
        }
    }
}

async fn interactive_login(
    terminal: &mut CrosstermTerminal,
    auth: &str,
    diagnostic: Option<&str>,
) -> Result<Navigation<()>> {
    let mode = if ProviderAuthentication::supports_device_login(auth) {
        InteractiveLoginMode::Device
    } else {
        InteractiveLoginMode::Browser
    };
    let login = ProviderAuthentication::start_interactive_login(auth, mode)
        .await
        .map_err(anyhow::Error::new)?;
    let provider_label = login.provider_label;
    let user_action = login.user_action;
    match login.completion {
        InteractiveLoginCompletion::Confirm(completion) => {
            match interactive_login_wait(
                terminal,
                provider_label,
                &user_action,
                completion,
                diagnostic,
            )
            .await?
            {
                Navigation::Selected(completed) => {
                    completed.save(&SlideCredentialStore)?;
                    Ok(Navigation::Selected(()))
                }
                Navigation::Back => Ok(Navigation::Back),
            }
        }
        InteractiveLoginCompletion::Unconfirmed { instruction } => confirm_external_login(
            terminal,
            provider_label,
            &user_action,
            instruction,
            diagnostic,
        ),
    }
}

fn api_key_login(
    terminal: &mut CrosstermTerminal,
    provider: &str,
    entry_label: &str,
    diagnostic: Option<&str>,
) -> Result<Navigation<()>> {
    let mut secret = String::new();
    let result = (|| -> Result<Navigation<()>> {
        loop {
            terminal.draw(|frame| {
                let mut lines = vec![Line::styled(
                    format!("Connect {provider}"),
                    Style::default().add_modifier(Modifier::BOLD),
                )];
                if let Some(diagnostic) = diagnostic.filter(|text| !text.is_empty()) {
                    lines.push(Line::from(diagnostic.to_owned()));
                }
                lines.extend([
                    Line::from("Your credential is stored in the slide-builder OS keyring."),
                    Line::from(""),
                    Line::from(format!("{entry_label}:")),
                    Line::styled(
                        "•".repeat(secret.chars().count()),
                        Style::default().fg(Color::Cyan),
                    ),
                    Line::from(""),
                    Line::styled(
                        "Enter save securely  ·  Esc back",
                        Style::default().fg(Color::DarkGray),
                    ),
                ]);
                let body = Text::from(lines);
                let popup = centered(frame.area(), 72, (body.lines.len() as u16 + 2).max(13));
                frame.render_widget(Clear, popup);
                frame.render_widget(
                    Paragraph::new(body).wrap(Wrap { trim: true }).block(
                        Block::default()
                            .title(" Authentication ")
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
                    KeyCode::Enter if !secret.trim().is_empty() => {
                        ProviderAuthentication::save_api_key(
                            &SlideCredentialStore,
                            provider,
                            secret.trim(),
                        )?;
                        return Ok(Navigation::Selected(()));
                    }
                    KeyCode::Esc => return Ok(Navigation::Back),
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
    result
}

fn cancels_authentication(key: KeyEvent) -> bool {
    key.kind != KeyEventKind::Release
        && (key.code == KeyCode::Esc
            || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL)))
}

fn interactive_login_body(
    label: &str,
    action: &InteractiveUserAction,
    status: &str,
    footer: &str,
    diagnostic: Option<&str>,
) -> Text<'static> {
    let mut lines = vec![Line::styled(
        label.to_owned(),
        Style::default().add_modifier(Modifier::BOLD),
    )];
    if let Some(diagnostic) = diagnostic.filter(|text| !text.is_empty()) {
        lines.push(Line::from(diagnostic.to_owned()));
    }
    lines.extend([
        Line::from("Complete sign-in in your browser."),
        Line::from(""),
    ]);
    match action {
        InteractiveUserAction::BrowserOpened => {
            lines.push(Line::from("A browser window was opened for sign-in."));
        }
        InteractiveUserAction::OpenUrl { url, instruction } => {
            lines.push(Line::from(instruction.clone()));
            lines.push(Line::from(""));
            lines.push(Line::styled(url.clone(), Style::default().fg(Color::Cyan)));
        }
        InteractiveUserAction::DeviceCode {
            verification_uri,
            user_code,
            ..
        } => {
            lines.push(Line::from("Open:"));
            lines.push(Line::styled(
                verification_uri.clone(),
                Style::default().fg(Color::Cyan),
            ));
            lines.push(Line::from(""));
            lines.push(Line::from("Enter code:"));
            lines.push(Line::styled(
                user_code.clone(),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ));
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::styled(
        status.to_owned(),
        Style::default().fg(Color::DarkGray),
    ));
    lines.push(Line::from(""));
    lines.push(Line::styled(
        footer.to_owned(),
        Style::default().fg(Color::DarkGray),
    ));
    Text::from(lines)
}

async fn interactive_login_wait(
    terminal: &mut CrosstermTerminal,
    label: &str,
    action: &InteractiveUserAction,
    completion: AuthenticationFuture,
    diagnostic: Option<&str>,
) -> Result<Navigation<CompletedAuthentication>> {
    terminal.draw(|frame| {
        let body = interactive_login_body(
            label,
            action,
            "Waiting for authorization…",
            "Esc or Ctrl+C back",
            diagnostic,
        );
        let popup = centered(frame.area(), 72, body.lines.len() as u16 + 2);
        frame.render_widget(Clear, popup);
        frame.render_widget(
            Paragraph::new(body).wrap(Wrap { trim: true }).block(
                Block::default()
                    .title(" Authentication ")
                    .borders(Borders::ALL),
            ),
            popup,
        );
    })?;
    let mut input = EventStream::new();
    let mut completion = completion;
    loop {
        tokio::select! {
            result = &mut completion => {
                break result
                    .map(Navigation::Selected)
                    .map_err(anyhow::Error::new);
            }
            event = input.next() => match event {
                Some(Ok(Event::Key(key))) if cancels_authentication(key) => {
                    break Ok(Navigation::Back);
                }
                Some(Ok(_)) => {}
                Some(Err(error)) => break Err(error.into()),
                None => break Err(anyhow::anyhow!("terminal input closed during login")),
            }
        }
    }
}

fn confirm_external_login(
    terminal: &mut CrosstermTerminal,
    label: &str,
    action: &InteractiveUserAction,
    instruction: &str,
    diagnostic: Option<&str>,
) -> Result<Navigation<()>> {
    loop {
        terminal.draw(|frame| {
            let body = interactive_login_body(
                label,
                action,
                instruction,
                "Enter continue  ·  Esc or Ctrl+C back",
                diagnostic,
            );
            let popup = centered(frame.area(), 72, body.lines.len() as u16 + 2);
            frame.render_widget(Clear, popup);
            frame.render_widget(
                Paragraph::new(body).wrap(Wrap { trim: true }).block(
                    Block::default()
                        .title(" Authentication ")
                        .borders(Borders::ALL),
                ),
                popup,
            );
        })?;
        if let Event::Key(key) = read()? {
            if cancels_authentication(key) {
                return Ok(Navigation::Back);
            }
            if key.kind != KeyEventKind::Release && key.code == KeyCode::Enter {
                return Ok(Navigation::Selected(()));
            }
        }
    }
}

async fn discover_models(provider: &str, auth: &str) -> Result<Vec<ModelChoice>> {
    let descriptor = rho_providers::provider::provider_descriptor(provider)
        .with_context(|| format!("unsupported provider {provider}"))?;

    let refreshed = if descriptor.supports_model_refresh() {
        // Must stay in sync with the API base ProviderBuildOptions resolves for
        // runtime construction.
        let endpoint_url = match descriptor.runtime {
            ProviderRuntime::OpenAiCompatible {
                default_api_base, ..
            } => Some(
                default_api_base
                    .parse()
                    .with_context(|| format!("invalid {provider} model endpoint"))?,
            ),
            _ => None,
        };
        let endpoint = endpoint_url
            .as_ref()
            .map(provider_models::ProviderModelEndpoint::OpenAiCompatible)
            .unwrap_or(provider_models::ProviderModelEndpoint::ProviderOwned);
        Some(
            provider_models::refresh_provider_models_with_store(
                provider,
                auth,
                &SlideCredentialStore,
                endpoint,
            )
            .await,
        )
    } else {
        None
    };

    let mut choices: Vec<ModelChoice> = match refreshed {
        Some(Ok(refresh)) => refresh.models.into_iter().map(ModelChoice::from).collect(),
        Some(Err(error)) => {
            let cached = provider_models::cached_provider_models(provider);
            if cached.is_empty() {
                return Err(anyhow::Error::new(error))
                    .context(format!("could not detect models for {provider}"));
            }
            cached.into_iter().map(ModelChoice::from).collect()
        }
        None => catalog::model_catalog()
            .iter()
            .filter(|model| model.provider == provider)
            .map(ModelChoice::from)
            .collect(),
    };

    if choices.is_empty() {
        if let Some(model) = catalog::default_model_for_provider(provider) {
            choices.push(ModelChoice {
                id: model.clone(),
                label: model,
            });
        }
    }
    if choices.is_empty() {
        anyhow::bail!("{provider} returned no available models");
    }
    Ok(unique_models_by_id(choices))
}

fn unique_models_by_id(mut choices: Vec<ModelChoice>) -> Vec<ModelChoice> {
    choices.sort_by(|left, right| left.id.cmp(&right.id).then(left.label.cmp(&right.label)));
    choices.dedup_by(|left, right| left.id == right.id);
    choices.sort_by(|left, right| left.label.cmp(&right.label).then(left.id.cmp(&right.id)));
    choices
}

fn choose_model(
    terminal: &mut CrosstermTerminal,
    provider: &str,
    models: &[ModelChoice],
) -> Result<Navigation<String>> {
    let descriptor = rho_providers::provider::provider_descriptor(provider)
        .with_context(|| format!("unsupported provider {provider}"))?;
    let rows = models
        .iter()
        .map(|model| {
            if model.label == model.id {
                model.id.clone()
            } else {
                format!("{}  {}", model.label, model.id)
            }
        })
        .collect::<Vec<_>>();
    let initial = descriptor
        .default_model
        .and_then(|default| models.iter().position(|model| model.id == default))
        .unwrap_or(0);
    let selected = select(
        terminal,
        " Choose a model ",
        &[
            descriptor.display_name,
            "Models were detected through the Rho provider catalog.",
        ],
        &rows,
        initial,
        "Enter start building  ·  ↑/↓ move  ·  Esc back",
    )?;
    Ok(match selected {
        Navigation::Selected(index) => Navigation::Selected(models[index].id.clone()),
        Navigation::Back => Navigation::Back,
    })
}

fn selection_line(row: &str, selected: bool) -> Line<'_> {
    let style = if selected {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    Line::from(vec![
        Span::styled(if selected { "› " } else { "  " }, style),
        Span::styled(row, style),
    ])
}

fn select(
    terminal: &mut CrosstermTerminal,
    title: &str,
    intro: &[&str],
    rows: &[String],
    initial: usize,
    footer: &str,
) -> Result<Navigation<usize>> {
    if rows.is_empty() {
        anyhow::bail!("nothing is available to select");
    }
    let mut selected = initial.min(rows.len() - 1);
    loop {
        terminal.draw(|frame| {
            let max_visible = frame.area().height.saturating_sub(10).max(1) as usize;
            let start = selected
                .saturating_sub(max_visible / 2)
                .min(rows.len().saturating_sub(max_visible));
            let end = (start + max_visible).min(rows.len());
            let height = (intro.len() + (end - start) + 5) as u16;
            let popup = centered(frame.area(), 78, height);
            frame.render_widget(Clear, popup);

            let mut lines = intro
                .iter()
                .map(|line| Line::from(*line))
                .collect::<Vec<_>>();
            lines.push(Line::from(""));
            lines.extend(
                rows[start..end]
                    .iter()
                    .enumerate()
                    .map(|(offset, row)| selection_line(row, start + offset == selected)),
            );
            lines.push(Line::from(""));
            lines.push(Line::styled(footer, Style::default().fg(Color::DarkGray)));
            frame.render_widget(
                Paragraph::new(Text::from(lines))
                    .wrap(Wrap { trim: false })
                    .block(Block::default().title(title).borders(Borders::ALL)),
                popup,
            );
        })?;

        if let Event::Key(key) = read()? {
            if key.kind == KeyEventKind::Release {
                continue;
            }
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => selected = selected.saturating_sub(1),
                KeyCode::Down | KeyCode::Char('j') => selected = (selected + 1).min(rows.len() - 1),
                KeyCode::Home => selected = 0,
                KeyCode::End => selected = rows.len() - 1,
                KeyCode::Enter => return Ok(Navigation::Selected(selected)),
                KeyCode::Esc => return Ok(Navigation::Back),
                _ => {}
            }
        }
    }
}

fn centered(area: ratatui::layout::Rect, width: u16, height: u16) -> ratatui::layout::Rect {
    let vertical = Layout::vertical([Constraint::Length(height.min(area.height))])
        .flex(Flex::Center)
        .split(area)[0];
    Layout::horizontal([Constraint::Length(width.min(area.width))])
        .flex(Flex::Center)
        .split(vertical)[0]
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};

    #[test]
    fn authentication_wait_accepts_standard_cancel_keys() {
        assert!(cancels_authentication(KeyEvent::new(
            KeyCode::Esc,
            KeyModifiers::NONE,
        )));
        assert!(cancels_authentication(KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
        )));
        assert!(!cancels_authentication(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        )));
    }

    #[test]
    fn selection_arrow_uses_a_fixed_gutter() {
        let backend = TestBackend::new(12, 2);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                frame.render_widget(
                    Paragraph::new(Text::from(vec![
                        selection_line("Ollama", true),
                        selection_line("OpenAI", false),
                    ]))
                    .wrap(Wrap { trim: false }),
                    frame.area(),
                );
            })
            .unwrap();
        let buffer = terminal.backend().buffer();

        assert_eq!(buffer[(0, 0)].symbol(), "›");
        assert_eq!(buffer[(0, 1)].symbol(), " ");
        assert_eq!(buffer[(2, 0)].symbol(), "O");
        assert_eq!(buffer[(2, 1)].symbol(), "O");
    }

    #[test]
    fn multi_auth_stays_nested_under_its_runtime_provider() {
        let xai = rho_providers::provider::provider_descriptor("xai").unwrap();
        assert_eq!(
            xai.auth_modes().map(|mode| mode.id).collect::<Vec<_>>(),
            vec!["xai-api-key", "xai-oauth"]
        );

        let openai = rho_providers::provider::provider_descriptor("openai").unwrap();
        let codex = rho_providers::provider::provider_descriptor("openai-codex").unwrap();
        assert_ne!(openai.id, codex.id);
        assert_eq!(openai.auth_modes().count(), 1);
        assert_eq!(codex.auth_modes().count(), 1);
    }

    #[test]
    fn provider_rows_come_from_rho_registry() {
        let providers = rho_providers::provider::providers();
        assert!(!providers.is_empty());
        assert!(providers
            .iter()
            .any(|provider| provider.name == "anthropic"));
        assert!(providers
            .iter()
            .any(|provider| provider.name == "openai-codex"));
        assert!(providers.iter().any(|provider| provider.name == "ollama"));
    }

    #[test]
    fn static_model_choices_come_from_rho_catalog() {
        let models = catalog::model_catalog()
            .iter()
            .filter(|model| model.provider == "openai-codex")
            .collect::<Vec<_>>();
        assert!(!models.is_empty());
    }

    #[test]
    fn model_choices_dedup_by_id_then_sort_by_label() {
        assert_eq!(
            unique_models_by_id(vec![
                ModelChoice {
                    id: "b".into(),
                    label: "Zeta".into(),
                },
                ModelChoice {
                    id: "a".into(),
                    label: "Zeta".into(),
                },
                ModelChoice {
                    id: "b".into(),
                    label: "Alpha".into(),
                },
            ]),
            vec![
                ModelChoice {
                    id: "b".into(),
                    label: "Alpha".into(),
                },
                ModelChoice {
                    id: "a".into(),
                    label: "Zeta".into(),
                },
            ]
        );
    }
}
