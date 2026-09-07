use std::path::PathBuf;

use crossterm::event::KeyEvent;

use crate::{
    config::{Config, PermissionMode, RenderEngine},
    models::AvailableModel,
};

use super::menu::{MenuEvent, MenuGroup, MenuItem, MenuState, MenuValue};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigurationState {
    pub menu: MenuState,
    original: Config,
    /// Models offered by the `model` choice; the configured model is appended
    /// when it is not among them so saving never silently changes it.
    models: Vec<AvailableModel>,
}

impl ConfigurationState {
    /// `models` are the logged-in models from `models::discover_available_models`.
    pub fn new(config: &Config, mut models: Vec<AvailableModel>) -> Self {
        if !models.iter().any(|model| model.matches_config(config)) {
            models.push(AvailableModel::from_config(config));
        }
        let model_options = models.iter().map(AvailableModel::reference).collect();
        let permission = ["auto", "plan", "supervised"];
        Self {
            original: config.clone(),
            menu: MenuState {
                title: "Configuration".into(),
                selected: 0,
                editing: false,
                status: None,
                groups: vec![
                    MenuGroup { title: "AI provider".into(), items: vec![
                        choice("model", "Model", "Models from providers you are logged in to. Applied on save.", model_options, &AvailableModel::from_config(config).reference()),
                        choice("reasoning", "Reasoning", "Reasoning effort used for new agent runs.", vec!["low".into(), "medium".into(), "high".into()], &config.reasoning),
                    ]},
                    MenuGroup { title: "Permissions & files".into(), items: vec![
                        choice("permission", "Permission mode", "supervised asks before writes; plan disallows them; auto permits them.", permission.iter().map(|s| (*s).into()).collect(), match config.permission_mode { PermissionMode::Auto => "auto", PermissionMode::Plan => "plan", PermissionMode::Supervised => "supervised" }),
                        text("decks_dir", "Decks directory", "Default directory used by the deck picker.", &config.decks_dir.to_string_lossy()),
                    ]},
                    MenuGroup { title: "Preview".into(), items: vec![
                        toggle("preview_enabled", "Enabled", "Enable inline terminal slide previews.", config.preview.enabled),
                        text("preview_protocol", "Protocol", "Use auto to detect host support, or force kitty, sixel, iterm2, or halfblocks.", &config.preview.protocol),
                        text("preview_width", "Render width", "Preview render width in pixels; must be greater than zero.", &config.preview.width.to_string()),
                        text("preview_scale", "Scale", "Output pixels per CSS pixel, from 1 to 4.\nHigher scales keep layout size; Obscura capture budgets apply.", &config.preview.scale.to_string()),
                    ]},
                    MenuGroup { title: "Renderer".into(), items: vec![
                        choice("render_engine", "Engine", "Obscura is primary; macOS is experimental.\nChromium is opt-in. No fallback. Restart required.", vec!["obscura".into(), "chromium".into()], match config.render.engine { RenderEngine::Obscura => "obscura", RenderEngine::Chromium => "chromium" }),
                        text("sandbox_path", "Sandbox path", "Obscura: bwrap on Linux, sandbox-exec on macOS.\nAbsolute path or auto. No bypass. Restart required.", &config.render.sandbox_path.to_string_lossy()),
                        text("browser_path", "Chromium path", "Chromium only: executable path, or auto.\nIgnored by Obscura. Restart required.", &config.render.browser_path.to_string_lossy()),
                        text("debounce_ms", "Debounce (ms)", "Delay before rendering after deck changes.", &config.render.debounce_ms.to_string()),
                        text("timeout_ms", "Timeout (ms)", "Rendering timeout; must be greater than zero.", &config.render.timeout_ms.to_string()),
                        text("keep_generations", "Keep generations", "Number of render-cache generations to retain.", &config.render.keep_generations.to_string()),
                    ]},
                    MenuGroup { title: "Compatibility".into(), items: vec![
                        text("officecli_path", "officecli path", "Path to the optional Office compatibility checker.", &config.compat.officecli_path.to_string_lossy()),
                        toggle("detect_optional", "Auto-detect", "Automatically detect optional compatibility tools.", config.compat.detect_optional),
                    ]},
                ],
            },
            models,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ConfigurationEvent {
        match self.menu.handle_key(key) {
            MenuEvent::Save => match self.to_config() {
                Ok(config) => ConfigurationEvent::Save(Box::new(config)),
                Err(error) => {
                    self.menu.status = Some(error);
                    ConfigurationEvent::None
                }
            },
            MenuEvent::Cancel => ConfigurationEvent::Cancel,
            _ => ConfigurationEvent::None,
        }
    }

    fn to_config(&self) -> Result<Config, String> {
        let mut config = self.original.clone();
        let reference = self.string("model")?;
        let model = self
            .models
            .iter()
            .find(|model| model.reference() == reference)
            .ok_or_else(|| format!("unknown model {reference}"))?;
        if !model.matches_config(&self.original) {
            config.provider = model.provider.clone();
            config.auth = model.auth.clone();
            config.model = model.model.clone();
        }
        config.reasoning = self.string("reasoning")?;
        config.permission_mode = match self.string("permission")?.as_str() {
            "auto" => PermissionMode::Auto,
            "plan" => PermissionMode::Plan,
            _ => PermissionMode::Supervised,
        };
        config.decks_dir = PathBuf::from(self.string("decks_dir")?);
        config.preview.enabled = self.boolean("preview_enabled")?;
        config.preview.protocol = self.string("preview_protocol")?;
        config.preview.width = self.number("preview_width")?;
        config.preview.scale = self.number("preview_scale")?;
        config.render.engine = match self.string("render_engine")?.as_str() {
            "obscura" => RenderEngine::Obscura,
            "chromium" => RenderEngine::Chromium,
            engine => return Err(format!("unsupported render engine {engine}")),
        };
        config.render.sandbox_path = PathBuf::from(self.string("sandbox_path")?);
        config.render.browser_path = PathBuf::from(self.string("browser_path")?);
        config.render.debounce_ms = self.number("debounce_ms")?;
        config.render.timeout_ms = self.number("timeout_ms")?;
        config.render.keep_generations = self.number("keep_generations")?;
        config.compat.officecli_path = PathBuf::from(self.string("officecli_path")?);
        config.compat.detect_optional = self.boolean("detect_optional")?;
        config.validate().map_err(|error| error.to_string())?;
        if config.preview.enabled {
            crate::render::browser::CaptureOptions {
                width: config.preview.width,
                height: config.preview.width.saturating_mul(9) / 16,
                scale: config.preview.scale as f32,
                timeout: std::time::Duration::from_millis(config.render.timeout_ms),
            }
            .validate_for_engine(config.render.engine)
            .map_err(|error| error.to_string())?;
        }
        Ok(config)
    }

    fn value(&self, id: &str) -> Result<&MenuValue, String> {
        self.menu
            .item(id)
            .map(|item| &item.value)
            .ok_or_else(|| format!("missing configuration field {id}"))
    }
    fn string(&self, id: &str) -> Result<String, String> {
        Ok(self.value(id)?.display())
    }
    fn boolean(&self, id: &str) -> Result<bool, String> {
        match self.value(id)? {
            MenuValue::Toggle(value) => Ok(*value),
            _ => Err(format!("{id} is not a toggle")),
        }
    }
    fn number<T: std::str::FromStr>(&self, id: &str) -> Result<T, String> {
        self.string(id)?.parse().map_err(|_| {
            format!(
                "{} must be a positive whole number",
                self.menu.item(id).map(|i| i.label.as_str()).unwrap_or(id)
            )
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfigurationEvent {
    None,
    Save(Box<Config>),
    Cancel,
}

fn text(id: &str, label: &str, help: &str, value: &str) -> MenuItem {
    MenuItem {
        id: id.into(),
        label: label.into(),
        help: help.into(),
        value: MenuValue::Text(value.into()),
    }
}
fn toggle(id: &str, label: &str, help: &str, value: bool) -> MenuItem {
    MenuItem {
        id: id.into(),
        label: label.into(),
        help: help.into(),
        value: MenuValue::Toggle(value),
    }
}
fn choice(id: &str, label: &str, help: &str, mut options: Vec<String>, value: &str) -> MenuItem {
    let selected = options
        .iter()
        .position(|option| option == value)
        .unwrap_or_else(|| {
            options.push(value.into());
            options.len() - 1
        });
    MenuItem {
        id: id.into(),
        label: label.into(),
        help: help.into(),
        value: MenuValue::Choice { options, selected },
    }
}

#[cfg(test)]
#[path = "configuration_tests.rs"]
mod tests;
