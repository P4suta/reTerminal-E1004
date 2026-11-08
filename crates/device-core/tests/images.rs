use device_core::{
    geometry::{Color, Rotation, FRAME_BYTES},
    image::{Anchor, Conversion, Dither, Fit, Palette, Placement, RgbaImage, Size},
};

#[test]
fn covered_images_still_clip_to_the_target_and_ordered_dither_keeps_its_spatial_pattern() {
    let target = Size::new(12, 16).unwrap();
    let wide = Placement::new(Size::new(5, 3).unwrap(), target, Fit::Cover, Anchor::Center);
    let tall = Placement::new(Size::new(3, 5).unwrap(), target, Fit::Cover, Anchor::Center);
    assert_eq!(wide.source(12, 8), None);
    assert_eq!(tall.source(6, 16), None);
    let original = Placement::new(
        Size::new(3, 5).unwrap(),
        target,
        Fit::Original,
        Anchor::Center,
    );
    assert_eq!(original.source(4, 5), Some((0, 0)));
    assert_eq!(original.source(6, 9), Some((2, 4)));
    for (x, y) in [(3, 5), (4, 4), (7, 5), (4, 10)] {
        assert_eq!(original.source(x, y), None);
    }
    let conversion =
        Conversion::new(Palette::Monochrome, Dither::Ordered, 0, 100, 100, false).unwrap();
    assert_eq!(conversion.pixel([128, 128, 128, 255], 0, 0), Color::Black);
    assert_eq!(conversion.pixel([128, 128, 128, 255], 1, 0), Color::White);
}

#[test]
fn six_color_conversion_preserves_every_wire_color_and_composites_transparency() {
    let colors = [
        (Color::Black, [0, 0, 0]),
        (Color::White, [255, 255, 255]),
        (Color::Yellow, [255, 255, 0]),
        (Color::Red, [255, 0, 0]),
        (Color::Blue, [0, 0, 255]),
        (Color::Green, [0, 255, 0]),
    ];
    let conversion = Conversion::default();
    for (color, rgb) in colors {
        assert_eq!(Color::from_code(color as u8), Some(color));
        assert_eq!(color.rgb(), rgb);
        assert_eq!(conversion.pixel([rgb[0], rgb[1], rgb[2], 255], 0, 0), color);
        assert_eq!(
            conversion.pixel([rgb[0], rgb[1], rgb[2], 0], 0, 0),
            Color::White
        );
    }
    for code in 0..16 {
        assert_eq!(
            Color::from_code(code).is_some(),
            [0, 1, 2, 3, 5, 6].contains(&code)
        );
    }
    assert_eq!(
        Conversion::new(Palette::Monochrome, Dither::None, 0, 100, 100, false)
            .unwrap()
            .pixel([255, 0, 0, 255], 0, 0),
        Color::Black
    );
}

#[test]
fn placement_preserves_aspect_ratio_clips_originals_and_maps_every_anchor() {
    let source = Size::new(400, 200).unwrap();
    let target = Size::new(100, 100).unwrap();
    let contain = Placement::new(source, target, Fit::Contain, Anchor::Center);
    assert_eq!(contain.rectangle(), (0, 25, 100, 50));
    assert_eq!(contain.source(0, 24), None);
    assert_eq!(contain.source(0, 25), Some((0, 0)));
    assert_eq!(contain.source(99, 74), Some((396, 196)));
    assert_eq!(contain.source(100, 50), None);
    let cover = Placement::new(source, target, Fit::Cover, Anchor::Center);
    assert_eq!(cover.rectangle(), (-50, 0, 200, 100));
    assert_eq!(cover.source(0, 0), Some((100, 0)));
    let original = Placement::new(source, target, Fit::Original, Anchor::BottomRight);
    assert_eq!(original.rectangle(), (-300, -100, 400, 200));
    assert_eq!(original.source(99, 99), Some((399, 199)));
    for (anchor, x, y) in [
        (Anchor::TopLeft, 0, 0),
        (Anchor::TopCenter, 0, 0),
        (Anchor::TopRight, 0, 0),
        (Anchor::MiddleLeft, 0, 25),
        (Anchor::Center, 0, 25),
        (Anchor::MiddleRight, 0, 25),
        (Anchor::BottomLeft, 0, 50),
        (Anchor::BottomCenter, 0, 50),
        (Anchor::BottomRight, 0, 50),
    ] {
        let placed = Placement::new(source, target, Fit::Contain, anchor);
        let (actual_x, actual_y, _, _) = placed.rectangle();
        assert_eq!((actual_x, actual_y), (x, y), "{anchor:?}");
    }
    assert!(Size::new(0, 1).is_none());
    assert!(Size::new(1, 0).is_none());
    assert!(Size::new(16385, 1).is_none());
}

#[test]
fn rgba_validation_and_packed_rendering_keep_bounds_and_rotation() {
    let source = Size::new(2, 1).unwrap();
    let rgba = [255, 0, 0, 255, 0, 0, 255, 255];
    assert!(RgbaImage::new(source, &rgba[..7]).is_none());
    let image = RgbaImage::new(source, &rgba).unwrap();
    for rotation in [
        Rotation::Portrait,
        Rotation::Landscape,
        Rotation::InvertedPortrait,
        Rotation::InvertedLandscape,
    ] {
        let (width, height) = rotation.dimensions();
        let placement = Placement::new(
            source,
            Size::new(width, height).unwrap(),
            Fit::Stretch,
            Anchor::Center,
        );
        let mut frame = vec![0xff; FRAME_BYTES];
        image
            .render(
                &mut frame,
                rotation,
                placement,
                Conversion::default(),
                &mut [],
            )
            .unwrap();
        for (x, expected) in [(0, Color::Red), (width - 1, Color::Blue)] {
            let (px, py) = rotation.physical(x, height - 1).unwrap();
            let (index, high) = device_core::geometry::pixel_address(px, py).unwrap();
            assert_eq!(
                if high {
                    frame[index] >> 4
                } else {
                    frame[index] & 15
                },
                expected as u8
            );
        }
        assert!(frame
            .iter()
            .all(|byte| Color::from_code(byte >> 4).is_some()
                && Color::from_code(byte & 15).is_some()));
    }
}

#[test]
fn ordered_and_error_diffusion_dithering_preserve_shading_without_invalid_codes() {
    let ordered =
        Conversion::new(Palette::Monochrome, Dither::Ordered, 0, 100, 100, false).unwrap();
    let white = (0..8)
        .flat_map(|y| (0..8).map(move |x| (x, y)))
        .filter(|(x, y)| ordered.pixel([128, 128, 128, 255], *x, *y) == Color::White)
        .count();
    assert_eq!(white, 32);
    assert!(Conversion::new(Palette::SixColor, Dither::None, 101, 100, 100, false).is_none());
    assert!(Conversion::new(Palette::SixColor, Dither::None, 0, 201, 100, false).is_none());
    assert!(Conversion::new(Palette::SixColor, Dither::None, 0, 100, 201, false).is_none());
    let source = Size::new(1, 1).unwrap();
    let image = RgbaImage::new(source, &[128, 128, 128, 255]).unwrap();
    let rotation = Rotation::Portrait;
    let target = Size::new(1200, 1600).unwrap();
    let placement = Placement::new(source, target, Fit::Stretch, Anchor::Center);
    let conversion = Conversion::new(
        Palette::Monochrome,
        Dither::FloydSteinberg,
        0,
        100,
        100,
        false,
    )
    .unwrap();
    let mut frame = vec![0x55; FRAME_BYTES];
    assert!(image
        .render(&mut frame, rotation, placement, conversion, &mut [])
        .is_err());
    assert!(frame.iter().all(|byte| *byte == 0x55));
    let mut errors = vec![[0; 3]; 2 * (1200 + 2)];
    image
        .render(&mut frame, rotation, placement, conversion, &mut errors)
        .unwrap();
    let white: usize = frame
        .iter()
        .map(|byte| usize::from(byte >> 4 == 1) + usize::from(byte & 15 == 1))
        .sum();
    assert!((950_000..=980_000).contains(&white));
    assert!(frame
        .iter()
        .all(|byte| matches!(byte >> 4, 0 | 1) && matches!(byte & 15, 0 | 1)));
}
