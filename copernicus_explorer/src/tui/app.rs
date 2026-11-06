use crate::{
    BoundingBox, CopernicusError, DownloadProgressEvent, Geometry, OutputDestination, Point,
    Product, Satellite, SearchQuery, download_by_id_to_with_progress, get_access_token_from_env,
};
use chrono::{Duration, Utc};
use ratatui::layout::{Position, Rect};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex, mpsc};
use std::time::Instant;
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

/// Outer rectangles of the three panes from the last drawn frame.
#[derive(Debug, Clone, Copy, Default)]
pub struct PaneAreas {
    pub filters: Rect,
    pub results: Rect,
    pub downloads: Rect,
}

impl PaneAreas {
    fn hit(self, column: u16, row: u16) -> Option<Pane> {
        let position = Position::new(column, row);
        if self.filters.contains(position) {
            Some(Pane::Filters)
        } else if self.results.contains(position) {
            Some(Pane::Results)
        } else if self.downloads.contains(position) {
            Some(Pane::Downloads)
        } else {
            None
        }
    }
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
    /// When the transfer actually started (for speed / ETA).
    pub started_at: Option<Instant>,
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
    /// First visible results row. Kept so the cursor stays on screen.
    pub results_offset: usize,
    /// Data rows that fit in the results pane, updated while drawing.
    pub results_viewport: usize,
    /// Product IDs marked for batch download (Space to toggle).
    pub marked: HashSet<String>,
    /// Product IDs successfully downloaded during this session.
    pub downloaded_ids: HashSet<String>,
    pub selected_download: usize,
    /// First visible downloads row.
    pub downloads_offset: usize,
    /// Rows that fit in the downloads pane, updated while drawing.
    pub downloads_viewport: usize,
    pub downloads: Arc<Mutex<HashMap<String, DownloadState>>>,
    pub download_order: Vec<String>,
    pub focus: Pane,
    /// Pane rectangles from the last frame. Empty until the first draw.
    pub pane_areas: PaneAreas,
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
            results_offset: 0,
            results_viewport: 1,
            marked: HashSet::new(),
            downloaded_ids: HashSet::new(),
            selected_download: 0,
            downloads_offset: 0,
            downloads_viewport: 1,
            downloads: Arc::new(Mutex::new(HashMap::new())),
            download_order: Vec::new(),
            focus: Pane::Filters,
            pane_areas: PaneAreas::default(),
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
                                self.results_offset = 0;
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
                                self.results_offset = 0;
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
                        started_at: None,
                        status: DownloadUiStatus::Downloading,
                    });
                    match event {
                        DownloadProgressEvent::Started { label, total } => {
                            state.label = label;
                            state.total = total;
                            state.downloaded = 0;
                            state.started_at = Some(Instant::now());
                            state.status = DownloadUiStatus::Downloading;
                        }
                        DownloadProgressEvent::Progress { downloaded } => {
                            if state.started_at.is_none() {
                                state.started_at = Some(Instant::now());
                            }
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

    /// Focus the pane drawn at this cell. A click outside the panes does nothing.
    /// Switching panes leaves edit mode; clicking the focused pane does not.
    pub fn focus_pane_at(&mut self, column: u16, row: u16) {
        let Some(pane) = self.pane_areas.hit(column, row) else {
            return;
        };
        if pane != self.focus {
            self.focus = pane;
            self.editing = false;
        }
    }

    /// Move one row in the list under the pointer. `down` follows the wheel
    /// toward the user. Editing ignores the wheel, as it ignores arrow keys.
    pub fn scroll_at(&mut self, column: u16, row: u16, down: bool) {
        if self.editing {
            return;
        }
        match self.pane_areas.hit(column, row) {
            Some(Pane::Results) => {
                self.focus = Pane::Results;
                if down {
                    self.select_next_result();
                } else {
                    self.select_prev_result();
                }
            }
            Some(Pane::Downloads) => {
                self.focus = Pane::Downloads;
                if down {
                    self.select_next_download();
                } else {
                    self.select_prev_download();
                }
            }
            _ => {}
        }
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

    pub fn page_next_result(&mut self) {
        self.shift_result(self.results_viewport.max(1) as isize);
    }

    pub fn page_prev_result(&mut self) {
        self.shift_result(-(self.results_viewport.max(1) as isize));
    }

    pub fn select_first_result(&mut self) {
        if !self.products.is_empty() {
            self.selected_result = 0;
        }
    }

    pub fn select_last_result(&mut self) {
        if !self.products.is_empty() {
            self.selected_result = self.products.len() - 1;
        }
    }

    pub fn page_next_download(&mut self) {
        self.shift_download(self.downloads_viewport.max(1) as isize);
    }

    pub fn page_prev_download(&mut self) {
        self.shift_download(-(self.downloads_viewport.max(1) as isize));
    }

    pub fn select_first_download(&mut self) {
        if !self.download_order.is_empty() {
            self.selected_download = 0;
        }
    }

    pub fn select_last_download(&mut self) {
        if !self.download_order.is_empty() {
            self.selected_download = self.download_order.len() - 1;
        }
    }

    fn shift_result(&mut self, delta: isize) {
        if self.products.is_empty() {
            return;
        }
        let last = self.products.len() as isize - 1;
        self.selected_result = (self.selected_result as isize + delta).clamp(0, last) as usize;
    }

    fn shift_download(&mut self, delta: isize) {
        if self.download_order.is_empty() {
            return;
        }
        let last = self.download_order.len() as isize - 1;
        self.selected_download = (self.selected_download as isize + delta).clamp(0, last) as usize;
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
                        started_at: None,
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

/// Slide `offset` so `selected` stays inside a window of `visible` rows.
///
/// The window does not move while the cursor is already on screen. It scrolls
/// by just enough when the cursor crosses either edge, then clamps so a taller
/// viewport cannot leave a gap after the last row.
pub fn reveal(offset: &mut usize, selected: usize, len: usize, visible: usize) {
    if len == 0 || visible == 0 {
        *offset = 0;
        return;
    }
    let selected = selected.min(len - 1);
    if selected < *offset {
        *offset = selected;
    } else if selected >= (*offset).saturating_add(visible) {
        *offset = selected.saturating_add(1).saturating_sub(visible);
    }
    *offset = (*offset).min(len.saturating_sub(visible));
}

pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[0])
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}

pub fn format_rate(bytes_per_sec: f64) -> String {
    if !bytes_per_sec.is_finite() || bytes_per_sec <= 0.0 {
        return "—/s".to_string();
    }
    format!("{}/s", format_bytes(bytes_per_sec as u64))
}

pub fn format_eta(remaining_secs: f64) -> String {
    if !remaining_secs.is_finite() || remaining_secs < 0.0 {
        return "—".to_string();
    }
    let secs = remaining_secs.round() as u64;
    let hours = secs / 3600;
    let mins = (secs % 3600) / 60;
    let secs = secs % 60;
    if hours > 0 {
        format!("{hours}h{mins:02}m")
    } else if mins > 0 {
        format!("{mins}m{secs:02}s")
    } else {
        format!("{secs}s")
    }
}

/// Instantaneous-ish transfer rate from bytes downloaded since `started_at`.
pub fn download_rate(downloaded: u64, started_at: Option<Instant>) -> Option<f64> {
    let started = started_at?;
    let elapsed = started.elapsed().as_secs_f64();
    if elapsed < 0.05 {
        return None;
    }
    Some(downloaded as f64 / elapsed)
}

#[cfg(test)]
mod tests {
    use super::{App, Pane, PaneAreas, reveal};
    use crate::Product;
    use ratatui::layout::Rect;

    fn product(name: &str) -> Product {
        Product {
            name: name.to_string(),
            id: name.to_string(),
            acquisition_date: "2026-03-12T10:40:31.000Z".to_string(),
            publication_date: "2026-03-12T10:40:31.000Z".to_string(),
            online: true,
            cloud_cover: None,
        }
    }

    fn sample_areas() -> PaneAreas {
        PaneAreas {
            filters: Rect::new(0, 1, 38, 12),
            results: Rect::new(38, 1, 62, 12),
            downloads: Rect::new(0, 13, 100, 9),
        }
    }

    #[test]
    fn focus_pane_at_selects_each_pane() {
        let mut app = App::new();
        app.pane_areas = sample_areas();

        app.focus_pane_at(1, 2);
        assert_eq!(app.focus, Pane::Filters);

        app.focus_pane_at(50, 2);
        assert_eq!(app.focus, Pane::Results);

        app.focus_pane_at(10, 15);
        assert_eq!(app.focus, Pane::Downloads);
    }

    #[test]
    fn focus_pane_at_ignores_the_status_row() {
        let mut app = App::new();
        app.pane_areas = sample_areas();
        app.focus = Pane::Results;

        app.focus_pane_at(10, 22);

        assert_eq!(app.focus, Pane::Results);
    }

    #[test]
    fn focus_pane_at_clears_editing_only_when_the_pane_changes() {
        let mut app = App::new();
        app.pane_areas = sample_areas();
        app.focus = Pane::Filters;
        app.editing = true;

        app.focus_pane_at(1, 2);
        assert_eq!(app.focus, Pane::Filters);
        assert!(app.editing);

        app.focus_pane_at(50, 2);
        assert_eq!(app.focus, Pane::Results);
        assert!(!app.editing);
    }

    #[test]
    fn scroll_at_moves_the_result_under_the_pointer_by_one_row() {
        let mut app = App::new();
        app.pane_areas = sample_areas();
        app.focus = Pane::Filters;
        app.products = vec![product("a"), product("b"), product("c")];
        app.selected_result = 0;

        app.scroll_at(50, 2, true);
        assert_eq!(app.focus, Pane::Results);
        assert_eq!(app.selected_result, 1);

        app.scroll_at(10, 22, true);
        assert_eq!(app.selected_result, 1);
    }

    #[test]
    fn scroll_at_is_ignored_while_editing() {
        let mut app = App::new();
        app.pane_areas = sample_areas();
        app.focus = Pane::Filters;
        app.editing = true;
        app.products = vec![product("a"), product("b")];

        app.scroll_at(50, 2, true);

        assert!(app.editing);
        assert_eq!(app.focus, Pane::Filters);
        assert_eq!(app.selected_result, 0);
    }

    #[test]
    fn scroll_at_moves_the_download_under_the_pointer_by_one_row() {
        let mut app = App::new();
        app.pane_areas = sample_areas();
        app.download_order = vec!["a".into(), "b".into(), "c".into()];
        app.selected_download = 0;

        app.scroll_at(10, 15, true);

        assert_eq!(app.focus, Pane::Downloads);
        assert_eq!(app.selected_download, 1);
    }

    #[test]
    fn reveal_keeps_offset_while_cursor_is_inside_the_window() {
        let mut offset = 4;
        reveal(&mut offset, 8, 30, 10);
        assert_eq!(offset, 4);
    }

    #[test]
    fn reveal_scrolls_when_cursor_passes_the_bottom() {
        let mut offset = 0;
        reveal(&mut offset, 10, 30, 10);
        assert_eq!(offset, 1);
    }

    #[test]
    fn reveal_scrolls_when_cursor_passes_the_top() {
        let mut offset = 4;
        reveal(&mut offset, 3, 30, 10);
        assert_eq!(offset, 3);
    }

    #[test]
    fn reveal_clamps_offset_when_the_viewport_grows() {
        let mut offset = 20;
        reveal(&mut offset, 22, 30, 25);
        assert_eq!(offset, 5);
    }

    #[test]
    fn reveal_resets_offset_for_an_empty_list() {
        let mut offset = 4;
        reveal(&mut offset, 0, 0, 10);
        assert_eq!(offset, 0);
    }
}
