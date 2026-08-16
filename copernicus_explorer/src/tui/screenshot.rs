//! Demo-frame SVG export for README screenshots.

use super::app::{App, DownloadState, DownloadUiStatus, Pane};
use super::ui;
use crate::{Product, Satellite};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Cell;
use ratatui::style::Color;
use std::io::{self, Write};
use std::path::Path;
use std::time::{Duration, Instant};

const COLS: u16 = 120;
const ROWS: u16 = 36;
const CELL_W: f64 = 8.4;
const CELL_H: f64 = 17.0;
const PAD: f64 = 18.0;
const BG: &str = "#1a1b26";
const FG: &str = "#c0caf5";

/// Render a populated demo frame of the TUI to an SVG file.
pub fn write_demo_screenshot(path: impl AsRef<Path>) -> io::Result<()> {
    let app = demo_app();
    let backend = TestBackend::new(COLS, ROWS);
    let mut terminal = Terminal::new(backend)?;
    terminal.draw(|frame| ui::draw(frame, &app))?;

    let buffer = terminal.backend().buffer();
    let svg = buffer_to_svg(buffer);
    if let Some(parent) = path.as_ref().parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::File::create(path)?;
    file.write_all(svg.as_bytes())?;
    Ok(())
}

fn demo_app() -> App {
    let mut app = App::new();
    app.satellite = Satellite::Sentinel2;
    app.product = "L2A".to_string();
    app.start_date = "2026-03-01".to_string();
    app.end_date = "2026-03-24".to_string();
    app.cloud_cover = "30".to_string();
    app.point = "43.60,1.44".to_string();
    app.max_results = "10".to_string();
    app.focus = Pane::Results;
    app.selected_result = 1;
    app.status = "Queued 2 download(s) (max 4 concurrent)…".to_string();

    app.downloaded_ids.insert("a1".to_string());
    app.marked.insert("a3".to_string());
    app.products = vec![
        product(
            "a1",
            "S2A_MSIL2A_20260312T104031_N0511_R008_T31TCJ_20260312T141512.SAFE",
            "2026-03-12T10:40:31.000Z",
            Some(8.2),
        ),
        product(
            "a2",
            "S2B_MSIL2A_20260315T103629_N0511_R008_T31TFJ_20260315T140211.SAFE",
            "2026-03-15T10:36:29.000Z",
            Some(12.4),
        ),
        product(
            "a3",
            "S2A_MSIL2A_20260318T104031_N0511_R008_T31TCJ_20260318T142045.SAFE",
            "2026-03-18T10:40:31.000Z",
            Some(22.1),
        ),
        product(
            "a4",
            "S2B_MSIL2A_20260320T103629_N0511_R008_T31TFJ_20260320T135908.SAFE",
            "2026-03-20T10:36:29.000Z",
            Some(4.7),
        ),
        product(
            "a5",
            "S2A_MSIL2A_20260322T104031_N0511_R008_T31TCJ_20260322T141833.SAFE",
            "2026-03-22T10:40:31.000Z",
            Some(18.9),
        ),
    ];

    {
        let mut map = app.downloads.lock().unwrap();
        map.insert(
            "a2".to_string(),
            DownloadState {
                label: "S2B_MSIL2A_20260315T103629_N0511_R008_T31TFJ_20260315T140211.SAFE".into(),
                downloaded: 482_344_960,
                total: Some(1_073_741_824),
                started_at: Some(Instant::now() - Duration::from_secs(48)),
                status: DownloadUiStatus::Downloading,
            },
        );
        map.insert(
            "a4".to_string(),
            DownloadState {
                label: "S2B_MSIL2A_20260320T103629_N0511_R008_T31TFJ_20260320T135908.SAFE".into(),
                downloaded: 201_326_592,
                total: Some(988_000_000),
                started_at: Some(Instant::now() - Duration::from_secs(22)),
                status: DownloadUiStatus::Downloading,
            },
        );
        map.insert(
            "a1".to_string(),
            DownloadState {
                label: "S2A_MSIL2A_20260312T104031_N0511_R008_T31TCJ_20260312T141512.SAFE".into(),
                downloaded: 1_048_576_000,
                total: Some(1_048_576_000),
                started_at: None,
                status: DownloadUiStatus::Completed(
                    "./S2A_MSIL2A_20260312T104031_N0511_R008_T31TCJ_20260312T141512.SAFE".into(),
                ),
            },
        );
    }
    app.download_order = vec!["a2".into(), "a4".into(), "a1".into()];
    app
}

fn product(id: &str, name: &str, date: &str, cloud: Option<f64>) -> Product {
    Product {
        name: name.to_string(),
        id: id.to_string(),
        acquisition_date: date.to_string(),
        publication_date: date.to_string(),
        online: true,
        cloud_cover: cloud,
    }
}

fn buffer_to_svg(buffer: &ratatui::buffer::Buffer) -> String {
    let area = buffer.area();
    let width = PAD * 2.0 + f64::from(area.width) * CELL_W;
    let height = PAD * 2.0 + f64::from(area.height) * CELL_H;

    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width:.0}\" height=\"{height:.0}\" viewBox=\"0 0 {width:.1} {height:.1}\">\n"
    ));
    out.push_str(&format!(
        "  <rect width=\"100%\" height=\"100%\" rx=\"10\" ry=\"10\" fill=\"{BG}\"/>\n"
    ));
    out.push_str(
        "  <g font-family=\"ui-monospace, SFMono-Regular, Menlo, Consolas, monospace\" font-size=\"13\">\n",
    );

    for y in 0..area.height {
        for x in 0..area.width {
            emit_cell(&mut out, x, y, &buffer[(x, y)]);
        }
    }

    out.push_str("  </g>\n</svg>\n");
    out
}

fn emit_cell(out: &mut String, x: u16, y: u16, cell: &Cell) {
    let ch = cell.symbol();
    let px = PAD + f64::from(x) * CELL_W;
    let py = PAD + f64::from(y) * CELL_H;
    let bg = color_hex(cell.bg);

    if bg != BG {
        out.push_str(&format!(
            "    <rect x=\"{px:.1}\" y=\"{py:.1}\" width=\"{CELL_W}\" height=\"{CELL_H}\" fill=\"{bg}\"/>\n"
        ));
    }

    if ch.is_empty() || ch == " " {
        return;
    }

    let fill = {
        let fg = color_hex(cell.fg);
        if fg == BG { FG.to_string() } else { fg }
    };
    out.push_str(&format!(
        "    <text x=\"{px:.1}\" y=\"{:.1}\" fill=\"{fill}\">{}</text>\n",
        py + CELL_H * 0.78,
        xml_escape(ch)
    ));
}

fn color_hex(color: Color) -> String {
    match color {
        Color::Reset => BG.to_string(),
        Color::Black => "#15161e".to_string(),
        Color::Red => "#f7768e".to_string(),
        Color::Green => "#9ece6a".to_string(),
        Color::Yellow => "#e0af68".to_string(),
        Color::Blue => "#7aa2f7".to_string(),
        Color::Magenta => "#bb9af7".to_string(),
        Color::Cyan => "#7dcfff".to_string(),
        Color::Gray => "#a9b1d6".to_string(),
        Color::DarkGray => "#565f89".to_string(),
        Color::LightRed => "#ff899d".to_string(),
        Color::LightGreen => "#b9f27c".to_string(),
        Color::LightYellow => "#ff9e64".to_string(),
        Color::LightBlue => "#7dcfff".to_string(),
        Color::LightMagenta => "#c0caf5".to_string(),
        Color::LightCyan => "#b4f9f8".to_string(),
        Color::White => "#c0caf5".to_string(),
        Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        Color::Indexed(_) => FG.to_string(),
    }
}

fn xml_escape(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '&' => "&amp;".to_string(),
            '<' => "&lt;".to_string(),
            '>' => "&gt;".to_string(),
            '"' => "&quot;".to_string(),
            _ => c.to_string(),
        })
        .collect()
}
