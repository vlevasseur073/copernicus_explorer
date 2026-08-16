//! Generate `docs/tui-screenshot.svg` from a demo TUI frame.
//!
//! ```bash
//! cargo run -p copernicus_explorer --example render_tui_screenshot
//! ```

use std::path::PathBuf;

fn main() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../docs/tui-screenshot.svg");
    copernicus_explorer::tui::write_demo_screenshot(&path).expect("write screenshot");
    println!("Wrote {}", path.canonicalize().unwrap_or(path).display());
}
