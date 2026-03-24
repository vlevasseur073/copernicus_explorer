use std::fmt;

use clap::ValueEnum;
use serde::Deserialize;

/// The Sentinel satellite missions available on CDSE.
///
/// This is a Rust **enum** -- unlike Julia, each variant is a distinct type
/// checked at compile time.  No risk of typos like "SENTNEL-2".
///
/// `Clone` and `Copy` let us pass the enum by value (it's just a small tag).
/// `Debug` gives us `{:?}` formatting for development.
///
/// # `clap::ValueEnum`
///
/// Deriving `ValueEnum` lets clap parse command-line strings like
/// `sentinel-2` directly into this enum.  The `#[value(name = "...")]`
/// attribute controls the exact string the user types on the CLI.
/// Without it, clap would use the variant name in kebab-case
/// (e.g. `sentinel1`).
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Satellite {
    #[value(name = "sentinel-1")]
    Sentinel1,
    #[value(name = "sentinel-2")]
    Sentinel2,
    #[value(name = "sentinel-3")]
    Sentinel3,
    #[value(name = "sentinel-5p")]
    Sentinel5P,
    #[value(name = "sentinel-6")]
    Sentinel6,
}

impl Satellite {
    /// Returns the collection name string expected by the CDSE OData API.
    ///
    /// In Julia this was just a raw string "SENTINEL-2".  Here we tie the
    /// string to the enum so the compiler guarantees we never misspell it.
    pub fn collection_name(&self) -> &'static str {
        match self {
            Satellite::Sentinel1 => "SENTINEL-1",
            Satellite::Sentinel2 => "SENTINEL-2",
            Satellite::Sentinel3 => "SENTINEL-3",
            Satellite::Sentinel5P => "SENTINEL-5P",
            Satellite::Sentinel6 => "SENTINEL-6",
        }
    }

    /// Returns the known product types for this satellite.
    ///
    /// # `&'static [&'static str]` -- a slice of string literals
    ///
    /// This return type means "a borrowed reference to a fixed-size array
    /// of borrowed string literals, both living for the entire program."
    /// Since these are compile-time constants embedded in the binary, no
    /// heap allocation happens at all.
    ///
    /// The product strings are the substrings that appear in scene names
    /// on the CDSE catalogue (e.g. a Sentinel-2 L2A scene name contains
    /// "MSIL2A", so we list "L2A" which matches via `contains`).
    pub fn known_products(&self) -> &'static [&'static str] {
        match self {
            Satellite::Sentinel1 => &["GRD", "GRDH", "GRDM", "SLC", "OCN", "RAW"],
            Satellite::Sentinel2 => &["L1C", "L2A"],
            Satellite::Sentinel3 => &[
                // OLCI
                "OL_1_EFR", "OL_1_ERR", "OL_2_LFR", "OL_2_LRR", "OL_2_WFR", "OL_2_WRR",
                // SLSTR
                "SL_1_RBT", "SL_2_LST", "SL_2_WST", "SL_2_FRP", "SL_2_AOD", // SRAL
                "SR_1_SRA", "SR_2_LAN", "SR_2_WAT", // Synergy
                "SY_2_SYN", "SY_2_V10", "SY_2_VG1", "SY_2_VGP",
            ],
            Satellite::Sentinel5P => &[
                "L1B_IR_SIR",
                "L1B_IR_UVN",
                "L1B_RA_BD1",
                "L1B_RA_BD2",
                "L1B_RA_BD3",
                "L1B_RA_BD4",
                "L1B_RA_BD5",
                "L1B_RA_BD6",
                "L1B_RA_BD7",
                "L1B_RA_BD8",
                "L2__AER_AI",
                "L2__AER_LH",
                "L2__CH4___",
                "L2__CLOUD_",
                "L2__CO____",
                "L2__HCHO__",
                "L2__NO2___",
                "L2__NP_BD3",
                "L2__NP_BD6",
                "L2__NP_BD7",
                "L2__O3____",
                "L2__O3_TCL",
                "L2__SO2___",
            ],
            Satellite::Sentinel6 => &["MW_2__AMR", "P4_1B_LR", "P4_2__LR"],
        }
    }

    /// Check whether a product type string is valid for this satellite.
    ///
    /// The check is case-insensitive and matches if the known product
    /// contains the query or vice versa, so both "L2A" and "MSIL2A" work.
    pub fn is_valid_product(&self, product: &str) -> bool {
        let product_upper = product.to_uppercase();
        self.known_products().iter().any(|known| {
            let known_upper = known.to_uppercase();
            known_upper.contains(&product_upper) || product_upper.contains(&known_upper)
        })
    }
}

/// Implement `Display` so we can use `{}` in `println!`.
///
/// `Display` is Rust's equivalent of Julia's `show()` method.
/// `fmt::Formatter` is the output stream, and `write!` is like `print` but
/// writes into the formatter.
impl fmt::Display for Satellite {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.collection_name())
    }
}

/// A single product returned by the CDSE catalogue search.
///
/// `#[derive(Deserialize)]` tells serde to generate JSON parsing code
/// automatically.  The `#[serde(rename = "...")]` attributes map JSON
/// field names (PascalCase from the API) to our Rust field names
/// (snake_case, idiomatic Rust).
///
/// `Option<f64>` means "this field might be absent" -- Sentinel-1 products
/// have no cloud cover.  In Julia you'd use `missing`; in Rust it's `Option`.
#[derive(Debug, Deserialize)]
pub struct Product {
    #[serde(rename = "Name")]
    pub name: String,

    #[serde(rename = "Id")]
    pub id: String,

    #[serde(
        rename = "ContentDate",
        deserialize_with = "deserialize_acquisition_date"
    )]
    pub acquisition_date: String,

    #[serde(rename = "PublicationDate")]
    pub publication_date: String,

    #[serde(rename = "Online")]
    pub online: bool,

    #[serde(skip)]
    pub cloud_cover: Option<f64>,
}

/// The CDSE API nests the acquisition date inside `{"Start": "...", "End": "..."}`.
/// This custom deserializer extracts the "Start" value.
fn deserialize_acquisition_date<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    struct ContentDate {
        #[serde(rename = "Start")]
        start: String,
    }
    let cd = ContentDate::deserialize(deserializer)?;
    Ok(cd.start)
}

impl fmt::Display for Product {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{name}  (acquired: {date}, cloud: {cloud})",
            name = self.name,
            date = self.acquisition_date,
            cloud = match self.cloud_cover {
                Some(c) => format!("{c:.1}%"),
                None => "N/A".to_string(),
            },
        )
    }
}

/// Format a slice of products as an aligned table (same layout as the CLI).
///
/// Returns the full table as a `String` so it can be printed, logged, or
/// returned to a foreign language binding.
///
/// # Why a free function instead of a method?
///
/// We're formatting a *collection* of products, not a single one.  Rust
/// doesn't let you implement `Display` on `Vec<Product>` (orphan rule),
/// so a free function is the idiomatic choice.
pub fn format_products(products: &[Product]) -> String {
    use std::fmt::Write;

    let mut buf = String::new();

    writeln!(
        buf,
        "{id:<40} {cloud:>6}  {date:<28} NAME",
        id = "ID",
        cloud = "CLOUD",
        date = "ACQUISITION DATE",
    )
    .unwrap();
    writeln!(buf, "{}", "-".repeat(130)).unwrap();

    for p in products {
        let cloud_str = match p.cloud_cover {
            Some(c) => format!("{c:.1}%"),
            None => "N/A".into(),
        };
        writeln!(
            buf,
            "{id:<40} {cloud:>6}  {date:<28} {name}",
            id = p.id,
            cloud = cloud_str,
            date = p.acquisition_date,
            name = p.name,
        )
        .unwrap();
    }

    writeln!(buf, "\n{count} product(s) found.", count = products.len()).unwrap();
    buf
}

/// Print a slice of products as an aligned table to stdout.
///
/// Convenience wrapper around [`format_products`] for quick use.
pub fn print_products(products: &[Product]) {
    print!("{}", format_products(products));
}

/// Raw response envelope from the CDSE OData API.
///
/// The API returns `{"value": [...products...]}`.  We deserialize the
/// outer wrapper to get at the inner `Vec<RawProduct>`.
#[derive(Debug, Deserialize)]
pub(crate) struct ODataResponse {
    pub value: Vec<RawProduct>,
}

/// A raw product as returned by the API, before we clean it up.
///
/// This includes the `Attributes` array that contains cloud cover info.
/// We parse this into a cleaner `Product` in a second step.
#[derive(Debug, Deserialize)]
pub(crate) struct RawProduct {
    #[serde(rename = "Name")]
    pub name: String,

    #[serde(rename = "Id")]
    pub id: String,

    #[serde(
        rename = "ContentDate",
        deserialize_with = "deserialize_acquisition_date"
    )]
    pub acquisition_date: String,

    #[serde(rename = "PublicationDate")]
    pub publication_date: String,

    #[serde(rename = "Online")]
    pub online: bool,

    #[serde(rename = "Attributes", default)]
    pub attributes: Vec<Attribute>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Attribute {
    #[serde(rename = "Name")]
    pub name: String,

    #[serde(rename = "Value")]
    pub value: Option<serde_json::Value>,
}

impl RawProduct {
    /// Convert to a clean `Product`, extracting cloud cover from attributes.
    pub fn into_product(self) -> Product {
        let cloud_cover = self
            .attributes
            .iter()
            .find(|a| a.name == "cloudCover")
            .and_then(|a| a.value.as_ref())
            .and_then(|v| {
                v.as_f64()
                    .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
            });

        Product {
            name: self.name,
            id: self.id,
            acquisition_date: self.acquisition_date,
            publication_date: self.publication_date,
            online: self.online,
            cloud_cover,
        }
    }
}
