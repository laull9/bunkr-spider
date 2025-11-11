#![windows_subsystem = "windows"]
use crate::bunkr::BunkrSpider;

mod bunkr;
mod egui_printer;

use eframe::egui;
use std::{sync::Arc};
use tokio::sync::Mutex;
use rfd::{FileDialog};

const FONT_PIXEL: f32 = 1.3;
const BIG_BUTTON_SIZE: egui::Vec2 = egui::Vec2::new(100.0, 25.0);

#[derive(Clone)]
struct AppState {
    spider: Arc<Mutex<BunkrSpider>>,
    spider_info: Arc<tokio::sync::RwLock<bunkr::BunkrSpiderInformation>>,
}

struct GUI {
    state: AppState,
    base_dir: String,
    text_input_url: String,
    checked_delete_errorfile: bool,
    using_english: bool
}


impl eframe::App for GUI {
    // 每帧绘制UI的核心方法
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading(self.ltext(
                "A Bunkr Spider to download imgs and videos with an album url. Author: laull", 
                "Bunkr 爬虫, 可根据专辑批量下载图片和视频 作者: laull"));

            ui.hyperlink_to(self.ltext(
                "Click to Visit My Website(laull.top) For More Details",
                "点击访问个人网站(laull.top) 获取更多信息"
            ), 
            "https://laull.top/article/8bc5fd18-a180-4704-b8fd-c64497615472");

            ui.checkbox(&mut self.using_english, "Use English");
            ui.separator();

            ui.horizontal(|ui|{      
                ui.label(self.ltext("Download Directory:", "下载目录："));
                if ui.button(self.ltext("Select Folder", "选择文件夹")).clicked() {
                    // 打开本地文件夹选择框
                    let selected = FileDialog::new()
                        .set_title(self.ltext("Select Download Folder", "选择下载文件夹"))
                        .pick_folder();

                    self.base_dir = selected.map(
                        |path| path.to_string_lossy().into_owned())
                        .unwrap_or_default();
                }
            });
            ui.text_edit_singleline(&mut self.base_dir);

            ui.label(self.ltext("Bunkr Album Url:", "Bunkr 专辑网址: "));
            ui.text_edit_singleline(&mut self.text_input_url);

            let delete_invalid_label = self.ltext("Delete Invalid Files", "删除无效文件");
            ui.checkbox(&mut self.checked_delete_errorfile, delete_invalid_label);

            // 获取当前状态
            let state = if let Ok(spider_guard) = self.state.spider.try_lock() {
                    spider_guard.get_state()
                } else if let Ok(info) = self.state.spider_info.try_read() {
                    info.state
                } else {
                    bunkr::BunkrSpiderState::Idle
                };
            
            // 显示进度信息
            if let Ok(info) = self.state.spider_info.try_read() {
                if let Some(total) = info.total_sources {
                    if let Some(downloaded) = info.downloaded_sources {
                        let progress_percent = if total > 0 {
                            (downloaded as f32 / total as f32 * 100.0) as u32
                        } else {
                            0
                        };
                        ui.label(format!("{} {}/{} ({}%)", self.ltext("Progress:", "进度:"), downloaded, total, progress_percent));
                    }
                }
            }
            ui.horizontal(|ui|{
            if state == bunkr::BunkrSpiderState::Idle {
                if ui.add_sized(BIG_BUTTON_SIZE, 
                    egui::Button::new(self.ltext("Run", "运行")))
                    .clicked() {
                    let state = self.state.clone();
                    let url = self.text_input_url.clone();
                    let delete_error = self.checked_delete_errorfile;
                    let base_dir = self.base_dir.clone();
                    let spider_info = self.state.spider_info.clone();
                    
                    std::thread::spawn(move || {
                        
                        let rt = tokio::runtime::Runtime::new().unwrap();
                        rt.block_on(async {
                            
                            {
                                let mut lock = state.spider.lock().await;
                                let _info = lock.run(base_dir.clone(), url).await;
                                
                                if let Some(info) = lock.get_info() {
                                    if let Ok(mut shared_info) = spider_info.try_write() {
                                        *shared_info = info;
                                    }
                                }
                            }
                            
                            
                            {
                                let mut lock = state.spider.lock().await;
                                lock.download_all().await.ok();
                                
                                if let Some(info) = lock.get_info() {
                                    if let Ok(mut shared_info) = spider_info.try_write() {
                                        *shared_info = info;
                                    }
                                }
                            }
                            
                            if delete_error {
                                let lock = state.spider.lock().await;
                                lock.clean_error_files().await;
                            }
                        });
                    });
                }
            }
            else if state == bunkr::BunkrSpiderState::Finished {
                if ui.add_sized(BIG_BUTTON_SIZE, 
                    egui::Button::new(self.ltext("New Download", "新下载")))
                    .clicked() {
                    let spider = self.state.spider.clone();
                    tokio::task::spawn(async move {
                        let mut lock = spider.lock().await;
                        lock.reset();
                    });
                }
            }
            else{
                if ui.add_sized(BIG_BUTTON_SIZE, 
                    egui::Button::new(self.ltext("Stop", "停止")))
                    .clicked() {
                    let spider = self.state.spider.clone();
                    tokio::task::spawn(async move {
                        let lock = spider.lock().await;
                        lock.stop();
                    });
                }
            }

            match state {
                bunkr::BunkrSpiderState::Idle => {
                    ui.label(self.ltext("Current State: Idle", "当前状态：空闲"));
                }
                bunkr::BunkrSpiderState::Analyzing => {
                    ui.label(self.ltext("Current State: Analyzing...", "当前状态：分析..."));
                }
                bunkr::BunkrSpiderState::Downloading => {
                    ui.label(self.ltext("Current State: Downloading...", "当前状态：下载中..."));
                }
                bunkr::BunkrSpiderState::Finished => {
                    ui.label(self.ltext("Current State: Finished", "当前状态：已完成"));
                }
            }
            });
            
            ui.label(self.ltext(
                "Log Output:",
                "日志输出："
            ));
            let mut printer =  egui_printer::get_eprinter()
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
            printer.show(ui, Some(400.0), None);
        });
    }
}

impl GUI {
    // 初始化默认状态
    fn new(_ctx: &egui::Context) -> Self {

        let custom_font_data = include_bytes!("../font/LXGWWenKaiLite-Regular.ttf");
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "CustomFont".to_string(),
            egui::FontData::from_owned(custom_font_data.to_vec().into()).into(),
        );

        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "CustomFont".to_string());

        _ctx.set_fonts(fonts);

        _ctx.set_pixels_per_point(FONT_PIXEL);

        let spider_info = Arc::new(tokio::sync::RwLock::new(bunkr::BunkrSpiderInformation {
            total_sources: None,
            downloaded_sources: None,
            state: bunkr::BunkrSpiderState::Idle,
        }));

        let spider = Arc::new(Mutex::new(BunkrSpider::with_info(spider_info.clone())));

        Self {
            state: AppState {
                spider,
                spider_info,
            },
            text_input_url: String::new(),
            checked_delete_errorfile: true,
            base_dir: String::new(),
            using_english: false
        }
    }

    fn ltext<'a>(&self, en: &'a str, zh: &'a str) -> &'a str {
        if self.using_english {
            en
        } else {
            zh
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        window_builder: Some(Box::new(|viewport_builder| {
            viewport_builder
            .with_inner_size(egui::vec2(400.0, 500.0))
        })),
        ..Default::default()
    };

    eframe::run_native(
        "Bunkr Spider @laull",
        options,

        Box::new(|creation_ctx| 
            Ok(Box::new(GUI::new(&creation_ctx.egui_ctx)))),
    )
}