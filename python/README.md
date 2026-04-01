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
  cover, tile ID, point, bounding box, or GeoJSON geometry
- **Download** scenes by name with Bearer-token authentication, to local
  filesystem or directly to an **S3-compatible bucket**
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

### Search with a GeoJSON file

```python
import copernicus_explorer_py as ce

query = ce.SearchQuery(ce.Satellite.sentinel2())
query.product("L2A")
query.geometry_geojson("roi.geojson")
query.max_cloud_cover(30.0)
query.max_results(5)

products = query.execute()
ce.print_products(products)
```

The method also accepts a raw GeoJSON string instead of a file path.

### Download a single scene

```python
token = ce.get_access_token_from_env()
path = ce.download_scene(products[0].name, "./data", token)
print(f"Downloaded to {path}")
```

### Download by product ID

If you already have the CDSE product UUID (e.g. from a previous search),
you can skip the name-to-ID resolution query:

```python
token = ce.get_access_token_from_env()
path = ce.download_by_id(products[0].id, "./data", token)
print(f"Downloaded to {path}")
```

### Download directly to an S3 bucket

Pass an `s3://` URI as the directory and optionally point to a credentials file:

```python
token = ce.get_access_token_from_env()
path = ce.download_by_id(
    products[0].id,
    "s3://my-bucket/SAFE/",
    token,
    s3_config="~/.config/copernicus_explorer/s3.conf",
)
print(f"Uploaded to {path}")
```

S3 credentials are resolved in order: `s3_config` argument, default config at
`~/.config/copernicus_explorer/s3.conf`, then environment variables (`S3_*`,
then `AWS_*`). The config file uses rclone-style INI format where the section
name matches the bucket name:

```ini
[my-bucket]
access_key_id = ...
secret_access_key = ...
region = sbg
endpoint = https://s3.sbg.perf.cloud.ovh.net
```

If the file contains multiple sections, the one matching the bucket from the
`s3://` URI is used. If no section matches, resolution falls back to
environment variables.

| Variable (checked first) | Fallback variable |
|--------------------------|-------------------|
| `S3_ACCESS_KEY_ID` | `AWS_ACCESS_KEY_ID` |
| `S3_SECRET_ACCESS_KEY` | `AWS_SECRET_ACCESS_KEY` |
| `S3_ENDPOINT` | `AWS_ENDPOINT_URL` |
| `S3_REGION` | `AWS_REGION` |

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

# Search using a GeoJSON file as the area of interest
copernicus-explorer search sentinel-2 -p L2A --geojson roi.geojson -c 30
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
| `--geojson FILE` | GeoJSON file defining the area of interest |
| `-n, --max-results N` | Maximum number of results (default: `10`) |

### download

Download one or more scenes by name or by CDSE product ID. Requires authentication.

```bash
# Single scene by name
copernicus-explorer download \
  "S2B_MSIL2A_20260315T105019_N0512_R051_T31TCJ_20260315T144522.SAFE" \
  -o ./data

# Multiple scenes concurrently (max 4 in parallel by default)
copernicus-explorer download \
  "S2B_MSIL2A_20260315T105019_N0512_R051_T31TCJ_20260315T144522.SAFE" \
  "S2A_MSIL2A_20260317T104021_N0512_R008_T31TCJ_20260317T160837.SAFE" \
  -o ./data -j 2

# Download by product UUID (skips the name-to-ID resolution query)
copernicus-explorer download --id \
  "a1b2c3d4-e5f6-7890-abcd-ef1234567890" \
  -o ./data

# Download directly to an S3-compatible bucket
copernicus-explorer download --id \
  "a1b2c3d4-e5f6-7890-abcd-ef1234567890" \
  -o s3://my-bucket/SAFE/ --s3-config ~/.config/copernicus_explorer/s3.conf
```

| Flag | Description |
|------|-------------|
| `SCENES...` | One or more scene names or product IDs (depending on `--id`) |
| `--id` | Treat arguments as CDSE product UUIDs instead of scene names |
| `-o, --output-dir DIR or S3 URI` | Output directory or `s3://bucket/prefix/` (default: `.`) |
| `--s3-config FILE` | Path to S3 credentials config file (rclone-style INI) |
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
| `SearchQuery` | `SearchQuery(satellite)` | `.product(str)`, `.dates(start, end)`, `.tile(str)`, `.max_cloud_cover(float)`, `.geometry_point(Point)`, `.geometry_bbox(BoundingBox)`, `.geometry_geojson(str)`, `.max_results(int)`, `.execute()` |
| `Product` | returned by `SearchQuery.execute()` | `.name`, `.id`, `.acquisition_date`, `.publication_date`, `.online`, `.cloud_cover` |
| `Point` | `Point(lat, lon)` | `.lat`, `.lon` |
| `BoundingBox` | `BoundingBox((lat, lon), (lat, lon))` | `.upper_left`, `.lower_right` |

### Functions

| Function | Description |
|----------|-------------|
| `get_access_token(username, password)` | Authenticate and return an access token string |
| `get_access_token_from_env()` | Authenticate using `COPERNICUS_USER` / `COPERNICUS_PASS` env vars |
| `download_scene(scene_name, directory, token, s3_config=None)` | Download a single scene by name; returns the output file path (or S3 URI) |
| `download_by_id(id, directory, token, s3_config=None)` | Download a single product by CDSE UUID; skips name-to-ID resolution |
| `download_products(products, directory, token, max_concurrent=4, s3_config=None)` | Download multiple products concurrently; returns a list of paths (`None` on failure) |
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
