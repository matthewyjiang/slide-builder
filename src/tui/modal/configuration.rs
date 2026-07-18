use std::path::PathBuf;

use crossterm::event::KeyEvent;

use crate::config::{Config, PermissionMode};

use super::menu::{MenuEvent, MenuGroup, MenuItem, MenuState, MenuValue};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigurationState {
    pub menu: MenuState,
    original: Config,
}

impl ConfigurationState {
    pub fn new(config: &Config) -> Self {
        let providers = rho_providers::provider::providers()
            .iter()
            .map(|p| p.name.to_owned())
            .collect::<Vec<_>>();
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
                        choice("provider", "Provider", "Changing provider or model takes effect after restart.", providers, &config.provider),
                        text("model", "Model", "Exact provider model ID. Restart required after changing it.", &config.model),
                        choice("reasoning", "Reasoning", "Reasoning effort used for new agent runs.", vec!["low".into(), "medium".into(), "high".into()], &config.reasoning),
                    ]},
                    MenuGroup { title: "Permissions & files".into(), items: vec![
                        choice("permission", "Permission mode", "supervised asks before writes; plan disallows them; auto permits them.", permission.iter().map(|s| (*s).into()).collect(), match config.permission_mode { PermissionMode::Auto => "auto", PermissionMode::Plan => "plan", PermissionMode::Supervised => "supervised" }),
                        text("decks_dir", "Decks directory", "Default directory used by the deck picker.", &config.decks_dir.to_string_lossy()),
                    ]},
                    MenuGroup { title: "Preview".into(), items: vec![
                        toggle("preview_enabled", "Enabled", "Enable inline terminal slide previews.", config.preview.enabled),
                        text("preview_protocol", "Protocol", "Terminal image protocol (normally kitty).", &config.preview.protocol),
                        text("preview_width", "Render width", "Preview render width in pixels; must be greater than zero.", &config.preview.width.to_string()),
                        text("preview_scale", "Scale", "Device scale factor; must be greater than zero.", &config.preview.scale.to_string()),
                    ]},
                    MenuGroup { title: "Renderer".into(), items: vec![
                        text("browser_path", "Browser path", "Chromium executable path, or auto.", &config.render.browser_path.to_string_lossy()),
                        text("debounce_ms", "Debounce (ms)", "Delay before rendering after deck changes.", &config.render.debounce_ms.to_string()),
                        text("timeout_ms", "Timeout (ms)", "Browser rendering timeout; must be greater than zero.", &config.render.timeout_ms.to_string()),
                        text("keep_generations", "Keep generations", "Number of render-cache generations to retain.", &config.render.keep_generations.to_string()),
                    ]},
                    MenuGroup { title: "Compatibility".into(), items: vec![
                        text("officecli_path", "officecli path", "Path to the optional Office compatibility checker.", &config.compat.officecli_path.to_string_lossy()),
                        toggle("detect_optional", "Auto-detect", "Automatically detect optional compatibility tools.", config.compat.detect_optional),
                    ]},
                ],
            },
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
        config.provider = self.string("provider")?;
        config.model = self.string("model")?;
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
        config.render.browser_path = PathBuf::from(self.string("browser_path")?);
        config.render.debounce_ms = self.number("debounce_ms")?;
        config.render.timeout_ms = self.number("timeout_ms")?;
        config.render.keep_generations = self.number("keep_generations")?;
        config.compat.officecli_path = PathBuf::from(self.string("officecli_path")?);
        config.compat.detect_optional = self.boolean("detect_optional")?;
        config.validate().map_err(|error| error.to_string())?;
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
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyModifiers};

    #[test]
    fn configuration_round_trips() {
        let config = Config::default();
        assert_eq!(
            ConfigurationState::new(&config).to_config().unwrap(),
            config
        );
    }

    #[test]
    fn bad_numeric_value_is_reported_without_closing() {
        let mut state = ConfigurationState::new(&Config::default());
        if let MenuValue::Text(value) = &mut state.menu.groups[2].items[2].value {
            *value = "nope".into();
        }
        let event = state.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
        assert_eq!(event, ConfigurationEvent::None);
        assert!(state.menu.status.is_some());
    }
}
