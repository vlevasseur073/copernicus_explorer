use std::io::{self, BufRead, Write};

use chrono::{Duration, Utc};
use copernicus_explorer::{
    Geometry, Point, Products, Satellite, SearchQuery, download_products, download_scene,
    get_access_token, get_access_token_from_env,
};

#[tokio::main]
async fn main() -> Result<(), copernicus_explorer::CopernicusError> {
    let stdin = io::stdin();

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

    print!("\nDownload? [y]es one / [a]ll / [N]o: ");
    io::stdout().flush().unwrap();
    let mut answer = String::new();
    stdin.lock().read_line(&mut answer).unwrap();

    match answer.trim() {
        "a" | "all" => {
            let token = authenticate().await?;

            println!(
                "\nDownloading all {} products (max 3 concurrent)...\n",
                products.len()
            );
            let results = download_products(&products, ".".as_ref(), &token, 3).await;

            let mut ok = 0;
            let mut failed = 0;
            for (product, result) in products.iter().zip(results.iter()) {
                match result {
                    Ok(path) => {
                        println!("  OK: {} -> {}", product.name, path.display());
                        ok += 1;
                    }
                    Err(e) => {
                        println!("  FAILED: {} -> {e}", product.name);
                        failed += 1;
                    }
                }
            }
            println!("\n{ok} succeeded, {failed} failed.");
        }

        "y" | "yes" => {
            let token = authenticate().await?;

            print!("Enter the scene name: ");
            io::stdout().flush().unwrap();
            let mut name = String::new();
            stdin.lock().read_line(&mut name).unwrap();
            let name = name.trim();

            let path = download_scene(name, ".".as_ref(), &token).await?;
            println!("Downloaded to {}", path.display());
        }

        _ => {
            println!("Nothing to download, bye!");
        }
    }

    Ok(())
}

async fn authenticate() -> Result<String, copernicus_explorer::CopernicusError> {
    match get_access_token("user@example.com", "password").await {
        Ok(token) => {
            println!("Authentication successful");
            Ok(token)
        }
        Err(e) => {
            println!("Authentication failed: {e}");
            println!("Trying environment variables COPERNICUS_USER / COPERNICUS_PASS...");
            get_access_token_from_env().await
        }
    }
}
