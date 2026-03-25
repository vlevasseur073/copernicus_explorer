from datetime import datetime, timedelta, timezone

from copernicus_explorer_py import (
    Product,
    Satellite,
    SearchQuery,
    Point,
    download_products,
    download_scene,
    get_access_token,
    get_access_token_from_env,
    print_products,
)


def authenticate() -> str:
    try:
        token = get_access_token("user@example.com", "password")
        print("Authentication successful")
        return token
    except Exception as e:
        print(f"Authentication failed: {e}")
        print("Trying environment variables COPERNICUS_USER / COPERNICUS_PASS...")
        return get_access_token_from_env()


# Search (no authentication required)
query = SearchQuery(Satellite.sentinel2())
query.product("L2A")
query.dates(
    datetime.now(timezone.utc) - timedelta(days=20),
    datetime.now(timezone.utc),
)
query.max_cloud_cover(30.0)
query.geometry_point(Point(43.6, 1.44))
query.max_results(5)

products = query.execute()
print_products(products)

answer = input("\nDownload? [y]es one / [a]ll / [N]o: ").strip().lower()

if answer in ("a", "all"):
    token = authenticate()

    print(f"\nDownloading all {len(products)} products (max 3 concurrent)...\n")
    results = download_products(products, ".", token, max_concurrent=3)

    ok = 0
    failed = 0
    for product, result in zip(products, results):
        if result is not None:
            print(f"  OK: {product.name} -> {result}")
            ok += 1
        else:
            print(f"  FAILED: {product.name}")
            failed += 1

    print(f"\n{ok} succeeded, {failed} failed.")

elif answer in ("y", "yes"):
    token = authenticate()

    name = input("Enter the scene name: ").strip()
    path = download_scene(name, ".", token)
    print(f"Downloaded to {path}")

else:
    print("Nothing to download, bye!")
