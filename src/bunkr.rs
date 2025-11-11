use crate::{egui_printer, egui_println};

use spider::website::Website;
use spider::tokio;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use fake_user_agent::get_firefox_rua;
use scraper::{Html, Selector};
use reqwest;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::{time::Duration, time::Instant};
use futures::stream::{self, StreamExt};

const CONCURRENT_LIMIT: usize = 8;
const DEFAULT_DOWNLOAD_DIR: &str = "no_title";
const DEFAULT_BASE_DIR: &str = ".";
const RETRY_COUNT: usize = 3;  // 单文件重试次数
const MIN_SPEED_BPS: u64 = 0; // 最低下载速率（1KB/s）
const MIN_FILE_SIZE: u64 = 200; // 最小文件大小阈值（200B）

#[derive(PartialEq, Debug, Clone, Copy)]
pub enum BunkrSpiderState {
    Idle,
    Analyzing,
    Downloading,
    Finished,
}

#[derive(Clone)]
pub struct BunkrSpiderInformation {
    pub total_sources: Option<usize>,
    pub downloaded_sources: Option<usize>,
    pub state: BunkrSpiderState,
}

#[derive(Clone)]
pub struct BunkrSpider {
    website: Website,
    client: reqwest::Client,
    title: String,
    sources: Vec<(String, String)>,
    base_dir: String,
    should_stop: Arc<AtomicBool>,
    info: Arc<tokio::sync::RwLock<BunkrSpiderInformation>>,
}


impl BunkrSpider{
    pub fn new() -> BunkrSpider{
        BunkrSpider { 
            website: (Website::new("")), 
            client: (reqwest::Client::new()), 
            title: (String::new()), 
            sources: (Vec::<(String, String)>::new()),
            base_dir: (DEFAULT_BASE_DIR.to_string()),
            should_stop: Arc::new(AtomicBool::new(false)),
            info: Arc::new(tokio::sync::RwLock::new(BunkrSpiderInformation {
                total_sources: None,
                downloaded_sources: None,
                state: BunkrSpiderState::Idle,
            })),
        }
    }

    /// Construct a BunkrSpider that shares the provided `info` Arc.
    /// This allows the UI to read the same internal state while the spider
    /// holds its own lock during long-running operations.
    pub fn with_info(info: Arc<tokio::sync::RwLock<BunkrSpiderInformation>>) -> BunkrSpider {
        let mut s = BunkrSpider::new();
        s.info = info;
        s
    }

    pub async fn run(&mut self, base_dir: String, url: String) -> Arc<tokio::sync::RwLock<BunkrSpiderInformation>> {
        self.base_dir = base_dir;

        let website_name = url.trim().split('?').next().unwrap().to_string();
        self.website = Website::new(&website_name);
        self.website.with_user_agent(Some(get_firefox_rua()));

        if let Ok(mut info) = self.info.try_write(){
            info.state = BunkrSpiderState::Analyzing;
        }

        // 在持有锁期间定期释放，允许GUI读取
        self.website.scrape().await;
        // 释放锁，让GUI能读取状态
        tokio::task::yield_now().await;

        let meta_type_selector = Selector::parse("head > meta:nth-child(5)").unwrap();
        let meta_title_selector = Selector::parse("head > meta:nth-child(6)").unwrap();
        let meta_title_png_selector = Selector::parse("head > meta:nth-child(7)").unwrap();
        let img_selector = Selector::parse(
            "body > main:nth-child(11) > figure > img.max-h-full.w-auto.object-cover.relative.z-20"
        ).unwrap();

        for (page_count, page) in self.website.get_pages().unwrap().iter().enumerate() {
            let html = page.get_html();
            let document = Html::parse_document(&html);

            // type
            if let Some(meta_type_elem) = document.select(&meta_type_selector).next() {
                let _type = meta_type_elem.value().attr("content").unwrap_or("");
                    if _type.is_empty() || _type == "website" {
                        continue;
                    }

                    // title
                    if let Some(meta_title_elem) = document.select(&meta_title_selector).next() {
                        let _title = meta_title_elem.value().attr("content").unwrap_or("");
                        // 记录album标题
                        if self.title.is_empty() && _type == "album"{
                            let _t = Self::sanitize_filename(_title);

                            if !_t.is_empty(){
                                egui_println!("album title: {}", _t);
                                self.title = _t;
                            }
                        }
                        if _type == "album"{
                            continue;
                        }

                        let title_etc = 
                            ".".to_string() + _title.rsplit('.').next().unwrap_or("");

                        if title_etc == ".jpg" || title_etc == ".png" || title_etc == ".gif" || title_etc == ".jpeg" {
                            // image
                            if let Some(img_elem) = document.select(&img_selector).next() {
                                let _img_src = img_elem.value().attr("src").unwrap_or("");
                                if _img_src.is_empty() {
                                    continue;
                                }
                                let real_link =  _img_src.to_string();

                                // egui_println!("Image --- title: {}\nlink: {}", _title, real_link);
                                self.sources.push((_title.to_string(), real_link));
                            }
                        }
                        // video
                        else{
                        // title_png
                        if let Some(meta_title_elem) = document.select(&meta_title_png_selector).next() {
                            let _title_png = meta_title_elem.value().attr("content").unwrap_or("");
                            let mut real_link =  Self::remove_all_extensions_after_last_slash(
                            _title_png
                                .replace("/thumbs", "")
                                .replace("https://i-", "https://")
                            );
                            real_link += &title_etc;

                            // egui_println!("Video --- title: {}\nlink: {}", _title, real_link);
                            self.sources.push((_title.to_string(), real_link));
                        }
                    }
                }
            }
            
            // 每处理几个页面就释放一次锁，让 GUI 能更新状态
            if page_count % 5 == 0 {
                tokio::task::yield_now().await;
            }
        }

        if let Ok(mut info) = self.info.try_write(){
            info.total_sources = Some(self.sources.len());
            info.downloaded_sources = Some(0);
        }

        Arc::clone(&self.info)
    }

    fn remove_all_extensions_after_last_slash(url: String) -> String {
        if let Some(last_slash) = url.rfind('/') {
            let (base, filename) = url.split_at(last_slash + 1);
            
            if let Some(first_dot) = filename.find('.') {
                format!("{}{}", base, &filename[..first_dot])
            } else {
                url.to_string()
            }
        } else {
            if let Some(first_dot) = url.find('.') {
                url[..first_dot].to_string()
            } else {
                url.to_string()
            }
        }
    }

    fn get_download_dir(&self) -> String {
        let mut path = PathBuf::from(&self.base_dir);
        if !path.exists() || !path.is_dir() {
            path = PathBuf::from(DEFAULT_BASE_DIR);
        }

        if self.title.is_empty() {
            path = path.join(DEFAULT_DOWNLOAD_DIR);
        } else {
            path = path.join(&self.title);
        };
        
        path.to_string_lossy().to_string()
    }

    pub async fn download_all(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let download_dir = self.get_download_dir();

        let _ = fs::create_dir_all(&download_dir).await;

        egui_println!("find {} sources", self.sources.len());

        if let Ok(mut info) = self.info.try_write(){
            info.state = BunkrSpiderState::Downloading;
        }

        let client = Arc::new(self.client.clone());
        let should_stop = Arc::clone(&self.should_stop);
        let info = Arc::clone(&self.info);

        // 创建所有权版本的 sources 向量，避免生命周期问题
        let sources_owned: Vec<_> = self.sources
            .iter()
            .map(|(name, url)| (name.clone(), url.clone()))
            .collect();

        let downloads = stream::iter(sources_owned.into_iter().enumerate())
            .map(move |(index, (name, url))| {
                let client = Arc::clone(&client);
                let should_stop = Arc::clone(&should_stop);
                let info = Arc::clone(&info);
                let dir = download_dir.clone();

                async move {
                    if should_stop.load(Ordering::Relaxed) {
                        return (name.clone(), Err("Task stopped".to_string()));
                    }
                    let result = Self::download_with_retry(
                        &client, dir, url, 
                        &name, index, should_stop, 
                        Arc::clone(&info)).await;
                    (name, result)
                }
            })
            .buffer_unordered(CONCURRENT_LIMIT);

        let results: Vec<_> = downloads.collect().await;

        for (name, result) in results {
            match result {
                Ok(_) => egui_println!("✓ downloaded: {}", name),
                Err(e) => egui_println!("✗ failed: {} - {}", name, e),
            }
        }

        egui_println!("all downloads attempted.");
        if let Ok(mut info) = self.info.try_write(){
            info.state = BunkrSpiderState::Finished;
        }
        Ok(())
    }


    async fn download_with_retry(
        client: &reqwest::Client,
        download_dir: String,
        url: String,
        filename: &str,
        index: usize,
        should_stop: Arc<AtomicBool>,
        info: Arc<tokio::sync::RwLock<BunkrSpiderInformation>>,
    ) -> Result<(), String> {
        let mut last_error = None;
        
        for attempt in 0..=RETRY_COUNT {
            if should_stop.load(Ordering::Relaxed) {
                return Err("Task stopped".to_string());
            }

            if attempt > 0 {
                egui_println!("try {} times for: {}", attempt, filename);
                tokio::time::sleep(Duration::from_secs(2u64.pow(attempt as u32))).await;
            }

            match Self::download_with_speed_check(
                client, &download_dir, &url, 
                filename, index, 
                Arc::clone(&info)).await {
                Ok(_) => {
                    if let Ok(mut info_lock) = info.try_write() {
                        if let Some(count) = info_lock.downloaded_sources {
                            info_lock.downloaded_sources = Some(count + 1);
                        }
                    }
                    return Ok(());
                },
                Err(e) => {
                    let error_msg = e.to_string();
                    egui_println!("download try {} failed: {} - {}", attempt + 1, filename, error_msg);
                    last_error = Some(error_msg);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| "all try failed".to_string()))
    }

    async fn download_with_speed_check(
        client: &reqwest::Client,
        download_dir: &str,
        url: &str,
        filename: &str,
        index: usize,
        _info: Arc<tokio::sync::RwLock<BunkrSpiderInformation>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let start_time = Instant::now();
        
        egui_println!("start download [{}/?]: {}", index + 1, filename);
        
        let response = client.get(url).send().await?;
        let total_size = response.content_length().unwrap_or(0);
        
        // 确保文件名有效
        let safe_filename = Self::sanitize_filename(filename);
        let filepath = format!("{}/{}", download_dir, safe_filename);

        let mut file = fs::File::create(&filepath).await?;
        let mut stream = response.bytes_stream();
        let mut downloaded: u64 = 0;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            file.write(&chunk).await?;
            downloaded += chunk.len() as u64;

            // 更新进度信息
            if total_size > 0 {
                let _progress = (downloaded as f64 / total_size as f64 * 100.0) as u32;
                // 进度信息可以从 info 中读取以在 UI 上显示
            }

            let elapsed = start_time.elapsed().as_secs();
            if elapsed >= 3 {
                let speed_bps = downloaded / elapsed;
                if speed_bps < MIN_SPEED_BPS {
                    return Err(format!("download speed too low: {} B/s < {} B/s", speed_bps, MIN_SPEED_BPS).into());
                }
            }
        }

        let total_elapsed = start_time.elapsed().as_secs().max(1);
        let avg_speed_bps = downloaded / total_elapsed;
        
        if avg_speed_bps < MIN_SPEED_BPS {
            return Err(format!("dpd too low: {} B/s < {} B/s", avg_speed_bps, MIN_SPEED_BPS).into());
        }

        egui_println!("downloaded [{}]: {} (speed: {} B/s, size: {:.3} kb)", 
                 index + 1, filename, avg_speed_bps, downloaded as f64 / 1000.0);
        Ok(())
    }

    fn sanitize_filename(filename: &str) -> String {
        filename
            .chars()
            .map(|c| match c {
                '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
                c => c,
            })
            .collect()
    }

    pub async fn clean_error_files(&self) {
        let download_dir = self.get_download_dir();
        let download_dir = Path::new(&download_dir);

        if !download_dir.exists() {
            return;
        }

        let mut entries = match fs::read_dir(download_dir).await {
            Ok(entries) => entries,
            Err(e) => {
                egui_println!("Failed to read directory {}: {}", download_dir.display(), e);
                return;
            }
        };

        while let Some(entry) = entries.next_entry().await.transpose() {
            let entry = match entry {
                Ok(entry) => entry,
                Err(e) => {
                    egui_println!("Failed to read directory entry: {}", e);
                    continue;
                }
            };
            let path = entry.path();

            if !path.is_file() {
                continue;
            }

            let metadata = match fs::metadata(&path).await {
                Ok(metadata) => metadata,
                Err(e) => {
                    egui_println!("Failed to get metadata for {}: {}", path.display(), e);
                    continue;
                }
            };
            // delete small files
            if metadata.len() < MIN_FILE_SIZE {
                if let Err(e) = fs::remove_file(&path).await {
                    egui_println!("Failed to delete {}: {}", path.display(), e);
                } else {
                    egui_println!("Deleted small file: {}", path.display());
                }
            }
        }
    }

    pub fn get_state(&self) -> BunkrSpiderState {
        self.info.try_read()
            .ok()
            .map(|info| info.state)
            .unwrap_or(BunkrSpiderState::Idle)
    }

    pub fn get_info(&self) -> Option<BunkrSpiderInformation> {
        self.info.try_read().ok().map(|info| BunkrSpiderInformation {
            total_sources: info.total_sources,
            downloaded_sources: info.downloaded_sources,
            state: info.state,
        })
    }

    pub fn stop(&self) {
        self.should_stop.store(true, Ordering::Relaxed);
        egui_println!("Stop signal sent, task will terminate gracefully...");
    }

    pub fn reset(&mut self) {
        self.should_stop.store(false, Ordering::Relaxed);
        if let Ok(mut info) = self.info.try_write() {
            info.state = BunkrSpiderState::Idle;
            info.total_sources = None;
            info.downloaded_sources = None;
        }
        self.sources.clear();
        self.title.clear();
        self.base_dir.clear();
    }
}
