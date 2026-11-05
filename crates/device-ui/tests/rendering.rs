use device_core::{app::App, geometry::*};
use device_ui::*;

#[test]
fn text_capacity_failure_replaces_partial_content_with_a_visible_error() {
    use core::fmt::Write;
    let mut value = TextBuffer::<4>::default();
    write!(value, "okay").unwrap();
    assert_eq!(value.as_str(), "okay");
    assert!(write!(value, "overflow").is_err());
    assert_eq!(value.as_str(), "TEXT OVERFLOW");
}

#[test]
fn framebuffer_keeps_both_nibbles_and_clips_all_rotations() {
    for rotation in [
        Rotation::Portrait,
        Rotation::Landscape,
        Rotation::InvertedPortrait,
        Rotation::InvertedLandscape,
    ] {
        let mut bytes = vec![0x11; FRAME_BYTES];
        let mut target = Framebuffer::new(&mut bytes, rotation).unwrap();
        target.put(0, 0, Color::Red);
        let (x, y) = rotation.physical(0, 0).unwrap();
        let (index, high) = pixel_address(x, y).unwrap();
        assert_eq!(target.bytes()[index], if high { 0x31 } else { 0x13 });
        let before = target.bytes().to_vec();
        target.put(u16::MAX, u16::MAX, Color::Black);
        assert_eq!(target.bytes(), before);
    }
    assert!(matches!(
        Framebuffer::new(&mut [0; 3], Rotation::Portrait),
        Err(DrawError::BufferLength)
    ));
}

#[test]
fn status_screen_draws_black_content_and_is_deterministic() {
    let app = App::default();
    let mut first = vec![0; FRAME_BYTES];
    let mut second = vec![0xff; FRAME_BYTES];
    render(
        &app,
        &mut Framebuffer::new(&mut first, Rotation::Portrait).unwrap(),
    );
    render(
        &app,
        &mut Framebuffer::new(&mut second, Rotation::Portrait).unwrap(),
    );
    assert_eq!(first, second);
    assert!(first.contains(&0));
    assert!(first
        .iter()
        .all(|b| matches!(*b, 0x00 | 0x01 | 0x10 | 0x11)));
}

#[test]
fn bitmap_demo_is_monochrome_and_malformed_inputs_are_rejected() {
    let mut bytes = vec![0x11; FRAME_BYTES];
    let mut target = Framebuffer::new(&mut bytes, Rotation::Portrait).unwrap();
    draw_bmp(
        &mut target,
        &DEMO_BMP,
        embedded_graphics::prelude::Point::new(0, 0),
    )
    .unwrap();
    assert_eq!(target.bytes()[0] >> 4, Color::Black as u8);
    assert_eq!(target.bytes()[8] >> 4, Color::White as u8);
    for end in 0..54 {
        assert_eq!(
            draw_bmp(
                &mut target,
                &DEMO_BMP[..end],
                embedded_graphics::prelude::Point::zero()
            ),
            Err(DrawError::InvalidBitmap)
        );
    }
    draw_bmp(
        &mut target,
        &DEMO_BMP,
        embedded_graphics::prelude::Point::new(i32::MAX, i32::MAX),
    )
    .unwrap();
    let mut app = App::default();
    app.screen = device_core::app::ScreenId::Drawing;
    render(&app, &mut target);
    let (address, high) = pixel_address(1000, 410).unwrap();
    assert_eq!(
        if high {
            target.bytes()[address] >> 4
        } else {
            target.bytes()[address] & 15
        },
        Color::Black as u8
    );
}
