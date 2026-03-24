use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use indicatif::{ProgressBar, ProgressStyle};

use crate::error::{CopernicusError, Result};
use crate::search::get_scene_id;

const DOWNLOAD_BASE_URL: &str = "https://zipper.dataspace.copernicus.eu/odata/v1/Products";

/// Download a Sentinel scene to a local directory.
///
/// # Path and PathBuf
///
/// - `&Path` is a borrowed reference to a filesystem path (like `&str` for
///   strings).  It's the type you use in function parameters.
/// - `PathBuf` is the owned version (like `String`).  It's what you return
///   from functions or store in structs.
///
/// `Path` handles OS differences (forward vs back slashes) automatically.
///
/// # Streaming I/O with a progress bar
///
/// Instead of loading the entire file into memory (which could be gigabytes
/// for satellite imagery), we stream the HTTP response body in chunks and
/// write each chunk to disk immediately.  This keeps memory usage constant
/// regardless of file size.
///
/// We use the `indicatif` crate to display a progress bar.  `indicatif`
/// works by wrapping any I/O loop: you create a `ProgressBar`, call
/// `.inc(n)` after each chunk, and `.finish_with_message(...)` when done.
///
/// # The `Read` trait
///
/// `response` implements `std::io::Read`, which means we can call
/// `.read(&mut buf)` to pull bytes in fixed-size chunks.  This is Rust's
/// universal interface for anything you can read bytes from -- files,
/// network sockets, compressed streams, etc.
///
/// # Arguments
///
/// * `scene_name` - The full Sentinel scene name (e.g. "S2B_MSIL2A_20200804T183919_...")
/// * `dir` - The directory to save the downloaded file into
/// * `access_token` - A valid CDSE access token from `get_access_token`
pub fn download_scene(scene_name: &str, dir: &Path, access_token: &str) -> Result<PathBuf> {
    let id = get_scene_id(scene_name)?;

    let url = format!("{DOWNLOAD_BASE_URL}({id})/$value");

    let client = reqwest::blocking::Client::new();
    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {access_token}"))
        .send()?;

    if !response.status().is_success() {
        return Err(CopernicusError::SearchFailed(format!(
            "download failed with HTTP {status}",
            status = response.status()
        )));
    }

    let filename = response
        .headers()
        .get("content-disposition")
        .and_then(|v| v.to_str().ok())
        .and_then(extract_filename)
        .unwrap_or_else(|| format!("{scene_name}.zip"));

    let output_path = dir.join(&filename);
    let total_size = response.content_length();

    let pb = create_progress_bar(&filename, total_size);

    // Stream the response body to disk in 64 KB chunks.
    // `response` implements `std::io::Read`, so we can pull bytes
    // incrementally with `.read()` instead of buffering everything.
    let mut file = File::create(&output_path)?;
    let mut source = response;
    let mut buf = [0u8; 64 * 1024];
    let mut downloaded: u64 = 0;

    loop {
        let n = std::io::Read::read(&mut source, &mut buf)?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
        downloaded += n as u64;
        pb.set_position(downloaded);
    }

    pb.finish_with_message(format!("saved to {}", output_path.display()));

    Ok(output_path)
}

/// Create a progress bar appropriate for the download.
///
/// If the server sent a `Content-Length` header, we show a determinate bar
/// with percentage, downloaded/total bytes, speed, and ETA:
///
///   scene.zip  [████████████░░░░░░░░]  62% 158.3/255.1 MB  12.4 MB/s  eta 8s
///
/// If the total size is unknown, we show a spinner with a byte counter.
fn create_progress_bar(filename: &str, total_size: Option<u64>) -> ProgressBar {
    match total_size {
        Some(total) => {
            let pb = ProgressBar::new(total);
            pb.set_style(
                ProgressStyle::default_bar()
                    .template(
                        "{msg}  [{bar:40.cyan/dim}]  {percent}%  \
                         {bytes}/{total_bytes}  {binary_bytes_per_sec}  eta {eta}",
                    )
                    .expect("valid progress bar template")
                    .progress_chars("█▓░"),
            );
            pb.set_message(filename.to_string());
            pb
        }
        None => {
            let pb = ProgressBar::new_spinner();
            pb.set_style(
                ProgressStyle::default_spinner()
                    .template("{spinner:.cyan} {msg}  {bytes}  {binary_bytes_per_sec}")
                    .expect("valid spinner template"),
            );
            pb.set_message(filename.to_string());
            pb
        }
    }
}

/// Extract filename from a Content-Disposition header value.
/// E.g. `attachment; filename="scene.zip"` -> `"scene.zip"`
fn extract_filename(header_value: &str) -> Option<String> {
    header_value.split(';').find_map(|part| {
        let part = part.trim();
        if part.starts_with("filename=") {
            Some(
                part.trim_start_matches("filename=")
                    .trim_matches('"')
                    .to_string(),
            )
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_content_disposition() {
        let header = "attachment; filename=\"S2B_MSIL2A_20200804.zip\"";
        assert_eq!(
            extract_filename(header),
            Some("S2B_MSIL2A_20200804.zip".to_string())
        );
    }

    #[test]
    fn parse_content_disposition_no_quotes() {
        let header = "attachment; filename=scene.zip";
        assert_eq!(extract_filename(header), Some("scene.zip".to_string()));
    }

    #[test]
    fn parse_content_disposition_missing() {
        assert_eq!(extract_filename("inline"), None);
    }

    #[test]
    fn progress_bar_with_size() {
        let pb = create_progress_bar("test.zip", Some(1024));
        pb.set_position(512);
        pb.finish();
    }

    #[test]
    fn progress_bar_without_size() {
        let pb = create_progress_bar("test.zip", None);
        pb.inc(100);
        pb.finish();
    }
}
