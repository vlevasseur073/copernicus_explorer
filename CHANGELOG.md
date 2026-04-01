# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- **Download by ID**: new `download_by_id()` function (async + blocking) downloads
  a product directly by its CDSE UUID, skipping the name-to-ID resolution query.
  Useful when the ID is already known from a previous search.
- **CLI `--id` flag**: the `download` subcommand now accepts `--id` to treat
  positional arguments as product UUIDs instead of scene names. Available in
  both the Rust and Python CLIs.
- **Python `download_by_id()` binding**: new function exposed in the Python
  package for direct download by UUID.
- **Dedicated Python README**: PyPI now displays a Python-specific README with
  installation via `pip install`, Python API reference, and CLI usage.
- **GeoJSON geometry support**: new `Polygon` type and `Geometry::from_geojson` /
  `Geometry::from_geojson_file` constructors for loading spatial filters from
  GeoJSON (Point, Polygon, Feature, FeatureCollection).
- **CLI `--geojson` flag**: the `search` subcommand now accepts `--geojson <FILE>`
  as an alternative to `--point` or `--bbox`. Available in both the Rust and
  Python CLIs.
- **Python `SearchQuery.geometry_geojson()` method**: set a geometry filter from
  a GeoJSON file path or raw GeoJSON string.

## [0.2.0] - 2025-03-25

### Added

- **Batch download**: new `download_products()` function downloads multiple
  products concurrently with configurable parallelism via
  `tokio::sync::Semaphore`. Progress bars for all active downloads are
  displayed simultaneously using `indicatif::MultiProgress`.
- **Async-first architecture**: all I/O functions (`get_access_token`,
  `get_access_token_from_env`, `SearchQuery::execute`, `get_scene_id`,
  `download_scene`, `download_products`) are now `async fn`.
- **`blocking` module**: synchronous wrappers (`blocking::get_access_token`,
  `blocking::download_scene`, `blocking::download_products`,
  `SearchQuery::execute_blocking`, etc.) for use in non-async contexts.
- **CLI multi-scene download**: the `download` subcommand now accepts multiple
  scene names as positional arguments and a `-j/--concurrent` flag (default: 4).
- **Python `download_products()`**: new binding to batch-download a list of
  `Product` objects with a `max_concurrent` parameter (default: 4).
- **`DownloadFailed` error variant**: distinguishes download errors from search
  errors in `CopernicusError`.
- **`RuntimeError` error variant**: surfaces tokio runtime creation failures.
- Interactive examples (`test_rust_api.rs`, `test_python_api.py`) now offer an
  "all" option to demonstrate concurrent batch download of search results.

### Changed

- **reqwest** switched from `blocking` feature to async client with `stream`
  feature for non-blocking response body streaming.
- **Dependencies**: added `tokio` (rt-multi-thread, macros, fs, sync),
  `futures` (StreamExt for async byte streams).
- **CLI entrypoint** changed from `fn main()` to `#[tokio::main] async fn main()`.
- **Python bindings** now use `copernicus_explorer::blocking::*` internally
  instead of the previously synchronous top-level functions.
- **Download streaming** uses `tokio::fs::File` + `AsyncWriteExt` and
  `reqwest::Response::bytes_stream()` instead of `std::io::Read` chunking.

### Removed

- Direct dependency on `reqwest`'s `blocking` feature (replaced by the
  library's own `blocking` module backed by a tokio runtime).

## [0.1.1] - 2025-03-14

### Added

- Publication to crates.io and PyPI triggered by tag release.
- Centralized workspace version in root `Cargo.toml`.
- Python CLI (`copernicus-explorer` console script via click).
- Interactive Rust and Python API examples.
- GitHub Actions CI/CD workflow.
- Documentation badge in README.

### Fixed

- Syntax error for the `tileId` attribute in CDSE OData filter.

## [0.1.0] - 2025-03-13

### Added

- Initial release.
- Search the CDSE catalogue by satellite, product type, date range, cloud
  cover, tile ID, point, or bounding box.
- Download scenes by name with Bearer-token authentication and progress bar.
- OAuth2 password-grant authentication against CDSE identity provider.
- Support for Sentinel-1, Sentinel-2, Sentinel-3, Sentinel-5P, and Sentinel-6.
- Rust library with `SearchQuery` builder, `Product` model, and `Geometry` types.
- Native CLI binary (clap) with `search`, `download`, and `auth` subcommands.
- Python bindings via PyO3 and maturin.
