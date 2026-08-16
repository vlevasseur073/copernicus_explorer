use chrono::{Duration, Utc};
use copernicus_explorer::{
    BoundingBox, CopernicusError, DownloadProgressEvent, Geometry, OutputDestination, Point,
    Product, Satellite, SearchQuery, download_by_id_to_with_progress, get_access_token_from_env,
};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex, mpsc};
use tokio::runtime::Runtime;
use tokio::sync::Semaphore;

/// Max parallel CDSE downloads (same default as the CLI `-j`).
pub const MAX_CONCURRENT_DOWNLOADS: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Filters,
    Results,
    Downloads,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterField {
    Satellite,
    Product,
    StartDate,
    EndDate,
    Tile,
    CloudCover,
    Point,
    Bbox,
    Geojson,
    MaxResults,
}

impl FilterField {
    pub const ALL: [FilterField; 10] = [
        FilterField::Satellite,
        FilterField::Product,
        FilterField::StartDate,
        FilterField::EndDate,
        FilterField::Tile,
        FilterField::CloudCover,
        FilterField::Point,
        FilterField::Bbox,
        FilterField::Geojson,
        FilterField::MaxResults,
    ];

    pub fn label(self) -> &'static str {
        match self {
            FilterField::Satellite => "Satellite",
            FilterField::Product => "Product",
            FilterField::StartDate => "Start date",
            FilterField::EndDate => "End date",
            FilterField::Tile => "Tile",
            FilterField::CloudCover => "Cloud cover %",
            FilterField::Point => "Point (lat,lon)",
            FilterField::Bbox => "BBox (tlat,llon,blat,rlon)",
            FilterField::Geojson => "GeoJSON path",
            FilterField::MaxResults => "Max results",
        }
    }

    pub fn is_text(self) -> bool {
        !matches!(self, FilterField::Satellite | FilterField::Product)
    }
}

#[derive(Clone, Debug)]
pub enum DownloadUiStatus {
    Downloading,
    Completed(String),
    Failed(String),
}

#[derive(Clone, Debug)]
pub struct DownloadState {
    pub label: String,
    pub downloaded: u64,
    pub total: Option<u64>,
    pub status: DownloadUiStatus,
}

pub enum AppMessage {
    SearchFinished {
        outcome: SearchOutcome,
        append: bool,
    },
    DownloadProgress {
        id: String,
        event: DownloadProgressEvent,
    },
}

/// Minimum interval between Progress events forwarded to the UI (per product).
const PROGRESS_UI_INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);

pub enum SearchOutcome {
    Success(Vec<Product>),
    Empty,
    Failed(String),
}

pub struct App {
    pub satellite: Satellite,
    pub product: String,
    pub start_date: String,
    pub end_date: String,
    pub tile: String,
    pub cloud_cover: String,
    pub point: String,
    pub bbox: String,
    pub geojson: String,
    pub max_results: String,
    pub products: Vec<Product>,
    pub selected_result: usize,
    /// Product IDs marked for batch download (Space to toggle).
    pub marked: HashSet<String>,
    /// Product IDs successfully downloaded during this session.
    pub downloaded_ids: HashSet<String>,
    pub selected_download: usize,
    pub downloads: Arc<Mutex<HashMap<String, DownloadState>>>,
    pub download_order: Vec<String>,
    pub focus: Pane,
    pub filter_field: FilterField,
    pub editing: bool,
    pub searching: bool,
    pub status: String,
    pub should_quit: bool,
    pub tx: mpsc::Sender<AppMessage>,
    rx: mpsc::Receiver<AppMessage>,
    runtime: Arc<Runtime>,
    download_semaphore: Arc<Semaphore>,
}

impl App {
    pub fn new() -> Self {
        let end_date = Utc::now().date_naive();
        let start_date = end_date - Duration::days(7);
        let (tx, rx) = mpsc::channel();

        Self {
            satellite: Satellite::Sentinel2,
            product: Satellite::Sentinel2.known_products()[0].to_string(),
            start_date: start_date.format("%Y-%m-%d").to_string(),
            end_date: end_date.format("%Y-%m-%d").to_string(),
            tile: String::new(),
            cloud_cover: "15".to_string(),
            point: String::new(),
            bbox: String::new(),
            geojson: String::new(),
            max_results: "10".to_string(),
            products: Vec::new(),
            selected_result: 0,
            marked: HashSet::new(),
            downloaded_ids: HashSet::new(),
            selected_download: 0,
            downloads: Arc::new(Mutex::new(HashMap::new())),
            download_order: Vec::new(),
            focus: Pane::Filters,
            filter_field: FilterField::Satellite,
            editing: false,
            searching: false,
            status: "Ready — s search · S append · see footer for keys".to_string(),
            should_quit: false,
            tx,
            rx,
            runtime: Arc::new(Runtime::new().expect("Failed to create Tokio runtime")),
            download_semaphore: Arc::new(Semaphore::new(MAX_CONCURRENT_DOWNLOADS)),
        }
    }

    pub fn poll_messages(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                AppMessage::SearchFinished { outcome, append } => {
                    self.searching = false;
                    match outcome {
                        SearchOutcome::Success(products) => {
                            if append {
                                let before = self.products.len();
                                let mut seen: HashSet<String> =
                                    self.products.iter().map(|p| p.id.clone()).collect();
                                let mut added = 0;
                                for product in products {
                                    if seen.insert(product.id.clone()) {
                                        self.products.push(product);
                                        added += 1;
                                    }
                                }
                                self.status = format!(
                                    "Appended {added} new product(s) ({before} → {})",
                                    self.products.len()
                                );
                            } else {
                                let n = products.len();
                                self.products = products;
                                self.selected_result = 0;
                                self.marked.clear();
                                self.status = format!("Found {n} product(s)");
                            }
                            if !self.products.is_empty() {
                                self.focus = Pane::Results;
                            }
                        }
                        SearchOutcome::Empty => {
                            if append {
                                self.status =
                                    "No new products matched — existing list kept".to_string();
                            } else {
                                self.products.clear();
                                self.selected_result = 0;
                                self.marked.clear();
                                self.status =
                                    "No products matched these filters — try relaxing dates, cloud cover, or AOI"
                                        .to_string();
                            }
                        }
                        SearchOutcome::Failed(err) => {
                            // Avoid "Search failed: search failed: …" from CopernicusError display.
                            let msg = err
                                .strip_prefix("search failed: ")
                                .or_else(|| err.strip_prefix("invalid argument: "))
                                .unwrap_or(err.as_str());
                            self.status = format!("Search failed: {msg}");
                        }
                    }
                }
                AppMessage::DownloadProgress { id, event } => {
                    let mut map = self.downloads.lock().unwrap();
                    let state = map.entry(id.clone()).or_insert_with(|| DownloadState {
                        label: id.clone(),
                        downloaded: 0,
                        total: None,
                        status: DownloadUiStatus::Downloading,
                    });
                    match event {
                        DownloadProgressEvent::Started { label, total } => {
                            state.label = label;
                            state.total = total;
                            state.status = DownloadUiStatus::Downloading;
                        }
                        DownloadProgressEvent::Progress { downloaded } => {
                            state.downloaded = downloaded;
                        }
                        DownloadProgressEvent::Completed { path } => {
                            state.status = DownloadUiStatus::Completed(path.clone());
                            self.downloaded_ids.insert(id.clone());
                            self.marked.remove(&id);
                            self.status = format!("Download complete: {path}");
                        }
                        DownloadProgressEvent::Failed { message } => {
                            state.status = DownloadUiStatus::Failed(message.clone());
                            self.status = format!("Download failed: {message}");
                        }
                    }
                }
            }
        }
    }

    pub fn field_value(&self, field: FilterField) -> String {
        match field {
            FilterField::Satellite => self.satellite.to_string(),
            FilterField::Product => self.product.clone(),
            FilterField::StartDate => self.start_date.clone(),
            FilterField::EndDate => self.end_date.clone(),
            FilterField::Tile => self.tile.clone(),
            FilterField::CloudCover => self.cloud_cover.clone(),
            FilterField::Point => self.point.clone(),
            FilterField::Bbox => self.bbox.clone(),
            FilterField::Geojson => self.geojson.clone(),
            FilterField::MaxResults => self.max_results.clone(),
        }
    }

    pub fn field_value_mut(&mut self, field: FilterField) -> Option<&mut String> {
        match field {
            FilterField::Satellite | FilterField::Product => None,
            FilterField::StartDate => Some(&mut self.start_date),
            FilterField::EndDate => Some(&mut self.end_date),
            FilterField::Tile => Some(&mut self.tile),
            FilterField::CloudCover => Some(&mut self.cloud_cover),
            FilterField::Point => Some(&mut self.point),
            FilterField::Bbox => Some(&mut self.bbox),
            FilterField::Geojson => Some(&mut self.geojson),
            FilterField::MaxResults => Some(&mut self.max_results),
        }
    }

    pub fn cycle_satellite(&mut self, forward: bool) {
        const ALL: [Satellite; 5] = [
            Satellite::Sentinel1,
            Satellite::Sentinel2,
            Satellite::Sentinel3,
            Satellite::Sentinel5P,
            Satellite::Sentinel6,
        ];
        let idx = ALL.iter().position(|s| *s == self.satellite).unwrap_or(1);
        let next = if forward {
            (idx + 1) % ALL.len()
        } else {
            (idx + ALL.len() - 1) % ALL.len()
        };
        self.satellite = ALL[next];
        self.product = self.satellite.known_products()[0].to_string();
        // Cloud cover is rejected by the API for Sentinel-1.
        if matches!(self.satellite, Satellite::Sentinel1) {
            self.cloud_cover.clear();
        } else if self.cloud_cover.is_empty() {
            self.cloud_cover = "15".to_string();
        }
    }

    pub fn cycle_product(&mut self, forward: bool) {
        let products = self.satellite.known_products();
        let idx = products
            .iter()
            .position(|p| *p == self.product.as_str())
            .unwrap_or(0);
        let next = if forward {
            (idx + 1) % products.len()
        } else {
            (idx + products.len() - 1) % products.len()
        };
        self.product = products[next].to_string();
    }

    pub fn next_filter_field(&mut self) {
        let idx = FilterField::ALL
            .iter()
            .position(|f| *f == self.filter_field)
            .unwrap_or(0);
        self.filter_field = FilterField::ALL[(idx + 1) % FilterField::ALL.len()];
        self.editing = false;
    }

    pub fn prev_filter_field(&mut self) {
        let idx = FilterField::ALL
            .iter()
            .position(|f| *f == self.filter_field)
            .unwrap_or(0);
        self.filter_field =
            FilterField::ALL[(idx + FilterField::ALL.len() - 1) % FilterField::ALL.len()];
        self.editing = false;
    }

    pub fn cycle_pane(&mut self) {
        self.focus = match self.focus {
            Pane::Filters => Pane::Results,
            Pane::Results => Pane::Downloads,
            Pane::Downloads => Pane::Filters,
        };
        self.editing = false;
    }

    pub fn cycle_pane_back(&mut self) {
        self.focus = match self.focus {
            Pane::Filters => Pane::Downloads,
            Pane::Results => Pane::Filters,
            Pane::Downloads => Pane::Results,
        };
        self.editing = false;
    }

    pub fn is_downloaded(&self, product_id: &str) -> bool {
        self.downloaded_ids.contains(product_id)
    }

    pub fn is_downloading(&self, product_id: &str) -> bool {
        self.downloads
            .lock()
            .unwrap()
            .get(product_id)
            .is_some_and(|s| matches!(s.status, DownloadUiStatus::Downloading))
    }

    pub fn select_next_result(&mut self) {
        if !self.products.is_empty() {
            self.selected_result = (self.selected_result + 1) % self.products.len();
        }
    }

    pub fn select_prev_result(&mut self) {
        if !self.products.is_empty() {
            self.selected_result =
                (self.selected_result + self.products.len() - 1) % self.products.len();
        }
    }

    pub fn select_next_download(&mut self) {
        if !self.download_order.is_empty() {
            self.selected_download = (self.selected_download + 1) % self.download_order.len();
        }
    }

    pub fn select_prev_download(&mut self) {
        if !self.download_order.is_empty() {
            self.selected_download = (self.selected_download + self.download_order.len() - 1)
                % self.download_order.len();
        }
    }

    pub fn start_search(&mut self, append: bool) {
        if self.searching {
            self.status = "Search already in progress".to_string();
            return;
        }

        // Set synchronously so the UI updates immediately and we cannot
        // double-spawn before the first channel message is polled.
        self.searching = true;
        self.status = if append {
            "Searching (append)…".to_string()
        } else {
            "Searching…".to_string()
        };

        let params = SearchParams {
            satellite: self.satellite,
            product: self.product.trim().to_string(),
            start_date: self.start_date.trim().to_string(),
            end_date: self.end_date.trim().to_string(),
            tile: self.tile.trim().to_string(),
            cloud_cover: self.cloud_cover.trim().to_string(),
            point: self.point.trim().to_string(),
            bbox: self.bbox.trim().to_string(),
            geojson: self.geojson.trim().to_string(),
            max_results: self.max_results.trim().to_string(),
        };
        let tx = self.tx.clone();
        let runtime = self.runtime.clone();

        runtime.spawn(async move {
            let outcome = build_and_execute_search(params).await;
            let _ = tx.send(AppMessage::SearchFinished { outcome, append });
        });
    }

    pub fn toggle_mark_selected(&mut self) {
        let Some(product) = self.products.get(self.selected_result) else {
            return;
        };
        if !self.marked.remove(&product.id) {
            self.marked.insert(product.id.clone());
        }
        let n = self.marked.len();
        self.status = if n == 0 {
            "No products marked".to_string()
        } else {
            format!("{n} product(s) marked for download")
        };
    }

    pub fn start_download_selected(&mut self) {
        let targets: Vec<(String, String)> = if self.marked.is_empty() {
            self.products
                .get(self.selected_result)
                .map(|p| vec![(p.id.clone(), p.name.clone())])
                .unwrap_or_default()
        } else {
            self.products
                .iter()
                .filter(|p| self.marked.contains(&p.id))
                .map(|p| (p.id.clone(), p.name.clone()))
                .collect()
        };

        if targets.is_empty() {
            self.status = "No product selected".to_string();
            return;
        }

        self.enqueue_downloads(targets);
    }

    pub fn start_download_all(&mut self) {
        if self.products.is_empty() {
            self.status = "No products to download — search first".to_string();
            return;
        }
        let targets: Vec<(String, String)> = self
            .products
            .iter()
            .map(|p| (p.id.clone(), p.name.clone()))
            .collect();
        self.enqueue_downloads(targets);
    }

    fn enqueue_downloads(&mut self, targets: Vec<(String, String)>) {
        let mut queued = Vec::new();

        {
            let mut map = self.downloads.lock().unwrap();
            for (product_id, product_name) in targets {
                if let Some(existing) = map.get(&product_id) {
                    match &existing.status {
                        DownloadUiStatus::Downloading | DownloadUiStatus::Completed(_) => {
                            continue;
                        }
                        DownloadUiStatus::Failed(_) => {}
                    }
                }
                map.insert(
                    product_id.clone(),
                    DownloadState {
                        label: product_name.clone(),
                        downloaded: 0,
                        total: None,
                        status: DownloadUiStatus::Downloading,
                    },
                );
                if !self.download_order.contains(&product_id) {
                    self.download_order.push(product_id.clone());
                }
                queued.push((product_id, product_name));
            }
        }

        if queued.is_empty() {
            self.status = "Nothing new to download (already queued or completed)".to_string();
            return;
        }

        let n = queued.len();
        self.marked.clear();
        // Keep focus where the user is (usually Results) so they can keep
        // browsing / queuing more downloads while transfers run in the background.
        self.status =
            format!("Queued {n} download(s) (max {MAX_CONCURRENT_DOWNLOADS} concurrent)…");

        let tx = self.tx.clone();
        let runtime = self.runtime.clone();
        let semaphore = self.download_semaphore.clone();

        runtime.spawn(async move {
            let token = match get_access_token_from_env().await {
                Ok(token) => token,
                Err(error) => {
                    let message = error.to_string();
                    for (product_id, _) in &queued {
                        let _ = tx.send(AppMessage::DownloadProgress {
                            id: product_id.clone(),
                            event: DownloadProgressEvent::Failed {
                                message: message.clone(),
                            },
                        });
                    }
                    return;
                }
            };

            let dest = OutputDestination::Local(".".into());
            let mut handles = Vec::with_capacity(queued.len());

            for (product_id, _product_name) in queued {
                let sem = semaphore.clone();
                let tx = tx.clone();
                let token = token.clone();
                let dest = dest.clone();
                let progress_id = product_id.clone();

                let handle = tokio::spawn(async move {
                    let _permit = match sem.acquire().await {
                        Ok(permit) => permit,
                        Err(error) => {
                            let _ = tx.send(AppMessage::DownloadProgress {
                                id: progress_id,
                                event: DownloadProgressEvent::Failed {
                                    message: error.to_string(),
                                },
                            });
                            return;
                        }
                    };

                    // Throttle Progress events so the UI loop stays responsive.
                    let last_sent =
                        std::sync::Mutex::new(std::time::Instant::now() - PROGRESS_UI_INTERVAL);
                    let progress = Arc::new(move |event: DownloadProgressEvent| {
                        if let DownloadProgressEvent::Progress { .. } = &event {
                            let mut last = last_sent.lock().unwrap();
                            if last.elapsed() < PROGRESS_UI_INTERVAL {
                                return;
                            }
                            *last = std::time::Instant::now();
                        }
                        let _ = tx.send(AppMessage::DownloadProgress {
                            id: progress_id.clone(),
                            event,
                        });
                    });

                    let _ =
                        download_by_id_to_with_progress(&product_id, &dest, &token, progress).await;
                    drop(_permit);
                });
                handles.push(handle);
            }

            for handle in handles {
                let _ = handle.await;
            }
        });
    }
}

struct SearchParams {
    satellite: Satellite,
    product: String,
    start_date: String,
    end_date: String,
    tile: String,
    cloud_cover: String,
    point: String,
    bbox: String,
    geojson: String,
    max_results: String,
}

async fn build_and_execute_search(params: SearchParams) -> SearchOutcome {
    let SearchParams {
        satellite,
        product,
        start_date,
        end_date,
        tile,
        cloud_cover,
        point,
        bbox,
        geojson,
        max_results,
    } = params;

    let mut query = SearchQuery::new(satellite);

    if !product.is_empty() {
        query = query.product(product);
    }

    if !start_date.is_empty() && !end_date.is_empty() {
        let start_dt = match chrono::NaiveDate::parse_from_str(&start_date, "%Y-%m-%d") {
            Ok(d) => match d.and_hms_opt(0, 0, 0) {
                Some(dt) => dt.and_utc(),
                None => return SearchOutcome::Failed("Invalid start date time".to_string()),
            },
            Err(e) => return SearchOutcome::Failed(format!("Invalid start date: {e}")),
        };
        let end_dt = match chrono::NaiveDate::parse_from_str(&end_date, "%Y-%m-%d") {
            Ok(d) => match d.and_hms_opt(0, 0, 0) {
                Some(dt) => dt.and_utc(),
                None => return SearchOutcome::Failed("Invalid end date time".to_string()),
            },
            Err(e) => return SearchOutcome::Failed(format!("Invalid end date: {e}")),
        };
        query = query.dates(start_dt, end_dt);
    }

    if !tile.is_empty() {
        query = query.tile(tile);
    }

    // Cloud cover is rejected by CDSE for Sentinel-1.
    if !cloud_cover.is_empty() && !matches!(satellite, Satellite::Sentinel1) {
        match cloud_cover.parse::<f64>() {
            Ok(cover) if cover > 0.0 => query = query.max_cloud_cover(cover),
            Ok(_) => {}
            Err(_) => {
                return SearchOutcome::Failed(format!("Invalid cloud cover: {cloud_cover}"));
            }
        }
    }

    if !point.is_empty() {
        let parts: Result<Vec<f64>, _> = point.split(',').map(|s| s.trim().parse()).collect();
        match parts {
            Ok(parts) if parts.len() == 2 => {
                query = query.geometry(Geometry::Point(Point::new(parts[0], parts[1])));
            }
            Ok(_) => return SearchOutcome::Failed("Point must be lat,lon".to_string()),
            Err(_) => return SearchOutcome::Failed(format!("Invalid point: {point}")),
        }
    } else if !bbox.is_empty() {
        let parts: Result<Vec<f64>, _> = bbox.split(',').map(|s| s.trim().parse()).collect();
        match parts {
            Ok(parts) if parts.len() == 4 => {
                query = query.geometry(Geometry::BoundingBox(BoundingBox::new(
                    (parts[0], parts[1]),
                    (parts[2], parts[3]),
                )));
            }
            Ok(_) => {
                return SearchOutcome::Failed("BBox must be tlat,llon,blat,rlon".to_string());
            }
            Err(_) => return SearchOutcome::Failed(format!("Invalid bbox: {bbox}")),
        }
    } else if !geojson.is_empty() {
        match Geometry::from_geojson_file(Path::new(&geojson)) {
            Ok(geometry) => query = query.geometry(geometry),
            Err(e) => return SearchOutcome::Failed(format!("Invalid GeoJSON: {e}")),
        }
    }

    let max: u32 = if max_results.is_empty() {
        10
    } else {
        match max_results.parse() {
            Ok(n) => n,
            Err(_) => {
                return SearchOutcome::Failed(format!("Invalid max results: {max_results}"));
            }
        }
    };
    query = query.max_results(max);

    match query.execute().await {
        Ok(products) => SearchOutcome::Success(products),
        Err(CopernicusError::NoResults) => SearchOutcome::Empty,
        Err(err) => SearchOutcome::Failed(err.to_string()),
    }
}

pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[0])
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}
