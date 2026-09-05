use super::{Config, RenderConfig, RenderEngine};

#[test]
fn missing_renderer_fields_use_obscura_without_rewriting_explicit_scale() {
    let config: Config = toml::from_str("").unwrap();
    assert_eq!(config, Config::default());

    let legacy: Config = toml::from_str(
        r#"
        [preview]
        scale = 2
        [render]
        browser_path = "/usr/bin/custom-chromium"
        "#,
    )
    .unwrap();
    assert_eq!(
        legacy,
        Config {
            preview: super::PreviewConfig {
                scale: 2,
                ..super::PreviewConfig::default()
            },
            render: RenderConfig {
                engine: RenderEngine::Obscura,
                browser_path: "/usr/bin/custom-chromium".into(),
                ..RenderConfig::default()
            },
            ..Config::default()
        }
    );
}

#[test]
fn explicit_chromium_and_all_executable_paths_survive_round_trip() {
    let render: RenderConfig = toml::from_str(
        r#"
        engine = "chromium"
        browser_path = "/opt/chromium"
        obscura_path = "/opt/obscura"
        sandbox_path = "/opt/bwrap"
        "#,
    )
    .unwrap();
    let expected = RenderConfig {
        engine: RenderEngine::Chromium,
        browser_path: "/opt/chromium".into(),
        obscura_path: "/opt/obscura".into(),
        sandbox_path: "/opt/bwrap".into(),
        ..RenderConfig::default()
    };
    assert_eq!(render, expected);
    assert_eq!(
        toml::from_str::<RenderConfig>(&toml::to_string(&render).unwrap()).unwrap(),
        expected
    );
}

#[test]
fn unknown_engine_is_an_error_instead_of_a_fallback() {
    assert!(toml::from_str::<RenderConfig>("engine = 'other'").is_err());
}
