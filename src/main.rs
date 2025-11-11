use crate::bunkr::BunkrSpider;

mod bunkr;
mod egui_printer;

use eframe::egui;
use std::{sync::Arc};
use tokio::sync::Mutex;

const FONT_PIXEL: f32 = 1.3;

// 定义应用状态（存储UI交互的数据）
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
}

// 实现 eframe 的 App trait（核心入口）
impl eframe::App for GUI {
    // 每帧绘制UI的核心方法
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Bunkr Spider @laull");
            if ui.link("My Website: laull.top").clicked() {
                
            }
            ui.separator();
            if ui.button("test").clicked(){
                egui_println!("测试打印输出行 1");
                egui_print!("测试打印输出行 2，无换行... ");
                egui_println!("继续输出行 2，有换行");
            }

            ui.label("下载目录：");
            ui.text_edit_singleline(&mut self.base_dir);
            
            ui.label("Url: ");
            ui.text_edit_singleline(&mut self.text_input_url);

            ui.checkbox(&mut self.checked_delete_errorfile, "删除无效文件");

            // 获取当前状态
            let state = self.state.spider.try_lock()
                .map(|spider| spider.get_state())
                .unwrap_or(bunkr::BunkrSpiderState::Idle);
            
            // 显示进度信息
            if let Ok(info) = self.state.spider_info.try_read() {
                if let Some(total) = info.total_sources {
                    if let Some(downloaded) = info.downloaded_sources {
                        let progress_percent = if total > 0 {
                            (downloaded as f32 / total as f32 * 100.0) as u32
                        } else {
                            0
                        };
                        ui.label(format!("进度: {}/{} ({}%)", downloaded, total, progress_percent));
                    }
                }
            }
            
            match state {
                bunkr::BunkrSpiderState::Idle => {
                    ui.label("当前状态：空闲");
                }
                bunkr::BunkrSpiderState::Analyzing => {
                    ui.label("当前状态：分析...");
                }
                bunkr::BunkrSpiderState::Downloading => {
                    ui.label("当前状态：下载中...");
                }
                bunkr::BunkrSpiderState::Finished => {
                    ui.label("当前状态：已完成");
                }
            }

            if state == bunkr::BunkrSpiderState::Idle {
                if ui.button("运行").clicked() {
                    let state = self.state.clone();
                    let url = self.text_input_url.clone();
                    let delete_error = self.checked_delete_errorfile;
                    let base_dir = self.base_dir.clone();
                    let spider_info = self.state.spider_info.clone();
                    // 用 spawn_blocking 规避 Send 限制
                    tokio::task::spawn_blocking(move || {
                        let rt = tokio::runtime::Runtime::new().unwrap();
                        rt.block_on(async move {
                            let mut lock = state.spider.lock().await;
                            let _info = lock.run(base_dir, url).await;
                            // 同步 spider 内部的 info 到共享的 spider_info
                            if let Some(info) = lock.get_info() {
                                if let Ok(mut shared_info) = spider_info.try_write() {
                                    *shared_info = info;
                                }
                            }
                            lock.download_all().await.ok();
                            // 再次同步以更新最终状态
                            if let Some(info) = lock.get_info() {
                                if let Ok(mut shared_info) = spider_info.try_write() {
                                    *shared_info = info;
                                }
                            }
                            if delete_error {
                                lock.clean_error_files().await;
                            }
                        });
                    });
                }
            }
            else if state == bunkr::BunkrSpiderState::Finished {
                if ui.button("新下载").clicked() {
                    let spider = self.state.spider.clone();
                    tokio::task::spawn_blocking(move || {
                        let rt = tokio::runtime::Runtime::new().unwrap();
                        rt.block_on(async move {
                            let mut lock = spider.lock().await;
                            lock.reset();
                        });
                    });
                }
            }
            else{
                if ui.button("停止").clicked() {
                    let spider = self.state.spider.clone();
                    tokio::task::spawn_blocking(move || {
                        let rt = tokio::runtime::Runtime::new().unwrap();
                        rt.block_on(async move {
                            let lock = spider.lock().await;
                            lock.stop();
                        });
                    });
                }
            }

            ui.label("打印输出：");
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

        let custom_font_data = include_bytes!("../font/LXGWWenKaiLite-Regular3500+.ttf");
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

        Self {
            state: AppState {
                spider: Arc::new(Mutex::new(BunkrSpider::new())),
                spider_info: Arc::new(tokio::sync::RwLock::new(bunkr::BunkrSpiderInformation {
                    total_sources: None,
                    downloaded_sources: None,
                    state: bunkr::BunkrSpiderState::Idle,
                })),
            },
            text_input_url: String::new(),
            checked_delete_errorfile: true,
            base_dir: String::new(),
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
        // 从 CreationContext 中提取 egui::Context
        Box::new(|creation_ctx| 
            Ok(Box::new(GUI::new(&creation_ctx.egui_ctx)))),
    )
}