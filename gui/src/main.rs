use chrono::{Duration, Utc};
use copernicus_explorer::{Product, Satellite, SearchQuery};
use eframe::egui;
use std::sync::Arc;
use tokio::runtime::Runtime;

struct CopernicusExplorerApp {
    satellite: Satellite,
    previous_satellite: Satellite,
    product: String,
    start_date: String,
    end_date: String,
    tile: String,
    cloud_cover: f64,
    point: String,
    bbox: String,
    geojson: String,
    max_results: u32,
    products: Vec<Product>,
    runtime: Arc<Runtime>,
}

impl Default for CopernicusExplorerApp {
    fn default() -> Self {
        let end_date = Utc::now().date_naive();
        let start_date = end_date - Duration::days(7);

        Self {
            satellite: Satellite::Sentinel2,
            previous_satellite: Satellite::Sentinel2,
            product: Satellite::Sentinel2.known_products()[0].to_string(),
            start_date: start_date.format("%Y-%m-%d").to_string(),
            end_date: end_date.format("%Y-%m-%d").to_string(),
            tile: String::new(),
            cloud_cover: 15.0,
            point: String::new(),
            bbox: String::new(),
            geojson: String::new(),
            max_results: 10,
            products: Vec::new(),
            runtime: Arc::new(Runtime::new().expect("Failed to create Tokio runtime")),
        }
    }
}

impl CopernicusExplorerApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self::default()
    }

    fn search_products(&mut self) {
        let runtime = self.runtime.clone();
        let satellite = self.satellite;
        let product = self.product.clone();
        let start_date = self.start_date.clone();
        let end_date = self.end_date.clone();
        let tile = self.tile.clone();
        let cloud_cover = self.cloud_cover;
        let point = self.point.clone();
        let bbox = self.bbox.clone();
        let geojson = self.geojson.clone();
        let max_results = self.max_results;

        let products = runtime.block_on(async move {
            let mut query = SearchQuery::new(satellite);
            if !product.is_empty() {
                query = query.product(product);
            }
            if !start_date.is_empty() && !end_date.is_empty() {
                let start_dt = chrono::NaiveDate::parse_from_str(&start_date, "%Y-%m-%d")
                    .unwrap()
                    .and_hms_opt(0, 0, 0)
                    .unwrap()
                    .and_utc();
                let end_dt = chrono::NaiveDate::parse_from_str(&end_date, "%Y-%m-%d")
                    .unwrap()
                    .and_hms_opt(0, 0, 0)
                    .unwrap()
                    .and_utc();
                query = query.dates(start_dt, end_dt);
            }
            if !tile.is_empty() {
                query = query.tile(tile);
            }
            if cloud_cover > 0.0 {
                query = query.max_cloud_cover(cloud_cover);
            }
            if !point.is_empty() {
                let parts: Vec<f64> = point
                    .split(',')
                    .map(|s| s.trim().parse().unwrap())
                    .collect();
                let geometry = copernicus_explorer::Geometry::Point(
                    copernicus_explorer::Point::new(parts[0], parts[1]),
                );
                query = query.geometry(geometry);
            } else if !bbox.is_empty() {
                let parts: Vec<f64> = bbox.split(',').map(|s| s.trim().parse().unwrap()).collect();
                let geometry = copernicus_explorer::Geometry::BoundingBox(
                    copernicus_explorer::BoundingBox::new(
                        (parts[0], parts[1]),
                        (parts[2], parts[3]),
                    ),
                );
                query = query.geometry(geometry);
            } else if !geojson.is_empty() {
                let geometry = copernicus_explorer::Geometry::from_geojson_file(
                    std::path::Path::new(&geojson),
                )
                .unwrap();
                query = query.geometry(geometry);
            }
            query = query.max_results(max_results);
            query.execute().await.unwrap()
        });

        self.products = products;
    }

    fn download_product(&self, product: &Product) {
        let runtime = self.runtime.clone();
        let product_id = product.id.clone();
        runtime.spawn(async move {
            let token = copernicus_explorer::get_access_token_from_env()
                .await
                .unwrap();
            let dest = copernicus_explorer::OutputDestination::Local(".".into());
            let _ = copernicus_explorer::download_by_id_to(&product_id, &dest, &token).await;
        });
    }

    fn sync_product_with_satellite(&mut self) {
        if self.satellite != self.previous_satellite {
            self.previous_satellite = self.satellite;
            self.product = self.satellite.known_products()[0].to_string();
        } else if !self.satellite.is_valid_product(&self.product) {
            self.product = self.satellite.known_products()[0].to_string();
        }
    }
}

impl eframe::App for CopernicusExplorerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Copernicus Explorer");

            ui.horizontal(|ui| {
                ui.label("Satellite:");
                egui::ComboBox::from_id_source("satellite")
                    .selected_text(format!("{}", self.satellite))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.satellite,
                            Satellite::Sentinel2,
                            "Sentinel-2",
                        );
                        ui.selectable_value(
                            &mut self.satellite,
                            Satellite::Sentinel3,
                            "Sentinel-3",
                        );
                        ui.selectable_value(
                            &mut self.satellite,
                            Satellite::Sentinel5P,
                            "Sentinel-5P",
                        );
                    });
            });

            self.sync_product_with_satellite();

            ui.horizontal(|ui| {
                ui.label("Product:");
                egui::ComboBox::from_id_source("product")
                    .selected_text(if self.product.is_empty() {
                        "Select product…"
                    } else {
                        &self.product
                    })
                    .show_ui(ui, |ui| {
                        for product in self.satellite.known_products() {
                            ui.selectable_value(&mut self.product, product.to_string(), *product);
                        }
                    });
            });

            ui.horizontal(|ui| {
                ui.label("Start Date (YYYY-MM-DD):");
                ui.text_edit_singleline(&mut self.start_date);
            });

            ui.horizontal(|ui| {
                ui.label("End Date (YYYY-MM-DD):");
                ui.text_edit_singleline(&mut self.end_date);
            });

            ui.horizontal(|ui| {
                ui.label("Tile:");
                ui.text_edit_singleline(&mut self.tile);
            });

            ui.horizontal(|ui| {
                ui.label("Cloud Cover (%):");
                ui.add(
                    egui::DragValue::new(&mut self.cloud_cover)
                        .speed(0.1)
                        .clamp_range(0.0..=100.0),
                );
            });

            ui.horizontal(|ui| {
                ui.label("Point (lat,lon):");
                ui.text_edit_singleline(&mut self.point);
            });

            ui.horizontal(|ui| {
                ui.label("Bounding Box (tlat,llon,blat,rlon):");
                ui.text_edit_singleline(&mut self.bbox);
            });

            ui.horizontal(|ui| {
                ui.label("GeoJSON Path:");
                ui.text_edit_singleline(&mut self.geojson);
            });

            ui.horizontal(|ui| {
                ui.label("Max Results:");
                ui.add(
                    egui::DragValue::new(&mut self.max_results)
                        .speed(1.0)
                        .clamp_range(1..=100),
                );
            });

            if ui.button("Search").clicked() {
                self.search_products();
            }

            ui.separator();

            egui::ScrollArea::vertical().show(ui, |ui| {
                for product in &self.products {
                    ui.horizontal(|ui| {
                        ui.label(&product.name);
                        ui.push_id(&product.id, |ui| {
                            if ui.button("Download").clicked() {
                                self.download_product(product);
                            }
                        });
                    });
                }
            });
        });
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([800.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Copernicus Explorer",
        options,
        Box::new(|cc| Box::new(CopernicusExplorerApp::new(cc))),
    )
}
