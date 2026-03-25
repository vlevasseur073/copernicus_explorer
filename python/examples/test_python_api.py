from datetime import datetime, timedelta, timezone

from copernicus_explorer_py import (
    Satellite,
    SearchQuery,
    Point,
    get_access_token,
    get_access_token_from_env,
    download_scene,
    print_products,
)

# Search (no authentication required)
query = SearchQuery(Satellite.sentinel2())
query.product("L2A")
query.dates(
    datetime.now(timezone.utc) - timedelta(days=7),
    datetime.now(timezone.utc),
)
query.max_cloud_cover(20.0)
query.geometry_point(Point(43.6, 1.44))
query.max_results(5)

products = query.execute()

print_products(products)

answer = "y"
while answer == "y":
    print("Want to download a scene? (y/N)")
    answer = input()
    if answer == "y":
        print("Enter the scene name:")
        name = input()
        # Download (requires credentials)
        try:
            token = get_access_token("user@example.com", "password")
            print(f"Authentication successful, token: {token}")
        except Exception as e:
            print(f"Authentication failed: {e}")
            print("Try with environment variables: COPERNICUS_USER and COPERNICUS_PASS")
            token = get_access_token_from_env()

        path = download_scene(name, ".", token)
        print(f"Downloaded to {path}")
    else:
        break
