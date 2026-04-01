//! Blocking (synchronous) wrappers around the async API.
//!
//! These functions create a Tokio runtime internally and block on the
//! async counterparts.  Use them from synchronous contexts (e.g. Python
//! bindings, scripts, or tests that don't run inside a Tokio runtime).
//!
//! ```rust,no_run
//! use copernicus_explorer::blocking;
//!
//! let token = blocking::get_access_token("user@example.com", "password").unwrap();
//! ```

use std::path::{Path, PathBuf};

use crate::error::{CopernicusError, Result};
use crate::models::Product;
use crate::search::SearchQuery;

fn runtime() -> Result<tokio::runtime::Runtime> {
    tokio::runtime::Runtime::new()
        .map_err(|e| CopernicusError::RuntimeError(format!("failed to create runtime: {e}")))
}

/// Blocking version of [`crate::auth::get_access_token`].
pub fn get_access_token(username: &str, password: &str) -> Result<String> {
    runtime()?.block_on(crate::auth::get_access_token(username, password))
}

/// Blocking version of [`crate::auth::get_access_token_from_env`].
pub fn get_access_token_from_env() -> Result<String> {
    runtime()?.block_on(crate::auth::get_access_token_from_env())
}

/// Blocking version of [`crate::search::get_scene_id`].
pub fn get_scene_id(scene_name: &str) -> Result<String> {
    runtime()?.block_on(crate::search::get_scene_id(scene_name))
}

/// Blocking version of [`crate::download::download_scene`].
pub fn download_scene(scene_name: &str, dir: &Path, access_token: &str) -> Result<PathBuf> {
    runtime()?.block_on(crate::download::download_scene(
        scene_name,
        dir,
        access_token,
    ))
}

/// Blocking version of [`crate::download::download_by_id`].
pub fn download_by_id(id: &str, dir: &Path, access_token: &str) -> Result<PathBuf> {
    runtime()?.block_on(crate::download::download_by_id(id, dir, access_token))
}

/// Blocking version of [`crate::download::download_products`].
pub fn download_products(
    products: &[Product],
    dir: &Path,
    access_token: &str,
    max_concurrent: usize,
) -> Vec<Result<PathBuf>> {
    match runtime() {
        Ok(rt) => rt.block_on(crate::download::download_products(
            products,
            dir,
            access_token,
            max_concurrent,
        )),
        Err(e) => vec![Err(e)],
    }
}

impl SearchQuery {
    /// Blocking version of [`SearchQuery::execute`].
    pub fn execute_blocking(&self) -> Result<Vec<Product>> {
        runtime()?.block_on(self.execute())
    }
}
