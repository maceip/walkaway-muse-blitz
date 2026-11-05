//! Font-coverage probe: renders candidate toolbar glyphs through the same
//! Vello pipeline the browser uses, so we can see which ones actually resolve
//! instead of discovering tofu at runtime.
//!
//!   cargo run --example glyphs -- /tmp/glyphs.png

use anyrender::ImageRenderer;
use anyrender_vello::VelloImageRenderer;
use blitz_dom::DocumentConfig;
use blitz_html::HtmlDocument;
use blitz_traits::shell::{ColorScheme, Viewport};

fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/glyphs.png".into());
    let width = 900u32;
    let height = 260u32;

    let candidates = [
        ("2190", "&#8592;"),  // ←
        ("2192", "&#8594;"),  // →
        ("21BB", "&#8635;"),  // ↻
        ("27F3", "&#10227;"), // ⟳
        ("21BA", "&#8634;"),  // ↺
        ("27F2", "&#10226;"), // ⟲
        ("2039", "&#8249;"),  // ‹
        ("203A", "&#8250;"),  // ›
        ("00D7", "&#215;"),   // ×
        ("2715", "&#10005;"), // ✕
        ("21E8", "&#8680;"),  // ⇨
        ("2934", "&#10548;"), // ⤴
    ];

    let mut cells = String::new();
    for (code, entity) in candidates {
        cells.push_str(&format!(
            r#"<div class="cell"><div class="glyph">{entity}</div><div class="label">{code}</div></div>"#
        ));
    }

    let html = format!(
        r#"<!DOCTYPE html><html><head><style>
        * {{ margin:0; padding:0; box-sizing:border-box; }}
        body {{
          background-color:#FFFFFF; padding:20px;
          font-family:"Optimistic VF",Montserrat,"Helvetica Neue",Helvetica,Arial,sans-serif;
          display:flex; flex-direction:row; flex-wrap:wrap; gap:10px;
        }}
        .cell {{
          width:130px; height:100px; background-color:#F0F2F5; border-radius:12px;
          display:flex; flex-direction:column; align-items:center; justify-content:center; gap:6px;
        }}
        .glyph {{ font-size:26px; color:#1C2B33; }}
        .label {{ font-size:11px; color:#65676B; }}
        </style></head><body>{cells}</body></html>"#
    );

    let viewport = Viewport::new(width, height, 1.0, ColorScheme::Light);
    let mut doc = HtmlDocument::from_html(
        &html,
        DocumentConfig {
            viewport: Some(viewport),
            ..Default::default()
        },
    );
    doc.resolve(0.0);

    let mut renderer = VelloImageRenderer::new(width, height);
    let mut buffer: Vec<u8> = Vec::new();
    renderer.render_to_vec(
        |painter| {
            blitz_paint::paint_scene(painter, &mut doc, 1.0, width, height, 0, 0);
        },
        &mut buffer,
    );

    let expected = (width * height * 4) as usize;
    let file = std::fs::File::create(&path).expect("create png");
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("png header");
    writer
        .write_image_data(&buffer[..expected])
        .expect("png write");
    println!("wrote {path}");
}
