# Copernicus Explorer

[![PyPI](https://img.shields.io/pypi/v/copernicus-explorer)](https://pypi.org/project/copernicus-explorer/)
[![Python](https://img.shields.io/pypi/pyversions/copernicus-explorer)](https://pypi.org/project/copernicus-explorer/)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/vlevasseur073/copernicus_explorer/blob/main/LICENSE)

A Python package for browsing and downloading Sentinel satellite products from
the [Copernicus Data Space Ecosystem (CDSE)](https://dataspace.copernicus.eu/).

Built on a native Rust core via [PyO3](https://pyo3.rs), it runs at full
compiled speed with no serialization overhead.

## Features

- **Search** the CDSE catalogue by satellite, product type, date range, cloud
  cover, tile ID, point, or bounding box
- **Download** scenes by name with Bearer-token authentication
- **Batch download** multiple products concurrently with configurable parallelism
- **Authenticate** against the CDSE OAuth2 identity provider
- Supports **Sentinel-1, -2, -3, -5P, and -6**
- Includes a **CLI** (`copernicus-explorer`) for quick terminal usage

## Installation

```bash
pip install copernicus-explorer
```

### Prerequisites

- Python 3.9+
- A free [Copernicus Data Space](https://dataspace.copernicus.eu/) account
  (required for authentication and downloads; searching is anonymous)

## Quick start

### Search for products

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

### Download a single scene

```python
token = ce.get_access_token_from_env()
path = ce.download_scene(products[0].name, "./data", token)
print(f"Downloaded to {path}")
```

### Batch download with concurrency

```python
token = ce.get_access_token_from_env()
results = ce.download_products(products, "./data", token, max_concurrent=3)

for product, result in zip(products, results):
    if result is not None:
        print(f"  OK: {product.name} -> {result}")
    else:
        print(f"  FAILED: {product.name}")
```

### Authenticate explicitly

```python
token = ce.get_access_token("you@example.com", "yourpassword")
```

Or via environment variables:

```bash
export COPERNICUS_USER="you@example.com"
export COPERNICUS_PASS="yourpassword"
```

```python
token = ce.get_access_token_from_env()
```

## CLI usage

Installing the package also provides the `copernicus-explorer` command:

```
copernicus-explorer [OPTIONS] COMMAND [ARGS]...

Commands:
  search    Search the CDSE catalogue for satellite products
  download  Download one or more scenes by name
  auth      Test authentication and print a token summary
```

### search

Search the catalogue. Dates default to the last 30 days if omitted.

```bash
# Sentinel-2 L2A near Toulouse, max 30% cloud cover
copernicus-explorer search sentinel-2 -p L2A --point 43.6,1.44 -c 30

# Sentinel-1 GRD over the Alps with explicit date range
copernicus-explorer search sentinel-1 -p GRD \
  --bbox 47.5,6.0,45.5,11.0 \
  --start 2026-03-01 --end 2026-03-24

# Sentinel-2 by tile, limit to 3 results
copernicus-explorer search sentinel-2 -p L2A --tile T31TFJ -n 3
```

| Flag | Description |
|------|-------------|
| `SATELLITE` | `sentinel-1`, `sentinel-2`, `sentinel-3`, `sentinel-5p`, `sentinel-6` |
| `-p, --product TYPE` | Product type filter (e.g. `L2A`, `L1C`, `GRD`) |
| `--start YYYY-MM-DD` | Start of acquisition window (default: 30 days ago) |
| `--end YYYY-MM-DD` | End of acquisition window (default: today) |
| `--tile TILE` | Sentinel-2 tile identifier (e.g. `T31TFJ`) |
| `-c, --cloud 0-100` | Maximum cloud cover percentage |
| `--point LAT,LON` | Point geometry (e.g. `43.6,1.44`) |
| `--bbox TLAT,LLON,BLAT,RLON` | Bounding box (e.g. `47.5,6.0,45.5,11.0`) |
| `-n, --max-results N` | Maximum number of results (default: `10`) |

### download

Download one or more scenes by name. Requires authentication.

```bash
# Single scene
copernicus-explorer download \
  "S2B_MSIL2A_20260315T105019_N0512_R051_T31TCJ_20260315T144522.SAFE" \
  -o ./data

# Multiple scenes concurrently (max 4 in parallel by default)
copernicus-explorer download \
  "S2B_MSIL2A_20260315T105019_N0512_R051_T31TCJ_20260315T144522.SAFE" \
  "S2A_MSIL2A_20260317T104021_N0512_R008_T31TCJ_20260317T160837.SAFE" \
  -o ./data -j 2
```

| Flag | Description |
|------|-------------|
| `SCENES...` | One or more full scene names |
| `-o, --output-dir DIR` | Output directory (default: `.`) |
| `-j, --concurrent N` | Maximum concurrent downloads (default: `4`) |
| `-u, --user USER` | Username (or set `COPERNICUS_USER`) |
| `-P, --password PASS` | Password (or set `COPERNICUS_PASS`) |

### auth

Test your credentials:

```bash
copernicus-explorer auth
copernicus-explorer auth -u you@example.com -P yourpassword
```

## Python API reference

### Classes

| Class | Constructor | Key methods / attributes |
|-------|-------------|--------------------------|
| `Satellite` | `Satellite.sentinel1()`, `.sentinel2()`, `.sentinel3()`, `.sentinel5p()`, `.sentinel6()` | `.collection_name()`, `.known_products()`, `.is_valid_product(str)` |
| `SearchQuery` | `SearchQuery(satellite)` | `.product(str)`, `.dates(start, end)`, `.tile(str)`, `.max_cloud_cover(float)`, `.geometry_point(Point)`, `.geometry_bbox(BoundingBox)`, `.max_results(int)`, `.execute()` |
| `Product` | returned by `SearchQuery.execute()` | `.name`, `.id`, `.acquisition_date`, `.publication_date`, `.online`, `.cloud_cover` |
| `Point` | `Point(lat, lon)` | `.lat`, `.lon` |
| `BoundingBox` | `BoundingBox((lat, lon), (lat, lon))` | `.upper_left`, `.lower_right` |

### Functions

| Function | Description |
|----------|-------------|
| `get_access_token(username, password)` | Authenticate and return an access token string |
| `get_access_token_from_env()` | Authenticate using `COPERNICUS_USER` / `COPERNICUS_PASS` env vars |
| `download_scene(scene_name, directory, token)` | Download a single scene; returns the output file path |
| `download_products(products, directory, token, max_concurrent=4)` | Download multiple products concurrently; returns a list of paths (`None` on failure) |
| `get_scene_id(scene_name)` | Resolve a scene name to its CDSE UUID |
| `format_products(products)` | Format a list of products as a table string |
| `print_products(products)` | Print a formatted product table to stdout |

## Relation to the Rust crate

This package is the Python interface to the
[`copernicus_explorer`](https://github.com/vlevasseur073/copernicus_explorer)
Rust library. The Python import name is `copernicus_explorer_py` while the
CLI command is `copernicus-explorer` (same name as the Rust CLI).

If you are looking for the Rust library or the Rust-built CLI binary, see the
[main repository README](https://github.com/vlevasseur073/copernicus_explorer).

## License

[MIT](https://github.com/vlevasseur073/copernicus_explorer/blob/main/LICENSE)
