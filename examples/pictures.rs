//! Manual check for KB-39: a scrollable document with real pictures drawn through
//! the production path (`App` + `PictureSet::sliced`), clipped to the pane.
//!
//!   cargo run --example pictures [-- --protocol iterm2|kitty|sixel|halfblocks]
//!
//! Keys: Up/Down, PgUp/PgDn, Home/End scroll; `o` toggles a popup
//! overlay over the document; `q` quits. Protocol detection runs on the terminal
//! (WezTerm resolves to iTerm2); `--protocol` forces one.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::{io, path::PathBuf};

use crossterm::event::{self, Event, KeyCode};
use image::{DynamicImage, Rgba, RgbaImage};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::Rect,
    widgets::{Block, Clear, Paragraph},
};
use ratatui_image::picker::{Picker, ProtocolType};
use readmd::{
    doc::{Document, Source},
    graphics::PictureSet,
    layout::PictureKey,
    ui::App,
};

fn rasterise_svg(path: &std::path::Path, scale: f32) -> DynamicImage {
    let data = std::fs::read(path).unwrap();
    let tree = resvg::usvg::Tree::from_data(&data, &resvg::usvg::Options::default()).unwrap();
    let size = tree.size();
    let (w, h) = (
        (size.width() * scale) as u32,
        (size.height() * scale) as u32,
    );
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h).unwrap();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let img = RgbaImage::from_raw(w, h, pixmap.take_demultiplied()).unwrap();
    DynamicImage::ImageRgba8(img)
}

fn main() -> io::Result<()> {
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sample-assets");
    let mut md =
        String::from("# Picture clipping check\n\nScroll the pictures in and out of view.\n\n");
    for i in 0..4 {
        md.push_str(&format!("Paragraph {i} above the gradient.\n\n"));
    }
    md.push_str("![gradient](gradient.png)\n\n");
    for i in 0..6 {
        md.push_str(&format!("Paragraph {i} between the pictures.\n\n"));
    }
    md.push_str("![logo](logo.svg)\n\n");
    for i in 0..30 {
        md.push_str(&format!("Paragraph {i} below.\n\n"));
    }
    let doc = Document::from_bytes_in(md.as_bytes(), Source::Markdown, Some(&assets));

    let forced = std::env::args()
        .skip_while(|a| a != "--protocol")
        .nth(1)
        .and_then(|s| match s.as_str() {
            "kitty" => Some(ProtocolType::Kitty),
            "sixel" => Some(ProtocolType::Sixel),
            "iterm2" => Some(ProtocolType::Iterm2),
            "halfblocks" => Some(ProtocolType::Halfblocks),
            _ => None,
        });

    let mut terminal: Terminal<CrosstermBackend<io::Stdout>> = ratatui::init();
    // Detection must run once raw mode and the alternate screen are on.
    let mut picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());
    if let Some(kind) = forced {
        picker.set_protocol_type(kind);
    }
    let font = picker.font_size();
    let mut app = App::new(doc, "sample-assets/pictures.md".into());
    app.set_cell((font.width, font.height));
    let mut set = PictureSet::sliced(picker, Rgba([26, 27, 38, 255]));
    set.insert(
        PictureKey::Image(assets.join("gradient.png")),
        image::open(assets.join("gradient.png")).unwrap(),
    );
    set.insert(
        PictureKey::Image(assets.join("logo.svg")),
        rasterise_svg(&assets.join("logo.svg"), 2.0),
    );
    app.set_pictures(set);

    let mut overlay = false;
    let result = (|| -> io::Result<()> {
        loop {
            terminal.draw(|f| {
                let area = f.area();
                let popup = Rect::new(
                    area.width / 4,
                    area.height / 3,
                    area.width / 2,
                    5.min(area.height),
                );
                app.set_overlays(if overlay { vec![popup] } else { vec![] });
                app.render(f);
                if overlay {
                    f.render_widget(Clear, popup);
                    f.render_widget(
                        Paragraph::new("overlay (o closes)").block(Block::bordered()),
                        popup,
                    );
                }
            })?;
            match event::read()? {
                Event::Key(k) if k.code == KeyCode::Char('o') => overlay = !overlay,
                Event::Key(k) if k.code == KeyCode::Char('q') => return Ok(()),
                Event::Key(k) => app.handle_key(k),
                Event::Mouse(m) => app.handle_mouse(m),
                Event::Resize(w, h) => app.resize(w, h),
                _ => {}
            }
        }
    })();
    ratatui::restore();
    result
}
