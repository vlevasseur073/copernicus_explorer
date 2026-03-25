use std::io::{self, BufRead, Write};

use chrono::{Duration, Utc};
use copernicus_explorer::{
    Geometry, Point, Products, Satellite, SearchQuery, download_scene, get_access_token,
    get_access_token_from_env,
};

fn main() -> Result<(), copernicus_explorer::CopernicusError> {
    let stdin = io::stdin();

    // Search (no authentication required)
    let products = SearchQuery::new(Satellite::Sentinel2)
        .product("L2A")
        .dates(Utc::now() - Duration::days(7), Utc::now())
        .max_cloud_cover(20.0)
        .geometry(Geometry::Point(Point::new(43.6, 1.44)))
        .max_results(5)
        .execute()?;

    println!("{}", Products(&products));

    loop {
        print!("Want to download a scene? (y/N) ");
        io::stdout().flush().unwrap();
        let mut answer = String::new();
        stdin.lock().read_line(&mut answer).unwrap();
        if answer.trim() != "y" {
            break;
        }

        print!("Enter the scene name: ");
        io::stdout().flush().unwrap();
        let mut name = String::new();
        stdin.lock().read_line(&mut name).unwrap();
        let name = name.trim();

        let token = match get_access_token("user@example.com", "password") {
            Ok(token) => {
                println!("Authentication successful, token: {token}");
                token
            }
            Err(e) => {
                println!("Authentication failed: {e}");
                println!("Try with environment variables: COPERNICUS_USER and COPERNICUS_PASS");
                get_access_token_from_env()?
            }
        };

        let path = download_scene(name, ".".as_ref(), &token)?;
        println!("Downloaded to {}", path.display());
    }

    Ok(())
}
