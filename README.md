[![Stable](https://img.shields.io/badge/docs-stable-blue.svg)](https://vlevasseur073.github.io/copernicus_explorer/copernicus_explorer/)

# Copernicus Explorer

A Rust client for browsing and downloading Sentinel satellite products from the
[Copernicus Data Space Ecosystem (CDSE)](https://dataspace.copernicus.eu/).

## Features

- **Search** the CDSE catalogue by satellite, product type, date range, cloud
  cover, tile ID, point, or bounding box
- **Download** scenes by name with Bearer-token authentication
- **Batch download** multiple products concurrently with configurable parallelism
- **Authenticate** against the CDSE OAuth2 identity provider
- Supports Sentinel-1, Sentinel-2, Sentinel-3, Sentinel-5P, and Sentinel-6
- **Async-first** design (tokio) with synchronous `blocking` wrappers
- Usable as a **Rust library**, a **CLI**, or from **Python** via native bindings

## Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) 1.85+ (edition 2024)
- A free [Copernicus Data Space](https://dataspace.copernicus.eu/) account
  (required for authentication and downloads; searching is anonymous)
- For Python bindings: Python 3.9+, [uv](https://docs.astral.sh/uv/) (recommended),
  and [maturin](https://www.maturin.rs/)

## Project structure

The repository is a Cargo workspace with two members:

```
copernicus_explorer/       Cargo workspace root
  Cargo.toml               Workspace manifest
  copernicus_explorer/     Core Rust library + CLI binary
    Cargo.toml
    src/
      lib.rs               Module declarations and re-exports
      main.rs              CLI binary (clap)
      error.rs             CopernicusError enum (thiserror)
      models.rs            Satellite enum, Product struct (serde)
      geometry.rs          Point, BoundingBox, WKT conversion
      auth.rs              OAuth2 token retrieval (reqwest, async)
      search.rs            SearchQuery builder, OData filter construction (async)
      download.rs          Single & batch download with streaming I/O + progress bars (async)
      blocking.rs          Synchronous wrappers (block_on) for non-async contexts
    examples/
      test_rust_api.rs     Interactive demo: search, download one or all results
  python/                  Python bindings (PyO3 + maturin)
    Cargo.toml
    pyproject.toml
    src/
      lib.rs               #[pymodule] wrapping the core library
    examples/
      test_python_api.py   Interactive demo: search, download one or all results
```

## Building

```bash
git clone <repo-url>
cd copernicus_explorer
cargo build --release
```

The CLI binary is produced at `target/release/copernicus_explorer`.

## CLI usage

```
copernicus_explorer <COMMAND>

Commands:
  search    Search the CDSE catalogue for satellite products
  download  Download one or more scenes by name
  auth      Test authentication and print a token summary
  help      Print this message or the help of the given subcommand(s)
```

### search

Search the catalogue. Dates default to the last 30 days if omitted.

```bash
# Sentinel-2 L2A near Toulouse, max 30% cloud cover
copernicus_explorer search sentinel-2 -p L2A --point 43.6,1.44 -c 30

# Sentinel-1 GRD over the Alps with explicit date range
copernicus_explorer search sentinel-1 -p GRD \
  --bbox 47.5,6.0,45.5,11.0 \
  --start 2026-03-01 --end 2026-03-24

# Sentinel-2 by tile, limit to 3 results
copernicus_explorer search sentinel-2 -p L2A --tile T31TFJ -n 3
```

**Options:**

| Flag | Description |
|------|-------------|
| `<SATELLITE>` | `sentinel-1`, `sentinel-2`, `sentinel-3`, `sentinel-5p`, `sentinel-6` |
| `-p, --product <TYPE>` | Product type filter (e.g. `L2A`, `L1C`, `GRD`) |
| `--start <YYYY-MM-DD>` | Start of acquisition window (default: 30 days ago) |
| `--end <YYYY-MM-DD>` | End of acquisition window (default: today) |
| `--tile <TILE>` | Sentinel-2 tile identifier (e.g. `T31TFJ`) |
| `-c, --cloud <0-100>` | Maximum cloud cover percentage |
| `--point <LAT,LON>` | Point geometry (e.g. `43.6,1.44`) |
| `--bbox <TLAT,LLON,BLAT,RLON>` | Bounding box (e.g. `47.5,6.0,45.5,11.0`) |
| `-n, --max-results <N>` | Maximum number of results (default: `10`) |

### auth

Test your credentials. Reads `COPERNICUS_USER` and `COPERNICUS_PASS` from the
environment, or accepts them as flags.

```bash
# From environment variables
export COPERNICUS_USER="you@example.com"
export COPERNICUS_PASS="yourpassword"
copernicus_explorer auth

# Or pass directly
copernicus_explorer auth -u you@example.com -P yourpassword
```

### download

Download one or more scenes by name. Requires authentication.

```bash
# Single scene
copernicus_explorer download \
  "S2B_MSIL2A_20260315T105019_N0512_R051_T31TCJ_20260315T144522.SAFE" \
  -o ./data

# Multiple scenes concurrently (max 4 in parallel by default)
copernicus_explorer download \
  "S2B_MSIL2A_20260315T105019_N0512_R051_T31TCJ_20260315T144522.SAFE" \
  "S2A_MSIL2A_20260317T104021_N0512_R008_T31TCJ_20260317T160837.SAFE" \
  -o ./data -j 2
```

**Options:**

| Flag | Description |
|------|-------------|
| `<SCENES>...` | One or more full scene names |
| `-o, --output-dir <DIR>` | Output directory (default: `.`) |
| `-j, --concurrent <N>` | Maximum concurrent downloads (default: `4`) |
| `-u, --user <USER>` | Username (or set `COPERNICUS_USER`) |
| `-P, --pass <PASS>` | Password (or set `COPERNICUS_PASS`) |

## Rust library usage

The library is **async-first** (tokio). A `blocking` module provides synchronous
wrappers for contexts that don't use an async runtime.

Add to your `Cargo.toml`:

```toml
[dependencies]
copernicus_explorer = { path = "../copernicus_explorer" }
chrono = "0.4"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
```

### Async example (search + batch download)

```rust
use chrono::{Duration, Utc};
use copernicus_explorer::{
    Geometry, Point, Products, Satellite, SearchQuery,
    download_products, get_access_token_from_env,
};

#[tokio::main]
async fn main() -> Result<(), copernicus_explorer::CopernicusError> {
    // Search (no authentication required)
    let products = SearchQuery::new(Satellite::Sentinel2)
        .product("L2A")
        .dates(Utc::now() - Duration::days(20), Utc::now())
        .max_cloud_cover(30.0)
        .geometry(Geometry::Point(Point::new(43.6, 1.44)))
        .max_results(5)
        .execute()
        .await?;

    println!("{}", Products(&products));

    // Authenticate and download all results (3 concurrent)
    let token = get_access_token_from_env().await?;
    let results = download_products(&products, "./data".as_ref(), &token, 3).await;

    for (product, result) in products.iter().zip(results.iter()) {
        match result {
            Ok(path) => println!("  OK: {} -> {}", product.name, path.display()),
            Err(e) => println!("  FAILED: {} -> {e}", product.name),
        }
    }

    Ok(())
}
```

### Blocking example

```rust
use copernicus_explorer::{Satellite, SearchQuery, blocking};

fn main() -> Result<(), copernicus_explorer::CopernicusError> {
    let products = SearchQuery::new(Satellite::Sentinel2)
        .product("L2A")
        .max_results(5)
        .execute_blocking()?;

    let token = blocking::get_access_token_from_env()?;
    let results = blocking::download_products(&products, "./data".as_ref(), &token, 4);

    for (product, result) in products.iter().zip(results.iter()) {
        match result {
            Ok(path) => println!("  OK: {} -> {}", product.name, path.display()),
            Err(e) => println!("  FAILED: {} -> {e}", product.name),
        }
    }

    Ok(())
}
```

More examples can be found in [rust examples](copernicus_explorer/examples)

## Python bindings

The `python/` crate provides native Python bindings via
[PyO3](https://pyo3.rs) and [maturin](https://www.maturin.rs/).
The compiled extension runs at full Rust speed with no serialization overhead.

### Setup

```bash
cd python
uv venv .venv
source .venv/bin/activate
uv pip install maturin
maturin develop
```

### Python usage

#### Search and display results

```python
from datetime import datetime, timedelta, timezone
import copernicus_explorer_py as ce

query = ce.SearchQuery(ce.Satellite.sentinel2())
query.product("L2A")
query.dates(
    datetime.now(timezone.utc) - timedelta(days=20),
    datetime.now(timezone.utc),
)
query.max_cloud_cover(30.0)
query.geometry_point(ce.Point(43.6, 1.44))
query.max_results(5)

products = query.execute()
ce.print_products(products)
```

#### Download a single scene

```python
token = ce.get_access_token_from_env()
path = ce.download_scene(products[0].name, "./data", token)
print(f"Downloaded to {path}")
```

#### Batch download with concurrency

```python
token = ce.get_access_token_from_env()
results = ce.download_products(products, "./data", token, max_concurrent=3)

for product, result in zip(products, results):
    if result is not None:
        print(f"  OK: {product.name} -> {result}")
    else:
        print(f"  FAILED: {product.name}")
```

More examples can be found in [python examples](python/examples)

### Python API reference

**Classes:**

| Class | Constructor | Key methods / attributes |
|-------|-------------|--------------------------|
| `Satellite` | `Satellite.sentinel1()`, `.sentinel2()`, `.sentinel3()`, `.sentinel5p()`, `.sentinel6()` | `.collection_name()`, `.known_products()`, `.is_valid_product(str)` |
| `SearchQuery` | `SearchQuery(satellite)` | `.product(str)`, `.dates(start, end)`, `.tile(str)`, `.max_cloud_cover(float)`, `.geometry_point(Point)`, `.geometry_bbox(BoundingBox)`, `.max_results(int)`, `.execute()` |
| `Product` | returned by `SearchQuery.execute()` | `.name`, `.id`, `.acquisition_date`, `.publication_date`, `.online`, `.cloud_cover` |
| `Point` | `Point(lat, lon)` | `.lat`, `.lon` |
| `BoundingBox` | `BoundingBox((lat, lon), (lat, lon))` | `.upper_left`, `.lower_right` |

**Functions:**

| Function | Description |
|----------|-------------|
| `get_access_token(username, password)` | Authenticate and return an access token string |
| `get_access_token_from_env()` | Authenticate using `COPERNICUS_USER` / `COPERNICUS_PASS` env vars |
| `download_scene(scene_name, directory, token)` | Download a single scene; returns the output file path |
| `download_products(products, directory, token, max_concurrent=4)` | Download multiple products concurrently; returns a list of paths (or `None` on failure) |
| `get_scene_id(scene_name)` | Resolve a scene name to its CDSE UUID |

## Running tests

```bash
# Rust unit tests (from workspace root)
cargo test

# Quick Python smoke test
cd python
source .venv/bin/activate
python -c "
import copernicus_explorer_py as ce
s2 = ce.Satellite.sentinel2()
print(s2, s2.known_products())
q = ce.SearchQuery(s2).product('L2A').max_results(3)
print(q)
"
```

## Changelog

See [CHANGELOG.md](CHANGELOG.md) for release notes.

## License

This project is licensed under the [MIT License](LICENSE).
