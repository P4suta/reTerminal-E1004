use device_preview::{rgba, Preview};
use minifb::{Key, KeyRepeat, Window, WindowOptions};
use std::{
    error::Error,
    fs::{self, File},
    io::BufWriter,
    num::NonZeroU32,
    path::PathBuf,
};

fn main() -> Result<(), Box<dyn Error>> {
    let mut preview = Preview::default();
    let mut png_path = None::<PathBuf>;
    let mut raw_path = None::<PathBuf>;
    let mut remaining_frames = None::<NonZeroU32>;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                println!("{HELP}");
                return Ok(());
            }
            "--png" => png_path = Some(args.next().ok_or("--png requires a path")?.into()),
            "--raw" => raw_path = Some(args.next().ok_or("--raw requires a path")?.into()),
            "--image" => {
                let path = args.next().ok_or("--image requires a path")?;
                if fs::metadata(&path)?.len()
                    != (device_core::media::HEADER_BYTES + device_core::geometry::FRAME_BYTES)
                        as u64
                {
                    return Err("invalid prepared image length".into());
                }
                preview.import_image(1, &fs::read(path)?)?;
            }
            "--frames" => {
                remaining_frames = Some(
                    args.next()
                        .ok_or("--frames requires a positive count")?
                        .parse()?,
                )
            }
            "--replay" => preview.replay(&fs::read_to_string(
                args.next().ok_or("--replay requires a path")?,
            )?)?,
            "--event" => {
                preview.command(&args.next().ok_or("--event requires a preview command")?)?
            }
            _ => return Err(format!("unknown option: {arg}").into()),
        }
    }
    let frame = preview.frame();
    if let Some(path) = &raw_path {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, &frame)?;
    }
    if let Some(path) = &png_path {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut encoder = png::Encoder::new(
            BufWriter::new(File::create(path)?),
            u32::from(WIDTH),
            u32::from(HEIGHT),
        );
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header()?.write_image_data(&rgba(&frame))?;
        println!("PNG written: {}", path.display());
    }
    if png_path.is_some() || raw_path.is_some() {
        println!("{}", preview.text());
        return Ok(());
    }
    let mut window = Window::new(
        "reTerminal E1004 — Left / Space / Right",
        WINDOW_WIDTH,
        WINDOW_HEIGHT,
        WindowOptions::default(),
    )?;
    window.set_target_fps(WINDOW_FPS);
    while window.is_open() && !window.is_key_down(Key::Escape) {
        let held = window.is_key_down(Key::LeftShift) || window.is_key_down(Key::RightShift);
        for (key, command, hold) in [
            (Key::Left, "previous", "hold previous"),
            (Key::Space, "refresh", "hold refresh"),
            (Key::Right, "next", "hold next"),
        ] {
            if window.is_key_pressed(key, KeyRepeat::No) {
                preview.command(if held { hold } else { command })?;
            }
        }
        let rgba = rgba(&preview.frame());
        let mut scaled = vec![0u32; WINDOW_WIDTH * WINDOW_HEIGHT];
        for y in 0..WINDOW_HEIGHT {
            for x in 0..WINDOW_WIDTH {
                let i = (y * 2 * usize::from(WIDTH) + x * 2) * RGBA_CHANNELS;
                scaled[y * WINDOW_WIDTH + x] = (u32::from(rgba[i]) << RED_SHIFT)
                    | (u32::from(rgba[i + 1]) << GREEN_SHIFT)
                    | u32::from(rgba[i + 2]);
            }
        }
        window.update_with_buffer(&scaled, WINDOW_WIDTH, WINDOW_HEIGHT)?;
        if let Some(remaining) = remaining_frames {
            remaining_frames = NonZeroU32::new(remaining.get() - 1);
            if remaining_frames.is_none() {
                break;
            }
        }
    }
    println!("Native preview window closed after successful frame presentation");
    Ok(())
}
use device_core::geometry::{HEIGHT, WIDTH};
use device_preview::RGBA_CHANNELS;
const WINDOW_WIDTH: usize = WIDTH as usize / 2;
const WINDOW_HEIGHT: usize = HEIGHT as usize / 2;
const WINDOW_FPS: usize = 30;
const RED_SHIFT: u32 = 16;
const GREEN_SHIFT: u32 = 8;
const HELP: &str = "Usage: just sim [OPTIONS]\n\nOpen the Device Status preview.\nUse Left and Right to change screens, Space for the middle action, and Escape to close.\nHold Shift with an action key to simulate a held button.\n\n--png PATH       Export the complete display as PNG without opening a window.\n--raw PATH       Export the packed framebuffer without opening a window.\n--replay PATH    Replay a recorded event log before showing the display.\n--event COMMAND  Apply a preview command before showing the display.\n--frames COUNT   Close the window after a positive number of frames.\n--help, -h       Show this help.";
