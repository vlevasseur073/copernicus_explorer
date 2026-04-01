use std::fmt;
use std::path::Path;

use crate::error::{CopernicusError, Result};

/// A geographic point in latitude/longitude (EPSG:4326).
///
/// `f64` is Rust's 64-bit floating point -- equivalent to Julia's `Float64`.
/// `Clone, Copy` mean this tiny struct can be duplicated cheaply (it's just
/// two numbers on the stack).
#[derive(Debug, Clone, Copy)]
pub struct Point {
    pub lat: f64,
    pub lon: f64,
}

impl Point {
    pub fn new(lat: f64, lon: f64) -> Self {
        Self { lat, lon }
    }
}

/// A bounding box defined by its upper-left and lower-right corners.
///
/// Each corner is a `(lat, lon)` tuple.  Rust tuples are like Julia tuples
/// but with fixed types: `(f64, f64)`.
#[derive(Debug, Clone, Copy)]
pub struct BoundingBox {
    pub upper_left: (f64, f64),
    pub lower_right: (f64, f64),
}

impl BoundingBox {
    pub fn new(upper_left: (f64, f64), lower_right: (f64, f64)) -> Self {
        Self {
            upper_left,
            lower_right,
        }
    }
}

/// A polygon defined by an exterior ring of `(lon, lat)` coordinates.
///
/// The ring must be closed (first point == last point) and contain at
/// least 4 coordinate pairs.  This is the representation used by the
/// CDSE OData spatial filter.
///
/// Coordinates are stored in **(longitude, latitude)** order to match
/// both GeoJSON and WKT conventions.
#[derive(Debug, Clone)]
pub struct Polygon {
    pub exterior: Vec<(f64, f64)>,
}

impl Polygon {
    /// Create a polygon from an exterior ring of `(lon, lat)` pairs.
    ///
    /// The ring is automatically closed if the last point does not
    /// already equal the first.  Returns an error if fewer than 3
    /// distinct points are provided.
    pub fn new(mut coords: Vec<(f64, f64)>) -> Result<Self> {
        if coords.len() < 3 {
            return Err(CopernicusError::InvalidArgument(
                "a polygon requires at least 3 coordinate pairs".into(),
            ));
        }
        if coords.first() != coords.last() {
            coords.push(coords[0]);
        }
        Ok(Self { exterior: coords })
    }
}

/// A geometry that can be used as a spatial filter in CDSE searches.
///
/// This is a Rust enum with **data inside each variant** -- very different
/// from C-style enums.  Each variant holds a different geometry type.
/// `match` forces us to handle every variant, so we can never forget one.
#[derive(Debug, Clone)]
pub enum Geometry {
    Point(Point),
    BoundingBox(BoundingBox),
    Polygon(Polygon),
}

impl Geometry {
    /// Convert to Well-Known Text (WKT) for the OData spatial filter.
    ///
    /// The CDSE API expects WKT in **longitude-latitude** order (note: the
    /// opposite of how we store Point/BoundingBox!), wrapped in an OData
    /// geography literal:
    ///   `OData.CSC.Intersects(area=geography'SRID=4326;POINT(lon lat)')`
    ///
    /// The `match` expression is like Julia's pattern matching but exhaustive:
    /// the compiler will error if we add a new `Geometry` variant and forget
    /// to handle it here.
    pub fn to_wkt(&self) -> String {
        match self {
            Geometry::Point(p) => {
                format!("POINT({lon} {lat})", lon = p.lon, lat = p.lat)
            }
            Geometry::BoundingBox(bb) => {
                let (lat_top, lon_left) = bb.upper_left;
                let (lat_bottom, lon_right) = bb.lower_right;

                // A polygon ring must close (first point == last point).
                // OData expects lon/lat order.
                let coords = [
                    (lon_left, lat_top),
                    (lon_right, lat_top),
                    (lon_right, lat_bottom),
                    (lon_left, lat_bottom),
                    (lon_left, lat_top), // close the ring
                ];

                let ring: String = coords
                    .iter()
                    .map(|(lon, lat)| format!("{lon} {lat}"))
                    .collect::<Vec<_>>()
                    .join(",");

                format!("POLYGON(({ring}))")
            }
            Geometry::Polygon(poly) => {
                let ring: String = poly
                    .exterior
                    .iter()
                    .map(|(lon, lat)| format!("{lon} {lat}"))
                    .collect::<Vec<_>>()
                    .join(",");

                format!("POLYGON(({ring}))")
            }
        }
    }

    /// Load a geometry from a GeoJSON file.
    ///
    /// Supports GeoJSON objects of type `Point`, `Polygon`, `Feature`, and
    /// `FeatureCollection`.  For `Feature` and `FeatureCollection`, the
    /// geometry is extracted from the first feature's `"geometry"` field.
    ///
    /// Only the first geometry is used; multi-geometries and holes
    /// (interior rings) are not supported.
    pub fn from_geojson_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            CopernicusError::InvalidArgument(format!(
                "cannot read GeoJSON file '{}': {e}",
                path.display()
            ))
        })?;
        Self::from_geojson(&content)
    }

    /// Parse a geometry from a GeoJSON string.
    ///
    /// See [`from_geojson_file`](Self::from_geojson_file) for supported types.
    pub fn from_geojson(geojson: &str) -> Result<Self> {
        let value: serde_json::Value = serde_json::from_str(geojson)
            .map_err(|e| CopernicusError::InvalidArgument(format!("invalid GeoJSON: {e}")))?;
        parse_geojson_value(&value)
    }
}

/// Recursively extract a `Geometry` from a parsed GeoJSON value.
fn parse_geojson_value(value: &serde_json::Value) -> Result<Geometry> {
    let obj = value
        .as_object()
        .ok_or_else(|| CopernicusError::InvalidArgument("GeoJSON must be a JSON object".into()))?;

    let geo_type = obj.get("type").and_then(|v| v.as_str()).ok_or_else(|| {
        CopernicusError::InvalidArgument("GeoJSON object has no \"type\" field".into())
    })?;

    match geo_type {
        "Point" => {
            let coords = obj
                .get("coordinates")
                .and_then(|v| v.as_array())
                .ok_or_else(|| {
                    CopernicusError::InvalidArgument(
                        "GeoJSON Point missing \"coordinates\" array".into(),
                    )
                })?;
            if coords.len() < 2 {
                return Err(CopernicusError::InvalidArgument(
                    "GeoJSON Point requires at least [lon, lat]".into(),
                ));
            }
            let lon = coords[0].as_f64().ok_or_else(|| {
                CopernicusError::InvalidArgument("GeoJSON coordinate is not a number".into())
            })?;
            let lat = coords[1].as_f64().ok_or_else(|| {
                CopernicusError::InvalidArgument("GeoJSON coordinate is not a number".into())
            })?;
            Ok(Geometry::Point(Point::new(lat, lon)))
        }
        "Polygon" => {
            let rings = obj
                .get("coordinates")
                .and_then(|v| v.as_array())
                .ok_or_else(|| {
                    CopernicusError::InvalidArgument(
                        "GeoJSON Polygon missing \"coordinates\" array".into(),
                    )
                })?;
            if rings.is_empty() {
                return Err(CopernicusError::InvalidArgument(
                    "GeoJSON Polygon has no rings".into(),
                ));
            }
            let exterior = parse_ring(&rings[0])?;
            Ok(Geometry::Polygon(Polygon::new(exterior)?))
        }
        "Feature" => {
            let geom = obj.get("geometry").ok_or_else(|| {
                CopernicusError::InvalidArgument("GeoJSON Feature has no \"geometry\" field".into())
            })?;
            parse_geojson_value(geom)
        }
        "FeatureCollection" => {
            let features = obj
                .get("features")
                .and_then(|v| v.as_array())
                .ok_or_else(|| {
                    CopernicusError::InvalidArgument(
                        "GeoJSON FeatureCollection has no \"features\" array".into(),
                    )
                })?;
            if features.is_empty() {
                return Err(CopernicusError::InvalidArgument(
                    "GeoJSON FeatureCollection is empty".into(),
                ));
            }
            parse_geojson_value(&features[0])
        }
        other => Err(CopernicusError::InvalidArgument(format!(
            "unsupported GeoJSON type \"{other}\"; expected Point, Polygon, Feature, \
             or FeatureCollection"
        ))),
    }
}

/// Parse a GeoJSON coordinate ring `[[lon, lat], ...]` into `Vec<(lon, lat)>`.
fn parse_ring(ring: &serde_json::Value) -> Result<Vec<(f64, f64)>> {
    let arr = ring
        .as_array()
        .ok_or_else(|| CopernicusError::InvalidArgument("GeoJSON ring is not an array".into()))?;
    arr.iter()
        .map(|coord| {
            let pair = coord.as_array().ok_or_else(|| {
                CopernicusError::InvalidArgument("GeoJSON coordinate is not an array".into())
            })?;
            if pair.len() < 2 {
                return Err(CopernicusError::InvalidArgument(
                    "GeoJSON coordinate requires at least [lon, lat]".into(),
                ));
            }
            let lon = pair[0].as_f64().ok_or_else(|| {
                CopernicusError::InvalidArgument("GeoJSON coordinate is not a number".into())
            })?;
            let lat = pair[1].as_f64().ok_or_else(|| {
                CopernicusError::InvalidArgument("GeoJSON coordinate is not a number".into())
            })?;
            Ok((lon, lat))
        })
        .collect()
}

impl fmt::Display for Geometry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_wkt())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_wkt() {
        let p = Geometry::Point(Point::new(43.6, 1.44));
        assert_eq!(p.to_wkt(), "POINT(1.44 43.6)");
    }

    #[test]
    fn bbox_wkt() {
        let bb = Geometry::BoundingBox(BoundingBox::new((52.1, -114.4), (51.9, -114.1)));
        let wkt = bb.to_wkt();
        assert!(wkt.starts_with("POLYGON(("));
        assert!(wkt.ends_with("))"));
        // First and last coordinate should be the same (closed ring)
        let inner = &wkt["POLYGON((".len()..wkt.len() - "))".len()];
        let coords: Vec<&str> = inner.split(',').collect();
        assert_eq!(coords.first(), coords.last());
    }

    #[test]
    fn polygon_wkt() {
        let poly = Polygon::new(vec![(1.0, 43.0), (2.0, 43.0), (2.0, 44.0), (1.0, 44.0)]).unwrap();
        let geom = Geometry::Polygon(poly);
        let wkt = geom.to_wkt();
        assert!(wkt.starts_with("POLYGON(("));
        assert!(wkt.ends_with("))"));
        let inner = &wkt["POLYGON((".len()..wkt.len() - "))".len()];
        let coords: Vec<&str> = inner.split(',').collect();
        assert_eq!(coords.first(), coords.last(), "ring must be closed");
    }

    #[test]
    fn polygon_auto_closes() {
        let poly = Polygon::new(vec![(1.0, 43.0), (2.0, 43.0), (2.0, 44.0)]).unwrap();
        assert_eq!(poly.exterior.len(), 4);
        assert_eq!(poly.exterior.first(), poly.exterior.last());
    }

    #[test]
    fn polygon_too_few_points() {
        assert!(Polygon::new(vec![(1.0, 43.0), (2.0, 43.0)]).is_err());
    }

    #[test]
    fn geojson_point() {
        let json = r#"{"type": "Point", "coordinates": [1.44, 43.6]}"#;
        let geom = Geometry::from_geojson(json).unwrap();
        assert_eq!(geom.to_wkt(), "POINT(1.44 43.6)");
    }

    #[test]
    fn geojson_polygon() {
        let json = r#"{
            "type": "Polygon",
            "coordinates": [[[1.0, 43.0], [2.0, 43.0], [2.0, 44.0], [1.0, 44.0], [1.0, 43.0]]]
        }"#;
        let geom = Geometry::from_geojson(json).unwrap();
        let wkt = geom.to_wkt();
        assert!(wkt.starts_with("POLYGON(("));
        assert!(wkt.contains("1 43"));
    }

    #[test]
    fn geojson_feature() {
        let json = r#"{
            "type": "Feature",
            "properties": {},
            "geometry": {
                "type": "Point",
                "coordinates": [1.44, 43.6]
            }
        }"#;
        let geom = Geometry::from_geojson(json).unwrap();
        assert_eq!(geom.to_wkt(), "POINT(1.44 43.6)");
    }

    #[test]
    fn geojson_feature_collection() {
        let json = r#"{
            "type": "FeatureCollection",
            "features": [{
                "type": "Feature",
                "properties": {},
                "geometry": {
                    "type": "Polygon",
                    "coordinates": [[[1.0, 43.0], [2.0, 43.0], [2.0, 44.0], [1.0, 44.0], [1.0, 43.0]]]
                }
            }]
        }"#;
        let geom = Geometry::from_geojson(json).unwrap();
        assert!(matches!(geom, Geometry::Polygon(_)));
    }

    #[test]
    fn geojson_unsupported_type() {
        let json = r#"{"type": "MultiPoint", "coordinates": [[1.0, 2.0]]}"#;
        assert!(Geometry::from_geojson(json).is_err());
    }

    #[test]
    fn geojson_invalid_json() {
        assert!(Geometry::from_geojson("not json").is_err());
    }
}
