"""Copernicus Explorer — browse and download Sentinel products from CDSE.

The native Rust extension is compiled by maturin and installed alongside
this package.  We re-export everything here so that
``from copernicus_explorer_py import ...`` keeps working.
"""

from copernicus_explorer_py.copernicus_explorer_py import (
    BoundingBox,
    Point,
    Product,
    Satellite,
    SearchQuery,
    download_by_id,
    download_products,
    download_scene,
    format_products,
    get_access_token,
    get_access_token_from_env,
    get_scene_id,
    print_products,
)

__all__ = [
    "BoundingBox",
    "Point",
    "Product",
    "Satellite",
    "SearchQuery",
    "download_by_id",
    "download_products",
    "download_scene",
    "format_products",
    "get_access_token",
    "get_access_token_from_env",
    "get_scene_id",
    "print_products",
]
