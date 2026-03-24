use chrono::{DateTime, Utc};

use crate::error::{CopernicusError, Result};
use crate::geometry::Geometry;
use crate::models::{ODataResponse, Product, Satellite};

const CATALOGUE_URL: &str = "https://catalogue.dataspace.copernicus.eu/odata/v1/Products";

/// A builder for constructing CDSE catalogue search queries.
///
/// # The Builder Pattern
///
/// In Julia, `search("SENTINEL-2", product="L2A", dates=(...), ...)` uses
/// keyword arguments.  Rust doesn't have keyword arguments, so we use the
/// **builder pattern** instead:
///
/// ```rust,no_run
/// use copernicus_explorer::search::SearchQuery;
/// use copernicus_explorer::models::Satellite;
/// use chrono::Utc;
///
/// let results = SearchQuery::new(Satellite::Sentinel2)
///     .product("L2A")
///     .max_cloud_cover(20.0)
///     .max_results(10)
///     .execute();
/// ```
///
/// Each method takes `self` **by value** (consuming it) and returns `Self`,
/// enabling the fluent chaining.  The `Option<T>` fields start as `None`
/// and get filled in as you call methods.
pub struct SearchQuery {
    satellite: Satellite,
    product: Option<String>,
    date_range: Option<(DateTime<Utc>, DateTime<Utc>)>,
    tile: Option<String>,
    max_cloud_cover: Option<f64>,
    geometry: Option<Geometry>,
    max_results: u32,
}

impl SearchQuery {
    /// Start building a query for the given satellite.
    pub fn new(satellite: Satellite) -> Self {
        Self {
            satellite,
            product: None,
            date_range: None,
            tile: None,
            max_cloud_cover: None,
            geometry: None,
            max_results: 100,
        }
    }

    /// Filter by product type (e.g. "L2A", "L1C", "GRD").
    ///
    /// `impl Into<String>` is a Rust trick that accepts both `&str` and
    /// `String` -- the caller can pass either without worrying about
    /// conversions.
    pub fn product(mut self, product: impl Into<String>) -> Self {
        self.product = Some(product.into());
        self
    }

    /// Filter by acquisition date range.
    pub fn dates(mut self, start: DateTime<Utc>, end: DateTime<Utc>) -> Self {
        self.date_range = Some((start, end));
        self
    }

    /// Filter by Sentinel-2 tile identifier (e.g. "T31TFJ").
    pub fn tile(mut self, tile: impl Into<String>) -> Self {
        self.tile = Some(tile.into());
        self
    }

    /// Maximum allowable cloud cover as a percentage (0-100).
    pub fn max_cloud_cover(mut self, percent: f64) -> Self {
        self.max_cloud_cover = Some(percent);
        self
    }

    /// Spatial filter: only return products intersecting this geometry.
    pub fn geometry(mut self, geom: Geometry) -> Self {
        self.geometry = Some(geom);
        self
    }

    /// Maximum number of results to return (default: 100).
    pub fn max_results(mut self, n: u32) -> Self {
        self.max_results = n;
        self
    }

    /// Build the OData `$filter` string from all configured filters.
    ///
    /// This is where `Vec<String>` and iterators shine.  We push filter
    /// clauses into a Vec, then `join` them with " and ".
    fn build_filter(&self) -> Result<String> {
        let mut filters: Vec<String> = Vec::new();

        // Collection filter (always present)
        filters.push(format!(
            "Collection/Name eq '{name}'",
            name = self.satellite.collection_name()
        ));

        // Product type filter (with validation)
        if let Some(ref product) = self.product {
            if !self.satellite.is_valid_product(product) {
                let known = self.satellite.known_products().join(", ");
                return Err(CopernicusError::InvalidArgument(format!(
                    "unknown product type '{product}' for {sat}. \
                     Known types: {known}",
                    sat = self.satellite.collection_name(),
                )));
            }
            filters.push(format!("contains(Name,'{product}')"));
        }

        // Date range filter
        if let Some((start, end)) = self.date_range {
            if start > end {
                return Err(CopernicusError::InvalidArgument(
                    "start date must be before end date".into(),
                ));
            }
            let start_str = start.format("%Y-%m-%dT%H:%M:%S%.3fZ");
            let end_str = end.format("%Y-%m-%dT%H:%M:%S%.3fZ");
            filters.push(format!(
                "ContentDate/Start gt {start_str} and ContentDate/Start lt {end_str}"
            ));
        }

        // Tile filter (Sentinel-2 only)
        if let Some(ref tile) = self.tile {
            if !matches!(self.satellite, Satellite::Sentinel2) {
                return Err(CopernicusError::InvalidArgument(
                    "tile filter is only supported for Sentinel-2".into(),
                ));
            }
            // The CDSE catalogue stores tile IDs without the leading "T"
            // (e.g. "14CPE" not "T14CPE"), but users typically include it
            // because scene names use the T-prefixed form.
            let tile_id = tile.strip_prefix('T').unwrap_or(tile);
            let dtype = "OData.CSC.StringAttribute";
            filters.push(format!(
                "Attributes/{dtype}/any(att:att/Name eq 'tileId' and att/{dtype}/Value eq '{tile_id}')"
            ));
        }

        // Cloud cover filter (not for Sentinel-1)
        if let Some(max_clouds) = self.max_cloud_cover {
            if matches!(self.satellite, Satellite::Sentinel1) {
                return Err(CopernicusError::InvalidArgument(
                    "cloud cover filter is not supported for Sentinel-1".into(),
                ));
            }
            let dtype = "OData.CSC.DoubleAttribute";
            filters.push(format!(
                "Attributes/{dtype}/any(att:att/Name eq 'cloudCover' and att/{dtype}/Value lt {max_clouds:.2})"
            ));
        }

        // Geometry filter
        if let Some(ref geom) = self.geometry {
            let wkt = geom.to_wkt();
            filters.push(format!(
                "OData.CSC.Intersects(area=geography'SRID=4326;{wkt}')"
            ));
        }

        Ok(filters.join(" and "))
    }

    /// Execute the search query against the CDSE catalogue.
    ///
    /// Returns a `Vec<Product>` -- Rust's growable array, like Julia's
    /// `Vector`.  Each product is a fully typed struct, not a DataFrame row.
    pub fn execute(&self) -> Result<Vec<Product>> {
        let filter = self.build_filter()?;

        let client = reqwest::blocking::Client::new();
        let response = client
            .get(CATALOGUE_URL)
            .query(&[
                ("$filter", filter.as_str()),
                ("$expand", "Attributes"),
                ("$top", &self.max_results.to_string()),
                ("$orderby", "ContentDate/Start asc"),
            ])
            .send()?;

        if !response.status().is_success() {
            return Err(CopernicusError::SearchFailed(format!(
                "HTTP {status}",
                status = response.status()
            )));
        }

        let odata: ODataResponse = response.json()?;

        if odata.value.is_empty() {
            return Err(CopernicusError::NoResults);
        }

        // Convert raw products to clean Products, keeping only online ones.
        // This is an **iterator chain** -- Rust's equivalent of Julia's
        // piping with `|>` and `filter`/`map`.
        let products: Vec<Product> = odata
            .value
            .into_iter()
            .filter(|p| p.online)
            .map(|p| p.into_product())
            .collect();

        if products.is_empty() {
            return Err(CopernicusError::NoResults);
        }

        Ok(products)
    }
}

/// Look up the unique CDSE identifier for a scene by its name.
///
/// This is used by `download_scene` to resolve a human-readable scene name
/// (like "S2B_MSIL2A_20200804T183919_...") into the UUID needed for download.
pub fn get_scene_id(scene_name: &str) -> Result<String> {
    let mut filters: Vec<String> = Vec::new();

    // Extract sensing date from the scene name (format: YYYYMMDDTHHMMSS)
    if let Some(date_match) = extract_sensing_date(scene_name) {
        let sense_date = chrono::NaiveDate::parse_from_str(&date_match, "%Y%m%d").map_err(|e| {
            CopernicusError::InvalidArgument(format!("failed to parse date from scene name: {e}"))
        })?;
        let start = sense_date
            .pred_opt()
            .unwrap_or(sense_date)
            .and_hms_milli_opt(0, 0, 0, 0)
            .unwrap();
        let end = sense_date
            .succ_opt()
            .unwrap_or(sense_date)
            .and_hms_milli_opt(0, 0, 0, 0)
            .unwrap();
        let start_str = start.format("%Y-%m-%dT%H:%M:%S%.3fZ");
        let end_str = end.format("%Y-%m-%dT%H:%M:%S%.3fZ");
        filters.push(format!(
            "ContentDate/Start gt {start_str} and ContentDate/Start lt {end_str}"
        ));
    }

    filters.push(format!("contains(Name,'{scene_name}')"));

    let filter = filters.join(" and ");
    let client = reqwest::blocking::Client::new();
    let response = client
        .get(CATALOGUE_URL)
        .query(&[("$filter", filter.as_str()), ("$expand", "Attributes")])
        .send()?;

    let body: serde_json::Value = response.json()?;
    let values = body["value"]
        .as_array()
        .ok_or_else(|| CopernicusError::SearchFailed("unexpected response format".into()))?;

    if values.is_empty() {
        return Err(CopernicusError::NoResults);
    }

    values[0]["Id"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| CopernicusError::SearchFailed("product has no Id field".into()))
}

/// Extract the 8-digit sensing date from a Sentinel scene name.
/// E.g. "S2B_MSIL2A_20200804T183919_..." -> "20200804"
fn extract_sensing_date(scene_name: &str) -> Option<String> {
    scene_name
        .split('_')
        .find(|part| part.len() >= 15 && part.chars().take(8).all(|c| c.is_ascii_digit()))
        .map(|part| part[..8].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_date_from_s2_name() {
        let name = "S2B_MSIL2A_20200804T183919_N0500_R070_T11UPT_20230321T050221";
        assert_eq!(extract_sensing_date(name), Some("20200804".to_string()));
    }

    #[test]
    fn extract_date_from_s1_name() {
        let name = "S1A_IW_GRDH_1SDV_20141031T161924_20141031T161949_003076_003856_634E";
        assert_eq!(extract_sensing_date(name), Some("20141031".to_string()));
    }

    #[test]
    fn build_filter_basic() {
        let query = SearchQuery::new(Satellite::Sentinel2);
        let filter = query.build_filter().unwrap();
        assert_eq!(filter, "Collection/Name eq 'SENTINEL-2'");
    }

    #[test]
    fn build_filter_with_product() {
        let query = SearchQuery::new(Satellite::Sentinel2).product("L2A");
        let filter = query.build_filter().unwrap();
        assert!(filter.contains("contains(Name,'L2A')"));
    }

    #[test]
    fn build_filter_cloud_on_sentinel1_fails() {
        let query = SearchQuery::new(Satellite::Sentinel1).max_cloud_cover(20.0);
        assert!(query.build_filter().is_err());
    }

    #[test]
    fn build_filter_tile_on_sentinel1_fails() {
        let query = SearchQuery::new(Satellite::Sentinel1).tile("T31TFJ");
        assert!(query.build_filter().is_err());
    }

    #[test]
    fn build_filter_valid_product_s2() {
        let query = SearchQuery::new(Satellite::Sentinel2).product("L2A");
        assert!(query.build_filter().is_ok());
    }

    #[test]
    fn build_filter_invalid_product_s2() {
        let query = SearchQuery::new(Satellite::Sentinel2).product("GRD");
        let err = query.build_filter().unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("unknown product type"));
        assert!(msg.contains("L1C, L2A"));
    }

    #[test]
    fn build_filter_valid_product_s3() {
        let query = SearchQuery::new(Satellite::Sentinel3).product("SL_2_LST");
        assert!(query.build_filter().is_ok());
    }

    #[test]
    fn build_filter_invalid_product_s3() {
        let query = SearchQuery::new(Satellite::Sentinel3).product("L2A");
        assert!(query.build_filter().is_err());
    }

    #[test]
    fn build_filter_valid_product_s1() {
        for p in &["GRD", "SLC", "OCN", "RAW"] {
            let query = SearchQuery::new(Satellite::Sentinel1).product(*p);
            assert!(query.build_filter().is_ok(), "expected {p} to be valid for S1");
        }
    }

    #[test]
    fn is_valid_product_case_insensitive() {
        assert!(Satellite::Sentinel2.is_valid_product("l2a"));
        assert!(Satellite::Sentinel2.is_valid_product("L2A"));
        assert!(Satellite::Sentinel1.is_valid_product("grd"));
    }

    #[test]
    fn build_filter_tile_strips_t_prefix() {
        let query = SearchQuery::new(Satellite::Sentinel2).tile("T31TFJ");
        let filter = query.build_filter().unwrap();
        assert!(filter.contains("Value eq '31TFJ'"), "should strip the T prefix: {filter}");
        assert!(!filter.contains("Value eq 'T31TFJ'"), "should not keep the T prefix");
    }

    #[test]
    fn build_filter_tile_without_t_prefix() {
        let query = SearchQuery::new(Satellite::Sentinel2).tile("31TFJ");
        let filter = query.build_filter().unwrap();
        assert!(filter.contains("Value eq '31TFJ'"));
    }
}
