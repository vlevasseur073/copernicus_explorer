use std::path::PathBuf;
use std::process;

use chrono::{DateTime, Duration, NaiveDate, Utc};
use clap::{Parser, Subcommand};

use copernicus_explorer::{
    BoundingBox, Geometry, Point, Satellite, SearchQuery, download_scene, get_access_token,
    get_access_token_from_env, print_products,
};

// `clap` uses Rust's derive macros to turn struct/enum definitions into
// a full CLI parser -- argument names, types, help text, and validation
// are all generated from the code below.  No manual argument parsing!
//
// The `#[command(...)]` and `#[arg(...)]` attributes control how each
// field appears on the command line.
#[derive(Parser)]
#[command(
    name = "copernicus-explorer",
    version,
    about = "Browse and download Sentinel products from the Copernicus Data Space Ecosystem (CDSE)"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

/// Each variant of this enum becomes a subcommand (e.g. `search`, `download`).
///
/// Clap maps `Commands::Search { ... }` to `copernicus-explorer search ...`
/// on the command line.  The fields of each variant become that subcommand's
/// arguments and flags.
#[derive(Subcommand)]
enum Commands {
    /// Search the CDSE catalogue for satellite products.
    Search {
        /// Satellite mission to search.
        #[arg(value_enum)]
        satellite: Satellite,

        /// Product type filter (e.g. L2A, L1C, GRD).
        #[arg(short, long)]
        product: Option<String>,

        /// Start date for acquisition window (YYYY-MM-DD).
        /// Defaults to 30 days ago.
        #[arg(long)]
        start: Option<String>,

        /// End date for acquisition window (YYYY-MM-DD).
        /// Defaults to today.
        #[arg(long)]
        end: Option<String>,

        /// Sentinel-2 tile identifier (e.g. T31TFJ).
        #[arg(long)]
        tile: Option<String>,

        /// Maximum cloud cover percentage (0-100).
        #[arg(short, long)]
        cloud: Option<f64>,

        /// Point geometry as lat,lon (e.g. 43.6,1.44).
        #[arg(long, value_name = "LAT,LON")]
        point: Option<String>,

        /// Bounding box as top_lat,left_lon,bottom_lat,right_lon.
        #[arg(long, value_name = "TLAT,LLON,BLAT,RLON")]
        bbox: Option<String>,

        /// Maximum number of results.
        #[arg(short = 'n', long, default_value = "10")]
        max_results: u32,
    },

    /// Download a scene by name.
    Download {
        /// Full scene name (e.g. S2B_MSIL2A_20200804T183919_...).
        scene: String,

        /// Directory to save the downloaded file.
        #[arg(short, long, default_value = ".")]
        output_dir: PathBuf,

        /// Username (reads COPERNICUS_USER env var if omitted).
        #[arg(short, long)]
        user: Option<String>,

        /// Password (reads COPERNICUS_PASS env var if omitted).
        #[arg(short = 'P', long)]
        pass: Option<String>,
    },

    /// Test authentication and print a token summary.
    Auth {
        /// Username (reads COPERNICUS_USER env var if omitted).
        #[arg(short, long)]
        user: Option<String>,

        /// Password (reads COPERNICUS_PASS env var if omitted).
        #[arg(short = 'P', long)]
        pass: Option<String>,
    },
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Search {
            satellite,
            product,
            start,
            end,
            tile,
            cloud,
            point,
            bbox,
            max_results,
        } => run_search(
            satellite,
            product,
            start,
            end,
            tile,
            cloud,
            point,
            bbox,
            max_results,
        ),
        Commands::Download {
            scene,
            output_dir,
            user,
            pass,
        } => run_download(&scene, &output_dir, user.as_deref(), pass.as_deref()),
        Commands::Auth { user, pass } => run_auth(user.as_deref(), pass.as_deref()),
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        process::exit(1);
    }
}

fn run_search(
    satellite: Satellite,
    product: Option<String>,
    start: Option<String>,
    end: Option<String>,
    tile: Option<String>,
    cloud: Option<f64>,
    point: Option<String>,
    bbox: Option<String>,
    max_results: u32,
) -> Result<(), copernicus_explorer::CopernicusError> {
    let mut query = SearchQuery::new(satellite);

    if let Some(p) = product {
        query = query.product(p);
    }

    let (start_dt, end_dt) = parse_date_range(start.as_deref(), end.as_deref())?;
    query = query.dates(start_dt, end_dt);

    if let Some(t) = tile {
        query = query.tile(t);
    }

    if let Some(c) = cloud {
        query = query.max_cloud_cover(c);
    }

    if let Some(geom) = parse_geometry(point.as_deref(), bbox.as_deref())? {
        query = query.geometry(geom);
    }

    query = query.max_results(max_results);

    eprintln!(
        "Searching {sat} products...\n",
        sat = satellite.collection_name()
    );

    let products = query.execute()?;
    print_products(&products);
    Ok(())
}

fn run_download(
    scene: &str,
    output_dir: &PathBuf,
    user: Option<&str>,
    pass: Option<&str>,
) -> Result<(), copernicus_explorer::CopernicusError> {
    let token = resolve_token(user, pass)?;

    eprintln!("Resolving scene ID for:\n  {scene}\n");
    let path = download_scene(scene, output_dir, &token)?;
    eprintln!("\nDownload complete: {}", path.display());
    Ok(())
}

fn run_auth(
    user: Option<&str>,
    pass: Option<&str>,
) -> Result<(), copernicus_explorer::CopernicusError> {
    let token = resolve_token(user, pass)?;
    let preview_len = 20.min(token.len());
    println!("Authentication successful!");
    println!("Token: {}...", &token[..preview_len]);
    println!("Length: {} characters", token.len());
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn resolve_token(
    user: Option<&str>,
    pass: Option<&str>,
) -> Result<String, copernicus_explorer::CopernicusError> {
    match (user, pass) {
        (Some(u), Some(p)) => get_access_token(u, p),
        _ => get_access_token_from_env(),
    }
}

/// Parse --start / --end strings into `DateTime<Utc>`.
/// Defaults: start = 30 days ago, end = now.
fn parse_date_range(
    start: Option<&str>,
    end: Option<&str>,
) -> Result<(DateTime<Utc>, DateTime<Utc>), copernicus_explorer::CopernicusError> {
    let end_dt = match end {
        Some(s) => parse_date(s)?,
        None => Utc::now(),
    };
    let start_dt = match start {
        Some(s) => parse_date(s)?,
        None => end_dt - Duration::days(30),
    };
    Ok((start_dt, end_dt))
}

fn parse_date(s: &str) -> Result<DateTime<Utc>, copernicus_explorer::CopernicusError> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map(|d| {
            d.and_hms_opt(0, 0, 0)
                .unwrap()
                .and_utc()
        })
        .map_err(|e| {
            copernicus_explorer::CopernicusError::InvalidArgument(format!(
                "invalid date '{s}' (expected YYYY-MM-DD): {e}"
            ))
        })
}

/// Parse --point or --bbox into a Geometry.
fn parse_geometry(
    point: Option<&str>,
    bbox: Option<&str>,
) -> Result<Option<Geometry>, copernicus_explorer::CopernicusError> {
    if let Some(p) = point {
        let parts: Vec<f64> = p
            .split(',')
            .map(|s| s.trim().parse::<f64>())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| {
                copernicus_explorer::CopernicusError::InvalidArgument(format!(
                    "invalid --point '{p}' (expected lat,lon): {e}"
                ))
            })?;
        if parts.len() != 2 {
            return Err(copernicus_explorer::CopernicusError::InvalidArgument(
                format!("--point requires exactly 2 values (lat,lon), got {}", parts.len()),
            ));
        }
        return Ok(Some(Geometry::Point(Point::new(parts[0], parts[1]))));
    }

    if let Some(b) = bbox {
        let parts: Vec<f64> = b
            .split(',')
            .map(|s| s.trim().parse::<f64>())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| {
                copernicus_explorer::CopernicusError::InvalidArgument(format!(
                    "invalid --bbox '{b}' (expected tlat,llon,blat,rlon): {e}"
                ))
            })?;
        if parts.len() != 4 {
            return Err(copernicus_explorer::CopernicusError::InvalidArgument(
                format!(
                    "--bbox requires exactly 4 values (tlat,llon,blat,rlon), got {}",
                    parts.len()
                ),
            ));
        }
        return Ok(Some(Geometry::BoundingBox(BoundingBox::new(
            (parts[0], parts[1]),
            (parts[2], parts[3]),
        ))));
    }

    Ok(None)
}
