//! # Copernicus Explorer
//!
//! A Rust client for browsing and downloading Sentinel satellite products
//! from the [Copernicus Data Space Ecosystem (CDSE)](https://dataspace.copernicus.eu/).
//!
//! ## Async-first design
//!
//! The primary API is **async** (tokio-based).  For synchronous contexts
//! (Python bindings, simple scripts), use the [`blocking`] module which
//! provides thin wrappers that create a Tokio runtime internally.
//!
//! ## Quick start (async)
//!
//! ```rust,no_run
//! use copernicus_explorer::{Satellite, SearchQuery, get_access_token};
//!
//! #[tokio::main]
//! async fn main() {
//!     let token = get_access_token("user@example.com", "password").await.unwrap();
//!
//!     let products = SearchQuery::new(Satellite::Sentinel2)
//!         .product("L2A")
//!         .max_cloud_cover(20.0)
//!         .max_results(5)
//!         .execute()
//!         .await
//!         .unwrap();
//!
//!     for product in &products {
//!         println!("{product}");
//!     }
//! }
//! ```
//!
//! ## Quick start (blocking)
//!
//! ```rust,no_run
//! use copernicus_explorer::{Satellite, SearchQuery, blocking};
//!
//! let token = blocking::get_access_token("user@example.com", "password").unwrap();
//!
//! let products = SearchQuery::new(Satellite::Sentinel2)
//!     .product("L2A")
//!     .max_cloud_cover(20.0)
//!     .max_results(5)
//!     .execute_blocking()
//!     .unwrap();
//! ```
//!
//! ## Module overview
//!
//! Rust organises code into **modules**.  Each file in `src/` is a module.
//! `pub mod` makes a module visible to users of the library.
//! `pub use` **re-exports** items so they can be imported directly from
//! the crate root, without needing to know the internal module structure.

pub mod auth;
pub mod blocking;
pub mod download;
pub mod error;
pub mod geometry;
pub mod models;
pub mod search;

// Re-export the most commonly used types at the crate root for convenience.
// This lets users write `use copernicus_explorer::Satellite` instead of
// `use copernicus_explorer::models::Satellite`.
pub use auth::{get_access_token, get_access_token_from_env};
pub use download::{download_by_id, download_products, download_scene};
pub use error::CopernicusError;
pub use geometry::{BoundingBox, Geometry, Point};
pub use models::{Product, Products, Satellite, format_products, print_products};
pub use search::{SearchQuery, get_scene_id};
