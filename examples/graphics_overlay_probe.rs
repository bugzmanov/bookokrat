use std::io::{Write, stdout};
use std::thread;
use std::time::Duration;

#[cfg(feature = "pdf")]
use bookokrat::pdf::kittyv2::{DeleteCommand, DirectTransmit, Format, Quiet};
use crossterm::{
    cursor::{Hide, Show},
    execute,
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use icy_sixel::{
    DiffusionMethod, MethodForLargest, MethodForRep, PixelFormat, Quality, sixel_string,
};
use image::{DynamicImage, ImageBuffer, Rgba, RgbaImage};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Protocol {
    Iterm,
    Sixel,
    Kitty,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ClearMode {
    None,
    Spaces,
    EraseX,
}

#[derive(Debug)]
struct Config {
    protocol: Protocol,
    clear: ClearMode,
    frames: u16,
    image_w_cells: u16,
    image_h_cells: u16,
    alpha: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            protocol: Protocol::Iterm,
            clear: ClearMode::Spaces,
            frames: 120,
            image_w_cells: 60,
            image_h_cells: 22,
            alpha: true,
        }
    }
}

fn parse_args() -> Config {
    let mut cfg = Config::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--protocol" => {
                if let Some(value) = args.next() {
                    cfg.protocol = match value.as_str() {
                        "iterm" | "iterm2" => Protocol::Iterm,
                        "sixel" => Protocol::Sixel,
                        "kitty" => Protocol::Kitty,
                        _ => cfg.protocol,
                    };
                }
            }
            "--clear" => {
                if let Some(value) = args.next() {
                    cfg.clear = match value.as_str() {
                        "none" => ClearMode::None,
                        "spaces" => ClearMode::Spaces,
                        "erasex" => ClearMode::EraseX,
                        _ => cfg.clear,
                    };
                }
            }
            "--frames" => {
                if let Some(value) = args.next() {
                    if let Ok(n) = value.parse() {
                        cfg.frames = n;
                    }
                }
            }
            "--w" => {
                if let Some(value) = args.next() {
                    if let Ok(n) = value.parse() {
                        cfg.image_w_cells = n;
                    }
                }
            }
            "--h" => {
                if let Some(value) = args.next() {
                    if let Ok(n) = value.parse() {
                        cfg.image_h_cells = n;
                    }
                }
            }
            "--opaque" => cfg.alpha = false,
            _ => {}
        }
    }
    cfg
}

fn make_test_image(alpha: bool, px_w: u32, px_h: u32) -> DynamicImage {
    let mut img: RgbaImage = ImageBuffer::from_pixel(px_w, px_h, Rgba([240, 240, 240, 255]));

    for y in 0..px_h {
        for x in 0..px_w {
            if (x / 24 + y / 24) % 2 == 0 {
                let a = if alpha { 210 } else { 255 };
                img.put_pixel(x, y, Rgba([88, 133, 145, a]));
            }
        }
    }

    for x in 0..px_w {
        let y = (x as f32 * 0.45) as u32 % px_h;
        img.put_pixel(x, y, Rgba([0, 0, 0, 255]));
        if y + 1 < px_h {
            img.put_pixel(x, y + 1, Rgba([0, 0, 0, 255]));
        }
    }

    DynamicImage::ImageRgba8(img)
}

fn iterm_payload(img: &DynamicImage, w_cells: u16, h_cells: u16) -> String {
    let mut png_data = Vec::new();
    img.write_to(
        &mut std::io::Cursor::new(&mut png_data),
        image::ImageFormat::Png,
    )
    .ok();
    let data = base64_simd::STANDARD.encode_to_string(&png_data);
    format!(
        "\x1b]1337;File=inline=1;size={};width={};height={};preserveAspectRatio=0;doNotMoveCursor=1:{}\x07",
        png_data.len(),
        w_cells,
        h_cells,
        data
    )
}

fn sixel_payload(img: &DynamicImage) -> String {
    let rgb = img.to_rgb8();
    sixel_string(
        rgb.as_raw(),
        rgb.width() as i32,
        rgb.height() as i32,
        PixelFormat::RGB888,
        DiffusionMethod::Stucki,
        MethodForLargest::Auto,
        MethodForRep::Auto,
        Quality::HIGH,
    )
    .unwrap_or_default()
}

#[cfg(feature = "pdf")]
fn kitty_send(
    out: &mut impl Write,
    img: &DynamicImage,
    w_cells: u16,
    h_cells: u16,
    image_id: u32,
) -> std::io::Result<()> {
    let rgba = img.to_rgba8();
    let pixels = rgba.as_raw();
    DirectTransmit::new(rgba.width(), rgba.height())
        .format(Format::Rgba)
        .image_id(image_id)
        .quiet(Quiet::Silent)
        .no_cursor_move(true)
        .dest_cells(w_cells, h_cells)
        .send(out, pixels, false)?;
    Ok(())
}

fn draw_background(out: &mut impl Write, rows: u16, cols: u16) -> std::io::Result<()> {
    for y in 0..rows {
        write!(out, "\x1b[{};1H", y + 1)?;
        for x in 0..cols {
            let ch = if (x / 4 + y / 2) % 2 == 0 { '.' } else { ' ' };
            write!(out, "{ch}")?;
        }
    }
    Ok(())
}

fn clear_rect(
    out: &mut impl Write,
    x: u16,
    y: u16,
    w: u16,
    h: u16,
    mode: ClearMode,
) -> std::io::Result<()> {
    match mode {
        ClearMode::None => Ok(()),
        ClearMode::Spaces => {
            let blank = " ".repeat(w as usize);
            for row in 0..h {
                write!(out, "\x1b[{};{}H{}", y + row + 1, x + 1, blank)?;
            }
            Ok(())
        }
        ClearMode::EraseX => {
            for row in 0..h {
                write!(out, "\x1b[{};{}H\x1b[{}X", y + row + 1, x + 1, w)?;
            }
            Ok(())
        }
    }
}

fn main() -> std::io::Result<()> {
    let cfg = parse_args();
    let mut out = stdout();
    let (cols, rows) = terminal::size().unwrap_or((120, 40));

    execute!(out, EnterAlternateScreen, Hide, Clear(ClearType::All))?;
    write!(
        out,
        "\x1b[1;1Hprobe protocol={:?} clear={:?} alpha={}  (Ctrl+C to exit)",
        cfg.protocol, cfg.clear, cfg.alpha
    )?;
    draw_background(&mut out, rows, cols)?;
    out.flush()?;

    let px_w = (cfg.image_w_cells as u32) * 10;
    let px_h = (cfg.image_h_cells as u32) * 20;
    let image = make_test_image(cfg.alpha, px_w, px_h);
    let payload = match cfg.protocol {
        Protocol::Iterm => Some(iterm_payload(&image, cfg.image_w_cells, cfg.image_h_cells)),
        Protocol::Sixel => Some(sixel_payload(&image)),
        Protocol::Kitty => None,
    };

    let mut prev = (10u16, 4u16);
    for i in 0..cfg.frames {
        let x = 8 + ((i as i32 % 30) as u16);
        let y = 4 + (((i as f32 / 7.0).sin().abs() * 10.0) as u16);

        clear_rect(
            &mut out,
            prev.0,
            prev.1,
            cfg.image_w_cells,
            cfg.image_h_cells,
            cfg.clear,
        )?;

        #[cfg(feature = "pdf")]
        if cfg.protocol == Protocol::Kitty && cfg.clear != ClearMode::None {
            DeleteCommand::all()
                .clear()
                .quiet(Quiet::Silent)
                .write_to(&mut out, false)?;
        }

        write!(out, "\x1b[{};{}H", y + 1, x + 1)?;
        match cfg.protocol {
            Protocol::Kitty => {
                #[cfg(feature = "pdf")]
                {
                    kitty_send(
                        &mut out,
                        &image,
                        cfg.image_w_cells,
                        cfg.image_h_cells,
                        7000 + i as u32,
                    )?;
                }
                #[cfg(not(feature = "pdf"))]
                {
                    write!(out, "kitty protocol needs --features pdf")?;
                }
            }
            _ => {
                if let Some(ref seq) = payload {
                    write!(out, "{seq}")?;
                }
            }
        }
        out.flush()?;

        prev = (x, y);
        thread::sleep(Duration::from_millis(70));
    }

    write!(
        out,
        "\x1b[{};1Hdone. press Enter to exit...",
        rows.saturating_sub(1)
    )?;
    out.flush()?;
    let mut line = String::new();
    let _ = std::io::stdin().read_line(&mut line);

    execute!(out, Show, LeaveAlternateScreen)?;
    Ok(())
}
