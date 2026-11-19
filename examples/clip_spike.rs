//! KB-7 spike: scroll a tall striped picture through a document pane that sits
//! next to a sidebar and under a popup overlay.
//!
//!   cargo run --example clip_spike -- [--protocol kitty|sixel|iterm2|halfblocks] [--bench]
//!
//! Keys: Up/Down/PgUp/PgDn scroll, `o` toggles the overlay, `q` quits.
//! `--bench` runs headless and prints encode / per-scroll costs per protocol.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::{io, time::Instant};

use image::{DynamicImage, Rgba, RgbaImage};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, Clear, Paragraph},
};
use ratatui_image::picker::{Picker, ProtocolType};
use readmd::graphics::{PictureRenderer, PictureRequest, SlicedRenderer, flatten_over};

const BG: Rgba<u8> = Rgba([30, 30, 46, 255]);

/// Stripes with a numbered-looking gradient, a transparent corner and a frame,
/// so clipping errors and transparency leaks are visible.
fn picture(w: u32, h: u32) -> DynamicImage {
    let mut img = RgbaImage::new(w, h);
    for (x, y, p) in img.enumerate_pixels_mut() {
        let band = (y / 40) % 6;
        let c = [
            [220, 80, 80],
            [220, 160, 60],
            [200, 220, 60],
            [60, 200, 100],
            [60, 140, 220],
            [160, 90, 220],
        ][band as usize];
        let edge = x < 4 || y < 4 || x >= w - 4 || y >= h - 4;
        let transparent = x > w * 3 / 4 && (y / 20) % 2 == 0;
        *p = if edge {
            Rgba([255, 255, 255, 255])
        } else if transparent {
            Rgba([0, 0, 0, 0])
        } else {
            Rgba([c[0], c[1], c[2], 255])
        };
    }
    DynamicImage::ImageRgba8(img)
}

fn parse_protocol(s: &str) -> Option<ProtocolType> {
    match s {
        "kitty" => Some(ProtocolType::Kitty),
        "sixel" => Some(ProtocolType::Sixel),
        "iterm2" => Some(ProtocolType::Iterm2),
        "halfblocks" => Some(ProtocolType::Halfblocks),
        _ => None,
    }
}

fn bench() {
    let img = picture(640, 1600);
    println!("picture 640x1600 px, cell 10x20 (64x80 cells), release build recommended");
    for (name, kind) in [
        ("kitty", ProtocolType::Kitty),
        ("sixel", ProtocolType::Sixel),
        ("iterm2", ProtocolType::Iterm2),
        ("halfblocks", ProtocolType::Halfblocks),
    ] {
        let mut picker = Picker::halfblocks();
        picker.set_protocol_type(kind);
        let t = Instant::now();
        let mut r = SlicedRenderer::new(&picker, img.clone(), BG).unwrap();
        let encode = t.elapsed();

        let mut buf = Buffer::empty(Rect::new(0, 0, 120, 40));
        let mut bytes = 0usize;
        let t = Instant::now();
        let steps = 200;
        for i in 0..steps {
            buf.reset();
            let req = PictureRequest {
                pane: Rect::new(20, 1, 99, 38),
                slot_top: 1 - (i % 70),
                slot_left: 30,
                slot_cols: 64,
                slot_rows: 80,
                overlays: &[],
            };
            r.render(&req, &mut buf);
            bytes = bytes.max(buf.content.iter().map(|c| c.symbol().len()).sum());
        }
        let per_step = t.elapsed() / steps as u32;

        // alternative: crop the RGBA ourselves and re-encode at every step
        let t = Instant::now();
        for i in 0..20u32 {
            let crop = flatten_over(&img, BG).crop_imm(0, i * 20, 640, 760);
            let _ = SlicedRenderer::new(&picker, crop, BG).unwrap();
        }
        let reencode = t.elapsed() / 20;
        println!(
            "{name:<10} encode once {encode:>9.1?}  per scroll step {per_step:>9.1?}  \
             bytes emitted/frame {bytes:>8}  crop+re-encode/step {reencode:>9.1?}"
        );
    }
}

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--bench") {
        bench();
        return Ok(());
    }
    let forced = args
        .iter()
        .position(|a| a == "--protocol")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| parse_protocol(s));

    let mut terminal = ratatui::init();
    // Detection must run after the alternate screen / raw mode are set up.
    let mut picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());
    if let Some(kind) = forced {
        picker.set_protocol_type(kind);
    }
    let font = picker.font_size();
    let img = picture(font.width as u32 * 60, font.height as u32 * 40);
    let mut renderer = SlicedRenderer::new(&picker, img, BG).unwrap();
    let slot = renderer.size();
    let protocol = picker.protocol_type();

    let (mut scroll, mut overlay) = (0i32, false);
    let result = (|| -> io::Result<()> {
        loop {
            terminal.draw(|f| {
                let [main, status] =
                    Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(f.area());
                let [side, doc] =
                    Layout::horizontal([Constraint::Length(20), Constraint::Min(1)]).areas(main);
                let paint = Style::default().bg(Color::Rgb(30, 30, 46)).fg(Color::White);
                f.render_widget(Block::new().style(paint), f.area());
                f.render_widget(
                    Paragraph::new("sidebar\n> a.md\n  b.md")
                        .block(Block::new().borders(Borders::RIGHT))
                        .style(paint),
                    side,
                );
                f.render_widget(
                    Paragraph::new(format!(
                        " {protocol:?} scroll={scroll} slot={}x{} (o overlay, q quit)",
                        slot.width, slot.height
                    ))
                    .style(Style::default().bg(Color::Blue).fg(Color::White)),
                    status,
                );
                let popup = Rect::new(doc.x + 8, doc.y + 4, 30.min(doc.width), 6.min(doc.height));
                let overlays: Vec<Rect> = if overlay { vec![popup] } else { vec![] };
                let req = PictureRequest {
                    pane: doc,
                    slot_top: i32::from(doc.y) + 2 - scroll,
                    slot_left: doc.x + doc.width.saturating_sub(slot.width) / 2,
                    slot_cols: slot.width,
                    slot_rows: slot.height,
                    overlays: &overlays,
                };
                renderer.render(&req, f.buffer_mut());
                if overlay {
                    f.render_widget(Clear, popup);
                    f.render_widget(
                        Paragraph::new("overlay")
                            .block(Block::bordered())
                            .style(paint),
                        popup,
                    );
                }
            })?;
            if let crossterm::event::Event::Key(k) = crossterm::event::read()? {
                use crossterm::event::KeyCode::*;
                match k.code {
                    Char('q') | Esc => return Ok(()),
                    Char('o') => overlay = !overlay,
                    Down => scroll += 1,
                    Up => scroll -= 1,
                    PageDown => scroll += 20,
                    PageUp => scroll -= 20,
                    _ => {}
                }
            }
        }
    })();
    ratatui::restore();
    result
}
