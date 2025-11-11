use egui::{Id, TextEdit, Ui};
use std::fmt::{self, Write};

/// 类 print! 的 Egui 打印器
#[derive(Default)]
pub struct EguiPrinter {
    buffer: String,
}

impl EguiPrinter {
    /// 创建新的打印器
    pub fn new() -> Self {
        Self::default()
    }

    /// 模拟 print!：追加内容（不自动换行）
    pub fn print(&mut self, args: fmt::Arguments) {
        let _ = write!(self.buffer, "{}", args);  // 向缓冲区写入格式化内容
    }

    /// 模拟 println!：追加内容（自动换行）
    pub fn println(&mut self, args: fmt::Arguments) {
        let _ = writeln!(self.buffer, "{}", args);
    }

    /// 清空打印缓冲区
    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    /// 在 Egui 中渲染打印文本框
    pub fn show(&mut self, ui: &mut Ui, width: Option<f32>, height: Option<usize>) {
        use egui::ScrollArea;

        let width = width.unwrap_or(300.0);
        let height = (height.unwrap_or(10) as f32) * ui.text_style_height(&egui::TextStyle::Body);
        
        ScrollArea::vertical()
            .max_width(width)
            .max_height(height)
            .auto_shrink([false; 2])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                // 自动滚动和只读设置
                let text_edit = TextEdit::multiline(&mut self.buffer)
                    .font(egui::FontId::proportional(9.0))
                    .id(Id::new("egui_printer_textbox"))
                    .cursor_at_end(true)
                    .interactive(false)  // 设置为只读
                    .lock_focus(true)
                    .desired_width(width);

                ui.add(text_edit);
            });
    }
}

/// Egui 版 print!（不换行）
#[macro_export]
macro_rules! egui_print {
    ($($arg:tt)*) => {
        egui_printer::get_eprinter().lock().unwrap_or_else(|e| e.into_inner()).print(format_args!($($arg)*));
    };
}


/// Egui 版 println!（换行）
#[macro_export]
macro_rules! egui_println {
    ($($arg:tt)*) => {
        egui_printer::get_eprinter().lock().unwrap_or_else(|e| e.into_inner()).println(format_args!($($arg)*));
    };
}

pub static EPRINTER: std::sync::OnceLock<std::sync::Mutex<EguiPrinter>> = std::sync::OnceLock::new();

pub fn get_eprinter() -> &'static std::sync::Mutex<EguiPrinter> {
    // 首次调用自动初始化，后续直接返回实例（OnceLock 保证仅初始化一次）
    EPRINTER.get_or_init(|| std::sync::Mutex::new(EguiPrinter::new()))
}