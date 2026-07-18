use super::*;

#[test]
fn parses_terminal_background_and_ansi_palette() {
    let response = "\x1b]11;rgb:0000/0000/0000\x1b\\\x1b]4;1;rgb:ffff/0000/0000\x1b\\\x1b]4;2;rgb:0000/ffff/0000\x1b\\\x1b]4;3;rgb:ffff/ffff/0000\x1b\\\x1b]4;4;rgb:0000/0000/ffff\x1b\\\x1b]4;5;rgb:ffff/0000/ffff\x1b\\\x1b]4;6;rgb:0000/ffff/ffff\x1b\\\x1b]4;7;rgb:ffff/ffff/ffff\x1b\\";

    let palette = parse_palette_response(response).expect("palette");

    assert_eq!(palette.background, Rgb::new(0, 0, 0));
    assert_eq!(palette.ansi[&AnsiColor::Red], Rgb::new(255, 0, 0));
    assert_eq!(palette.ansi[&AnsiColor::Cyan], Rgb::new(0, 255, 255));
}

#[test]
fn accepts_bel_terminated_palette_responses() {
    let response = "\x1b]11;rgb:0000/0000/0000\x07\x1b]4;1;rgb:ffff/0000/0000\x07\x1b]4;2;rgb:0000/ffff/0000\x07\x1b]4;3;rgb:ffff/ffff/0000\x07\x1b]4;4;rgb:0000/0000/ffff\x07\x1b]4;5;rgb:ffff/0000/ffff\x07\x1b]4;6;rgb:0000/ffff/ffff\x07\x1b]4;7;rgb:ffff/ffff/ffff\x07";

    assert!(parse_palette_response(response).is_some());
}

#[test]
fn blends_ansi_colors_toward_the_terminal_background() {
    let palette = TerminalPalette {
        background: Rgb::new(10, 10, 10),
        ansi: HashMap::from([(AnsiColor::Green, Rgb::new(10, 110, 10))]),
    };

    let background = palette
        .blended_background(AnsiColor::Green, 0.16)
        .expect("green background");

    assert_eq!(background.color, Color::Rgb(10, 26, 10));
    assert!(!background.use_dark_foreground);
}

#[test]
fn identifies_light_and_dark_resolved_backgrounds() {
    assert!(relative_luminance(Rgb::new(240, 240, 240)) > 0.55);
    assert!(relative_luminance(Rgb::new(20, 20, 20)) <= 0.55);
}
