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
- Usable both as a **library** (`use copernicus_explorer::...`) and as a **CLI**

## Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) 1.85+ (edition 2024)
- A free [Copernicus Data Space](https://dataspace.copernicus.eu/) account
  (required for authentication and downloads; searching is anonymous)

## Building

```bash
git clone <repo-url>
cd copernicus_explorer
cargo build --release
```

The binary is produced at `target/release/copernicus_explorer`.

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

## Library usage

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

## Project structure

```
src/
  lib.rs        Module declarations and re-exports
  main.rs       CLI binary (clap)
  error.rs      CopernicusError enum (thiserror)
  models.rs     Satellite enum, Product struct (serde)
  geometry.rs   Point, BoundingBox, WKT conversion
  auth.rs       OAuth2 token retrieval (reqwest)
  search.rs     SearchQuery builder, OData filter construction
  download.rs   Scene download with streaming I/O
```

## Running tests

```bash
cargo test
```

## License

This project is provided as-is for educational purposes.
