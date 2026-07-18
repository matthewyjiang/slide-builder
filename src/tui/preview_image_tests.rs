use super::*;

#[test]
fn configured_protocol_names_are_case_insensitive() {
    assert_eq!(configured_protocol_type("Kitty"), Some(ProtocolType::Kitty));
    assert_eq!(configured_protocol_type("sixel"), Some(ProtocolType::Sixel));
    assert_eq!(configured_protocol_type("auto"), None);
    assert_eq!(configured_protocol_type("unknown"), None);
}

#[test]
fn centers_fitted_image_inside_available_area() {
    let area = Rect::new(10, 4, 40, 20);
    assert_eq!(
        centered_image_area(area, Size::new(30, 12)),
        Rect::new(15, 8, 30, 12)
    );
}

#[test]
fn clamps_fitted_image_to_available_area() {
    let area = Rect::new(2, 2, 10, 6);
    assert_eq!(
        centered_image_area(area, Size::new(20, 18)),
        Rect::new(2, 2, 10, 6)
    );
}
