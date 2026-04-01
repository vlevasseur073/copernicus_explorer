"""Command-line interface for Copernicus Explorer.

Mirrors the Rust CLI (``copernicus-explorer search|download|auth``).
"""

from __future__ import annotations

import sys
from datetime import datetime, timedelta, timezone

import click

from copernicus_explorer_py import (
    BoundingBox,
    Point,
    Product,
    Satellite,
    SearchQuery,
    download_by_id,
    download_products,
    download_scene,
    get_access_token,
    get_access_token_from_env,
    print_products,
)

SATELLITES = {
    "sentinel-1": Satellite.sentinel1,
    "sentinel-2": Satellite.sentinel2,
    "sentinel-3": Satellite.sentinel3,
    "sentinel-5p": Satellite.sentinel5p,
    "sentinel-6": Satellite.sentinel6,
}


def _resolve_token(user: str | None, password: str | None) -> str:
    if user and password:
        return get_access_token(user, password)
    return get_access_token_from_env()


def _parse_point(value: str) -> tuple[float, float]:
    parts = [float(x.strip()) for x in value.split(",")]
    if len(parts) != 2:
        raise click.BadParameter(
            f"expected 2 values (lat,lon), got {len(parts)}"
        )
    return parts[0], parts[1]


def _parse_bbox(value: str) -> tuple[float, ...]:
    parts = [float(x.strip()) for x in value.split(",")]
    if len(parts) != 4:
        raise click.BadParameter(
            f"expected 4 values (tlat,llon,blat,rlon), got {len(parts)}"
        )
    return tuple(parts)


@click.group()
@click.version_option()
def main() -> None:
    """Browse and download Sentinel products from the Copernicus Data Space Ecosystem (CDSE)."""


@main.command()
@click.argument(
    "satellite", type=click.Choice(list(SATELLITES.keys()), case_sensitive=False)
)
@click.option("-p", "--product", default=None, help="Product type filter (e.g. L2A, L1C, GRD).")
@click.option("--start", default=None, help="Start date (YYYY-MM-DD). Defaults to 30 days ago.")
@click.option("--end", default=None, help="End date (YYYY-MM-DD). Defaults to today.")
@click.option("--tile", default=None, help="Sentinel-2 tile identifier (e.g. T31TFJ).")
@click.option("-c", "--cloud", type=float, default=None, help="Maximum cloud cover % (0-100).")
@click.option("--point", default=None, metavar="LAT,LON", help="Point geometry (e.g. 43.6,1.44).")
@click.option("--bbox", default=None, metavar="TLAT,LLON,BLAT,RLON", help="Bounding box geometry.")
@click.option("--geojson", default=None, type=click.Path(exists=True), help="GeoJSON file defining the area of interest.")
@click.option("-n", "--max-results", type=int, default=10, show_default=True, help="Maximum number of results.")
def search(
    satellite: str,
    product: str | None,
    start: str | None,
    end: str | None,
    tile: str | None,
    cloud: float | None,
    point: str | None,
    bbox: str | None,
    geojson: str | None,
    max_results: int,
) -> None:
    """Search the CDSE catalogue for satellite products."""
    geo_count = sum(x is not None for x in (point, bbox, geojson))
    if geo_count > 1:
        raise click.UsageError("--point, --bbox, and --geojson are mutually exclusive.")

    sat_factory = SATELLITES[satellite]
    query = SearchQuery(sat_factory())

    if product:
        query.product(product)

    end_dt = (
        datetime.strptime(end, "%Y-%m-%d").replace(tzinfo=timezone.utc)
        if end
        else datetime.now(timezone.utc)
    )
    start_dt = (
        datetime.strptime(start, "%Y-%m-%d").replace(tzinfo=timezone.utc)
        if start
        else end_dt - timedelta(days=30)
    )
    query.dates(start_dt, end_dt)

    if tile:
        query.tile(tile)

    if cloud is not None:
        query.max_cloud_cover(cloud)

    if point:
        lat, lon = _parse_point(point)
        query.geometry_point(Point(lat, lon))
    elif bbox:
        parts = _parse_bbox(bbox)
        query.geometry_bbox(
            BoundingBox((parts[0], parts[1]), (parts[2], parts[3]))
        )
    elif geojson:
        query.geometry_geojson(geojson)

    query.max_results(max_results)

    click.echo(
        f"Searching {sat_factory().collection_name()} products...\n",
        err=True,
    )

    products = query.execute()
    print_products(products)


@main.command()
@click.argument("scenes", nargs=-1, required=True)
@click.option("--id", "by_id", is_flag=True, default=False, help="Treat arguments as CDSE product UUIDs instead of scene names.")
@click.option("-o", "--output-dir", default=".", show_default=True, help="Directory to save the downloaded file(s).")
@click.option("-j", "--concurrent", type=int, default=4, show_default=True, help="Maximum concurrent downloads.")
@click.option("-u", "--user", default=None, help="Username (reads COPERNICUS_USER env var if omitted).")
@click.option("-P", "--password", default=None, help="Password (reads COPERNICUS_PASS env var if omitted).")
def download(
    scenes: tuple[str, ...],
    by_id: bool,
    output_dir: str,
    concurrent: int,
    user: str | None,
    password: str | None,
) -> None:
    """Download one or more scenes by name or by CDSE product ID.

    By default the positional arguments are treated as scene names and
    each name is resolved to a CDSE UUID before downloading.  Pass --id
    to skip the resolution step and treat the arguments as UUIDs directly
    (useful after a previous search).
    """
    token = _resolve_token(user, password)

    if by_id:
        _download_by_ids(scenes, output_dir, concurrent, token)
    else:
        _download_by_names(scenes, output_dir, concurrent, token)


def _download_by_names(
    scenes: tuple[str, ...],
    output_dir: str,
    concurrent: int,
    token: str,
) -> None:
    if len(scenes) == 1:
        scene = scenes[0]
        click.echo(f"Resolving scene ID for:\n  {scene}\n", err=True)
        path = download_scene(scene, output_dir, token)
        click.echo(f"\nDownload complete: {path}", err=True)
    else:
        click.echo(
            f"Downloading {len(scenes)} scenes (max {concurrent} concurrent)...\n",
            err=True,
        )
        products = [
            Product(name=s, id="", acquisition_date="", publication_date="", online=True)
            for s in scenes
        ]
        results = download_products(products, output_dir, token, concurrent)
        _report_batch_results(scenes, results)


def _download_by_ids(
    ids: tuple[str, ...],
    output_dir: str,
    concurrent: int,
    token: str,
) -> None:
    if len(ids) == 1:
        click.echo(f"Downloading product by ID:\n  {ids[0]}\n", err=True)
        path = download_by_id(ids[0], output_dir, token)
        click.echo(f"\nDownload complete: {path}", err=True)
    else:
        click.echo(
            f"Downloading {len(ids)} products by ID (max {concurrent} concurrent)...\n",
            err=True,
        )
        products = [
            Product(name="", id=pid, acquisition_date="", publication_date="", online=True)
            for pid in ids
        ]
        results = download_products(products, output_dir, token, concurrent)
        _report_batch_results(ids, results)


def _report_batch_results(
    labels: tuple[str, ...] | list[str],
    results: list[str | None],
) -> None:
    failures = 0
    for label, result in zip(labels, results):
        if result is not None:
            click.echo(f"  OK: {label} -> {result}", err=True)
        else:
            click.echo(f"  FAILED: {label}", err=True)
            failures += 1

    ok = len(labels) - failures
    click.echo(f"\n{ok} succeeded, {failures} failed.", err=True)

    if failures > 0:
        sys.exit(1)


@main.command()
@click.option("-u", "--user", default=None, help="Username (reads COPERNICUS_USER env var if omitted).")
@click.option("-P", "--password", default=None, help="Password (reads COPERNICUS_PASS env var if omitted).")
def auth(user: str | None, password: str | None) -> None:
    """Test authentication and print a token summary."""
    token = _resolve_token(user, password)
    preview = token[:20]
    click.echo("Authentication successful!")
    click.echo(f"Token: {preview}...")
    click.echo(f"Length: {len(token)} characters")


if __name__ == "__main__":
    main()
