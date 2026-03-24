use std::fmt;

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

/// A geometry that can be used as a spatial filter in CDSE searches.
///
/// This is a Rust enum with **data inside each variant** -- very different
/// from C-style enums.  Each variant holds a different geometry type.
/// `match` forces us to handle every variant, so we can never forget one.
#[derive(Debug, Clone, Copy)]
pub enum Geometry {
    Point(Point),
    BoundingBox(BoundingBox),
}

impl Geometry {
    /// Convert to Well-Known Text (WKT) for the OData spatial filter.
    ///
    /// The CDSE API expects WKT in **longitude-latitude** order (note: the
    /// opposite of how we store them!), wrapped in an OData geography literal:
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
        }
    }
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
}
