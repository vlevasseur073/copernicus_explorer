# Copernicus Explorer

A Rust client for browsing and downloading Sentinel satellite products from the
[Copernicus Data Space Ecosystem (CDSE)](https://dataspace.copernicus.eu/).

Inspired by [SentinelExplorer.jl](https://github.com/JoshuaBillson/SentinelExplorer.jl)
and built as a didactic Rust learning project.

## Features

- **Search** the CDSE catalogue by satellite, product type, date range, cloud
  cover, tile ID, point, or bounding box
- **Download** scenes by name with Bearer-token authentication
- **Authenticate** against the CDSE OAuth2 identity provider
- Supports Sentinel-1, Sentinel-2, Sentinel-3, Sentinel-5P, and Sentinel-6
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
      auth.rs              OAuth2 token retrieval (reqwest)
      search.rs            SearchQuery builder, OData filter construction
      download.rs          Scene download with streaming I/O + progress bar
  python/                  Python bindings (PyO3 + maturin)
    Cargo.toml
    pyproject.toml
    src/
      lib.rs               #[pymodule] wrapping the core library
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
  download  Download a scene by name
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

Download a scene by its full name. Requires authentication.

```bash
copernicus_explorer download \
  "S2B_MSIL2A_20260315T105019_N0512_R051_T31TCJ_20260315T144522.SAFE" \
  -o ./data
```

**Options:**

| Flag | Description |
|------|-------------|
| `<SCENE>` | Full scene name |
| `-o, --output-dir <DIR>` | Output directory (default: `.`) |
| `-u, --user <USER>` | Username (or set `COPERNICUS_USER`) |
| `-P, --pass <PASS>` | Password (or set `COPERNICUS_PASS`) |

## Rust library usage

Add to your `Cargo.toml`:

```toml
[dependencies]
copernicus_explorer = { path = "../copernicus_explorer" }
chrono = "0.4"
```

Then in your code:

```rust
use chrono::{Duration, Utc};
use copernicus_explorer::{
    BoundingBox, Geometry, Point, Satellite, SearchQuery,
    get_access_token, download_scene,
};

fn main() -> Result<(), copernicus_explorer::CopernicusError> {
    // Search (no authentication required)
    let products = SearchQuery::new(Satellite::Sentinel2)
        .product("L2A")
        .dates(Utc::now() - Duration::days(7), Utc::now())
        .max_cloud_cover(20.0)
        .geometry(Geometry::Point(Point::new(43.6, 1.44)))
        .max_results(5)
        .execute()?;

    for p in &products {
        println!("{p}");
    }

    // Download (requires credentials)
    let token = get_access_token("user@example.com", "password")?;
    let path = download_scene(&products[0].name, ".".as_ref(), &token)?;
    println!("Downloaded to {}", path.display());

    Ok(())
}
```

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

```python
import copernicus_explorer_py as ce

# Choose a satellite
s2 = ce.Satellite.sentinel2()
print(s2.known_products())   # ['L1C', 'L2A']

# Search the catalogue
results = (
    ce.SearchQuery(s2)
    .product("L2A")
    .dates("2026-03-01", "2026-03-24")
    .max_cloud_cover(20.0)
    .geometry_point(ce.Point(43.6, 1.44))
    .max_results(5)
    .execute()
)

for p in results:
    print(p.name, p.acquisition_date, p.cloud_cover)

# Authenticate and download
token = ce.get_access_token("user@example.com", "password")
path = ce.download_scene(results[0].name, "./data", token)
print(path)
```

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
| `download_scene(scene_name, directory, token)` | Download a scene; returns the output file path |
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

## License

This project is provided as-is for educational purposes.
