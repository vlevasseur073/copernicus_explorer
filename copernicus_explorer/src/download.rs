use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures::StreamExt;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use tokio::io::AsyncWriteExt;
use tokio::sync::Semaphore;

use crate::error::{CopernicusError, Result};
use crate::models::Product;
use crate::s3::{OutputDestination, S3Destination};
use crate::search::get_scene_id;

const DOWNLOAD_BASE_URL: &str = "https://zipper.dataspace.copernicus.eu/odata/v1/Products";

// ---------------------------------------------------------------------------
// Public API — local filesystem (backward-compatible)
// ---------------------------------------------------------------------------

/// Download a Sentinel scene to a local directory.
///
/// Resolves the scene name to a CDSE UUID via `get_scene_id`, then streams
/// the product archive to disk with a progress bar.
pub async fn download_scene(scene_name: &str, dir: &Path, access_token: &str) -> Result<PathBuf> {
    let id = get_scene_id(scene_name).await?;
    download_by_id_inner(
        &id,
        scene_name,
        &OutputDestination::Local(dir.to_path_buf()),
        access_token,
        None,
    )
    .await
    .map(PathBuf::from)
}

/// Download a Sentinel product by its CDSE UUID.
///
/// Use this when you already have the product ID (e.g. from a previous
/// search), avoiding the extra API call that [`download_scene`] makes to
/// resolve a scene name to an ID.
pub async fn download_by_id(id: &str, dir: &Path, access_token: &str) -> Result<PathBuf> {
    download_by_id_inner(
        id,
        id,
        &OutputDestination::Local(dir.to_path_buf()),
        access_token,
        None,
    )
    .await
    .map(PathBuf::from)
}

/// Download multiple products concurrently with a configurable concurrency limit.
///
/// Each product from the input slice is downloaded in parallel (up to
/// `max_concurrent` at a time).  Progress bars are displayed for all
/// active downloads using `indicatif::MultiProgress`.
pub async fn download_products(
    products: &[Product],
    dir: &Path,
    access_token: &str,
    max_concurrent: usize,
) -> Vec<Result<PathBuf>> {
    download_products_to(
        products,
        &OutputDestination::Local(dir.to_path_buf()),
        access_token,
        max_concurrent,
    )
    .await
    .into_iter()
    .map(|r| r.map(PathBuf::from))
    .collect()
}

// ---------------------------------------------------------------------------
// Public API — generic destination (local or S3)
// ---------------------------------------------------------------------------

/// Download a scene to an arbitrary destination (local path or S3 bucket).
pub async fn download_scene_to(
    scene_name: &str,
    dest: &OutputDestination,
    access_token: &str,
) -> Result<String> {
    let id = get_scene_id(scene_name).await?;
    download_by_id_inner(&id, scene_name, dest, access_token, None).await
}

/// Download a product by CDSE UUID to an arbitrary destination.
pub async fn download_by_id_to(
    id: &str,
    dest: &OutputDestination,
    access_token: &str,
) -> Result<String> {
    download_by_id_inner(id, id, dest, access_token, None).await
}

/// Download multiple products to an arbitrary destination.
pub async fn download_products_to(
    products: &[Product],
    dest: &OutputDestination,
    access_token: &str,
    max_concurrent: usize,
) -> Vec<Result<String>> {
    let semaphore = Arc::new(Semaphore::new(max_concurrent));
    let multi = Arc::new(MultiProgress::new());
    let client = Arc::new(reqwest::Client::new());

    let mut handles = Vec::with_capacity(products.len());

    for product in products {
        let sem = Arc::clone(&semaphore);
        let mp = Arc::clone(&multi);
        let cl = Arc::clone(&client);
        let id = product.id.clone();
        let name = product.name.clone();
        let token = access_token.to_string();
        let dest = dest.clone();

        let handle = tokio::spawn(async move {
            let _permit = sem
                .acquire()
                .await
                .map_err(|e| CopernicusError::RuntimeError(e.to_string()))?;

            let resolved_id = if id.is_empty() {
                get_scene_id(&name).await?
            } else {
                id
            };

            download_by_id_with_client(&cl, &resolved_id, &name, &dest, &token, Some(&mp)).await
        });

        handles.push(handle);
    }

    let mut results = Vec::with_capacity(handles.len());
    for handle in handles {
        match handle.await {
            Ok(result) => results.push(result),
            Err(e) => results.push(Err(CopernicusError::RuntimeError(e.to_string()))),
        }
    }

    results
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Core download logic: fetch a product by its CDSE UUID and write to destination.
async fn download_by_id_inner(
    id: &str,
    display_name: &str,
    dest: &OutputDestination,
    access_token: &str,
    multi: Option<&MultiProgress>,
) -> Result<String> {
    let client = reqwest::Client::new();
    download_by_id_with_client(&client, id, display_name, dest, access_token, multi).await
}

/// Inner download using a shared `reqwest::Client`.
async fn download_by_id_with_client(
    client: &reqwest::Client,
    id: &str,
    display_name: &str,
    dest: &OutputDestination,
    access_token: &str,
    multi: Option<&MultiProgress>,
) -> Result<String> {
    let url = format!("{DOWNLOAD_BASE_URL}({id})/$value");

    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {access_token}"))
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(CopernicusError::DownloadFailed(format!(
            "{display_name}: HTTP {status}",
            status = response.status()
        )));
    }

    let filename = response
        .headers()
        .get("content-disposition")
        .and_then(|v| v.to_str().ok())
        .and_then(extract_filename)
        .unwrap_or_else(|| format!("{display_name}.zip"));

    let total_size = response.content_length();

    let pb = create_progress_bar(&filename, total_size);
    let pb = match multi {
        Some(mp) => mp.add(pb),
        None => pb,
    };

    let result = match dest {
        OutputDestination::Local(dir) => write_to_local(dir, &filename, response, &pb).await?,
        OutputDestination::S3(s3dest) => write_to_s3(s3dest, &filename, response, &pb).await?,
    };

    pb.finish_with_message(format!("saved to {result}"));

    Ok(result)
}

async fn write_to_local(
    dir: &Path,
    filename: &str,
    response: reqwest::Response,
    pb: &ProgressBar,
) -> Result<String> {
    let output_path = dir.join(filename);
    let mut file = tokio::fs::File::create(&output_path).await?;
    let mut stream = response.bytes_stream();
    let mut downloaded: u64 = 0;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
        pb.set_position(downloaded);
    }

    Ok(output_path.to_string_lossy().into_owned())
}

async fn write_to_s3(
    s3dest: &S3Destination,
    filename: &str,
    response: reqwest::Response,
    pb: &ProgressBar,
) -> Result<String> {
    let mut stream = response.bytes_stream();
    let mut buffer = Vec::new();
    let mut downloaded: u64 = 0;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        downloaded += chunk.len() as u64;
        pb.set_position(downloaded);
        buffer.extend_from_slice(&chunk);
    }

    let mut cursor = std::io::Cursor::new(buffer);
    s3dest.upload(&mut cursor, filename).await
}

/// Create a progress bar appropriate for the download.
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
