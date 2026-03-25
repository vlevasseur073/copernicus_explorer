//! Python bindings for the `copernicus_explorer` Rust library.
//!
//! This crate uses [PyO3](https://pyo3.rs) to expose the core library's
//! types and functions to Python.  The `#[pymodule]` macro generates the
//! C-level init function that Python calls when you `import copernicus_explorer_py`.
//!
//! # How PyO3 works (didactic notes)
//!
//! - `#[pyclass]` turns a Rust struct into a Python class.
//! - `#[pymethods]` adds methods visible from Python.
//! - `#[pyfunction]` turns a free Rust function into a Python function.
//! - `#[pymodule]` defines the top-level module and registers everything.
//!
//! PyO3 handles reference counting, GIL management, and type conversions
//! automatically.  We just write idiomatic Rust and annotate with macros.
//!
//! # Blocking wrappers
//!
//! The core Rust library is async-first.  Since Python's GIL makes async
//! impractical here, we use the `blocking` module which creates a Tokio
//! runtime internally for each call.

use pyo3::exceptions::{PyRuntimeError, PyTypeError};
use pyo3::prelude::*;
use pyo3::types::PyDateTime;

// ---------------------------------------------------------------------------
// Error mapping
// ---------------------------------------------------------------------------

/// Convert a `CopernicusError` into a `PyErr`.
///
/// We can't implement `From<CopernicusError> for PyErr` here because of
/// Rust's **orphan rule**: you can only implement a trait for a type if
/// either the trait or the type is defined in your crate.  Both `From`
/// and `PyErr` are foreign, and so is `CopernicusError`.
///
/// Instead we use a simple helper function.  The `?` operator won't
/// auto-convert, so we call this explicitly via `.map_err(to_pyerr)?`.
fn to_pyerr(err: copernicus_explorer::CopernicusError) -> PyErr {
    PyRuntimeError::new_err(err.to_string())
}

// ---------------------------------------------------------------------------
// Satellite enum wrapper
// ---------------------------------------------------------------------------

/// Python wrapper for the `Satellite` enum.
///
/// Python doesn't have Rust-style enums, so we store the Rust enum inside
/// a struct and expose it as a class with classmethods for each variant.
/// This is the standard PyO3 pattern for wrapping enums.
#[pyclass(name = "Satellite")]
#[derive(Clone)]
struct PySatellite {
    inner: copernicus_explorer::Satellite,
}

#[pymethods]
impl PySatellite {
    #[staticmethod]
    fn sentinel1() -> Self {
        Self {
            inner: copernicus_explorer::Satellite::Sentinel1,
        }
    }

    #[staticmethod]
    fn sentinel2() -> Self {
        Self {
            inner: copernicus_explorer::Satellite::Sentinel2,
        }
    }

    #[staticmethod]
    fn sentinel3() -> Self {
        Self {
            inner: copernicus_explorer::Satellite::Sentinel3,
        }
    }

    #[staticmethod]
    fn sentinel5p() -> Self {
        Self {
            inner: copernicus_explorer::Satellite::Sentinel5P,
        }
    }

    #[staticmethod]
    fn sentinel6() -> Self {
        Self {
            inner: copernicus_explorer::Satellite::Sentinel6,
        }
    }

    /// Return the CDSE collection name (e.g. "SENTINEL-2").
    fn collection_name(&self) -> &'static str {
        self.inner.collection_name()
    }

    /// Return the list of known product types for this satellite.
    fn known_products(&self) -> Vec<&'static str> {
        self.inner.known_products().to_vec()
    }

    /// Check whether a product type string is valid for this satellite.
    fn is_valid_product(&self, product: &str) -> bool {
        self.inner.is_valid_product(product)
    }

    fn __repr__(&self) -> String {
        format!("Satellite({})", self.inner.collection_name())
    }

    fn __str__(&self) -> String {
        self.inner.collection_name().to_string()
    }
}

// ---------------------------------------------------------------------------
// Product wrapper
// ---------------------------------------------------------------------------

/// Python wrapper for a search result product.
///
/// We store plain Python-friendly types (strings, floats) rather than
/// wrapping the Rust struct directly, because `Product` contains types
/// that don't implement `Clone` (required by `#[pyclass]`).
#[pyclass(name = "Product")]
#[derive(Clone)]
struct PyProduct {
    #[pyo3(get)]
    name: String,
    #[pyo3(get)]
    id: String,
    #[pyo3(get)]
    acquisition_date: String,
    #[pyo3(get)]
    publication_date: String,
    #[pyo3(get)]
    online: bool,
    #[pyo3(get)]
    cloud_cover: Option<f64>,
}

impl From<copernicus_explorer::Product> for PyProduct {
    fn from(p: copernicus_explorer::Product) -> Self {
        Self {
            name: p.name,
            id: p.id,
            acquisition_date: p.acquisition_date,
            publication_date: p.publication_date,
            online: p.online,
            cloud_cover: p.cloud_cover,
        }
    }
}

impl From<&PyProduct> for copernicus_explorer::Product {
    fn from(p: &PyProduct) -> Self {
        Self {
            name: p.name.clone(),
            id: p.id.clone(),
            acquisition_date: p.acquisition_date.clone(),
            publication_date: p.publication_date.clone(),
            online: p.online,
            cloud_cover: p.cloud_cover,
        }
    }
}

#[pymethods]
impl PyProduct {
    #[new]
    #[pyo3(signature = (name, id, acquisition_date, publication_date, online, cloud_cover=None))]
    fn new(
        name: String,
        id: String,
        acquisition_date: String,
        publication_date: String,
        online: bool,
        cloud_cover: Option<f64>,
    ) -> Self {
        Self {
            name,
            id,
            acquisition_date,
            publication_date,
            online,
            cloud_cover,
        }
    }

    fn __repr__(&self) -> String {
        let cloud = match self.cloud_cover {
            Some(c) => format!("{c:.1}%"),
            None => "N/A".to_string(),
        };
        format!(
            "Product({name}, acquired={date}, cloud={cloud})",
            name = self.name,
            date = self.acquisition_date,
        )
    }

    fn __str__(&self) -> String {
        self.__repr__()
    }
}

// ---------------------------------------------------------------------------
// Geometry wrappers
// ---------------------------------------------------------------------------

/// A geographic point (latitude, longitude).
#[pyclass(name = "Point")]
#[derive(Clone)]
struct PyPoint {
    #[pyo3(get)]
    lat: f64,
    #[pyo3(get)]
    lon: f64,
}

#[pymethods]
impl PyPoint {
    #[new]
    fn new(lat: f64, lon: f64) -> Self {
        Self { lat, lon }
    }

    fn __repr__(&self) -> String {
        format!("Point(lat={}, lon={})", self.lat, self.lon)
    }
}

/// A bounding box defined by upper-left (lat, lon) and lower-right (lat, lon).
#[pyclass(name = "BoundingBox")]
#[derive(Clone)]
struct PyBoundingBox {
    #[pyo3(get)]
    upper_left: (f64, f64),
    #[pyo3(get)]
    lower_right: (f64, f64),
}

#[pymethods]
impl PyBoundingBox {
    #[new]
    fn new(upper_left: (f64, f64), lower_right: (f64, f64)) -> Self {
        Self {
            upper_left,
            lower_right,
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "BoundingBox(upper_left={:?}, lower_right={:?})",
            self.upper_left, self.lower_right,
        )
    }
}

// ---------------------------------------------------------------------------
// SearchQuery wrapper
// ---------------------------------------------------------------------------

/// Builder for CDSE catalogue search queries.
///
/// Each setter mutates `self` in place and returns `self` so you can
/// chain calls from Python:
///
/// ```python
/// q = SearchQuery(Satellite.sentinel2())
/// q.product("L2A").max_cloud_cover(20.0).max_results(5)
/// results = q.execute()
/// ```
///
/// # Why `&mut self` instead of consuming `self`?
///
/// In the Rust core library, the builder uses `self` (by value) for
/// zero-cost moves.  But PyO3 classes live on the Python heap behind
/// reference counts -- you can't move them.  So we use `&mut self`
/// which borrows the object in place, and return `Self` by cloning
/// to allow Python-style chaining.
#[pyclass(name = "SearchQuery")]
#[derive(Clone)]
struct PySearchQuery {
    satellite: copernicus_explorer::Satellite,
    product: Option<String>,
    start_date: Option<String>,
    end_date: Option<String>,
    tile: Option<String>,
    max_cloud_cover: Option<f64>,
    point: Option<(f64, f64)>,
    bbox: Option<((f64, f64), (f64, f64))>,
    max_results: u32,
}

#[pymethods]
impl PySearchQuery {
    #[new]
    fn new(satellite: &PySatellite) -> Self {
        Self {
            satellite: satellite.inner,
            product: None,
            start_date: None,
            end_date: None,
            tile: None,
            max_cloud_cover: None,
            point: None,
            bbox: None,
            max_results: 100,
        }
    }

    /// Set the product type filter (e.g. "L2A", "GRD").
    fn product(&mut self, product: &str) -> Self {
        self.product = Some(product.to_string());
        self.clone()
    }

    /// Set the date range filter.
    ///
    /// Accepts `datetime.datetime` objects or ISO-8601 strings
    /// (e.g. "2024-01-01" or "2024-01-01T00:00:00Z").
    fn dates(&mut self, start: &Bound<'_, PyAny>, end: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.start_date = Some(pyany_to_iso(start)?);
        self.end_date = Some(pyany_to_iso(end)?);
        Ok(self.clone())
    }

    /// Set the Sentinel-2 tile filter (e.g. "T31TFJ").
    fn tile(&mut self, tile: &str) -> Self {
        self.tile = Some(tile.to_string());
        self.clone()
    }

    /// Set the maximum cloud cover percentage (0-100).
    fn max_cloud_cover(&mut self, percent: f64) -> Self {
        self.max_cloud_cover = Some(percent);
        self.clone()
    }

    /// Set a point geometry filter.
    fn geometry_point(&mut self, point: &PyPoint) -> Self {
        self.point = Some((point.lat, point.lon));
        self.bbox = None;
        self.clone()
    }

    /// Set a bounding box geometry filter.
    fn geometry_bbox(&mut self, bbox: &PyBoundingBox) -> Self {
        self.bbox = Some((bbox.upper_left, bbox.lower_right));
        self.point = None;
        self.clone()
    }

    /// Set the maximum number of results (default: 100).
    fn max_results(&mut self, n: u32) -> Self {
        self.max_results = n;
        self.clone()
    }

    /// Execute the search and return a list of `Product` objects.
    fn execute(&self) -> PyResult<Vec<PyProduct>> {
        let mut query = copernicus_explorer::SearchQuery::new(self.satellite);

        if let Some(ref p) = self.product {
            query = query.product(p.clone());
        }

        if let Some(ref start) = self.start_date {
            let end = self
                .end_date
                .as_deref()
                .ok_or_else(|| PyRuntimeError::new_err("start date set but end date missing"))?;
            let start_dt = parse_datetime(start)?;
            let end_dt = parse_datetime(end)?;
            query = query.dates(start_dt, end_dt);
        }

        if let Some(ref t) = self.tile {
            query = query.tile(t.clone());
        }

        if let Some(cc) = self.max_cloud_cover {
            query = query.max_cloud_cover(cc);
        }

        if let Some((lat, lon)) = self.point {
            let geom =
                copernicus_explorer::Geometry::Point(copernicus_explorer::Point::new(lat, lon));
            query = query.geometry(geom);
        }

        if let Some((ul, lr)) = self.bbox {
            let geom = copernicus_explorer::Geometry::BoundingBox(
                copernicus_explorer::BoundingBox::new(ul, lr),
            );
            query = query.geometry(geom);
        }

        query = query.max_results(self.max_results);

        let products = query.execute_blocking().map_err(to_pyerr)?;
        Ok(products.into_iter().map(PyProduct::from).collect())
    }

    fn __repr__(&self) -> String {
        format!(
            "SearchQuery(satellite={}, product={:?}, max_results={})",
            self.satellite.collection_name(),
            self.product,
            self.max_results,
        )
    }
}

// ---------------------------------------------------------------------------
// Free functions
// ---------------------------------------------------------------------------

/// Authenticate with the CDSE identity provider and return an access token.
#[pyfunction]
fn get_access_token(username: &str, password: &str) -> PyResult<String> {
    copernicus_explorer::blocking::get_access_token(username, password).map_err(to_pyerr)
}

/// Authenticate using COPERNICUS_USER / COPERNICUS_PASS environment variables.
#[pyfunction]
fn get_access_token_from_env() -> PyResult<String> {
    copernicus_explorer::blocking::get_access_token_from_env().map_err(to_pyerr)
}

/// Download a Sentinel scene to a local directory.
///
/// Returns the path to the downloaded file.
#[pyfunction]
fn download_scene(scene_name: &str, directory: &str, access_token: &str) -> PyResult<String> {
    let dir = std::path::Path::new(directory);
    let path = copernicus_explorer::blocking::download_scene(scene_name, dir, access_token)
        .map_err(to_pyerr)?;
    Ok(path.to_string_lossy().into_owned())
}

/// Download multiple products concurrently.
///
/// Returns a list of results: each element is either the path to the
/// downloaded file (str) or None if that download failed.  Errors are
/// printed to stderr.
///
/// # Arguments
///
/// * `products` - List of `Product` objects (from `SearchQuery.execute()`)
/// * `directory` - Directory to save downloaded files
/// * `access_token` - A valid CDSE access token
/// * `max_concurrent` - Maximum simultaneous downloads (default: 4)
#[pyfunction]
#[pyo3(signature = (products, directory, access_token, max_concurrent=4))]
fn download_products(
    products: Vec<PyProduct>,
    directory: &str,
    access_token: &str,
    max_concurrent: usize,
) -> PyResult<Vec<Option<String>>> {
    let core_products: Vec<copernicus_explorer::Product> =
        products.iter().map(Into::into).collect();
    let dir = std::path::Path::new(directory);

    let results = copernicus_explorer::blocking::download_products(
        &core_products,
        dir,
        access_token,
        max_concurrent,
    );

    Ok(results
        .into_iter()
        .enumerate()
        .map(|(i, r)| match r {
            Ok(path) => Some(path.to_string_lossy().into_owned()),
            Err(e) => {
                eprintln!("  download failed for {}: {e}", products[i].name);
                None
            }
        })
        .collect())
}

/// Look up the CDSE UUID for a scene by its name.
#[pyfunction]
fn get_scene_id(scene_name: &str) -> PyResult<String> {
    copernicus_explorer::blocking::get_scene_id(scene_name).map_err(to_pyerr)
}

/// Format a list of products as an aligned table (same layout as the CLI).
///
/// Returns the table as a string.  Call `print(format_products(results))`
/// or just use `print_products(results)` for direct output.
#[pyfunction]
fn format_products(products: Vec<PyProduct>) -> String {
    let core_products: Vec<copernicus_explorer::Product> =
        products.iter().map(Into::into).collect();
    copernicus_explorer::Products(&core_products).to_string()
}

/// Print a list of products as an aligned table to stdout.
#[pyfunction]
fn print_products(products: Vec<PyProduct>) {
    let core_products: Vec<copernicus_explorer::Product> =
        products.iter().map(Into::into).collect();
    println!("{}", copernicus_explorer::Products(&core_products));
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract an ISO-8601 string from a Python `datetime` or `str`.
fn pyany_to_iso(obj: &Bound<'_, PyAny>) -> PyResult<String> {
    if obj.is_instance_of::<pyo3::types::PyString>() {
        return obj.extract::<String>();
    }
    if obj.downcast::<PyDateTime>().is_ok() {
        let iso: String = obj.call_method0("isoformat")?.extract()?;
        return Ok(iso);
    }
    Err(PyTypeError::new_err(
        "expected a datetime.datetime or an ISO-8601 string",
    ))
}

fn parse_datetime(s: &str) -> PyResult<chrono::DateTime<chrono::Utc>> {
    use chrono::{NaiveDate, NaiveDateTime, TimeZone, Utc};

    if let Ok(dt) = s.parse::<chrono::DateTime<Utc>>() {
        return Ok(dt);
    }

    if let Ok(ndt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return Ok(Utc.from_utc_datetime(&ndt));
    }

    if let Ok(nd) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        let ndt = nd.and_hms_opt(0, 0, 0).unwrap();
        return Ok(Utc.from_utc_datetime(&ndt));
    }

    Err(PyRuntimeError::new_err(format!(
        "cannot parse date '{s}'. Expected ISO-8601 format like \
         '2024-01-15' or '2024-01-15T12:00:00Z'"
    )))
}

// ---------------------------------------------------------------------------
// Module definition
// ---------------------------------------------------------------------------

/// The Python module entry point.
///
/// `#[pymodule]` generates the C `PyInit_copernicus_explorer_py` function
/// that Python's import machinery calls.  Inside, we register every class
/// and function that should be visible from Python.
#[pymodule]
fn copernicus_explorer_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PySatellite>()?;
    m.add_class::<PyProduct>()?;
    m.add_class::<PyPoint>()?;
    m.add_class::<PyBoundingBox>()?;
    m.add_class::<PySearchQuery>()?;
    m.add_function(wrap_pyfunction!(get_access_token, m)?)?;
    m.add_function(wrap_pyfunction!(get_access_token_from_env, m)?)?;
    m.add_function(wrap_pyfunction!(download_scene, m)?)?;
    m.add_function(wrap_pyfunction!(download_products, m)?)?;
    m.add_function(wrap_pyfunction!(get_scene_id, m)?)?;
    m.add_function(wrap_pyfunction!(format_products, m)?)?;
    m.add_function(wrap_pyfunction!(print_products, m)?)?;
    Ok(())
}
