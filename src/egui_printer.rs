use egui::{Id, TextEdit, Ui};
use std::fmt::{self, Write};

#[derive(Default)]
pub struct EguiPrinter {
    buffer: String,
}

impl EguiPrinter {

    pub fn new() -> Self {
        Self::default()
    }

    pub fn print(&mut self, args: fmt::Arguments) {
        let _ = write!(self.buffer, "{}", args);
    }

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

#[macro_export]
macro_rules! egui_print {
    ($($arg:tt)*) => {
        egui_printer::get_eprinter().lock().unwrap_or_else(|e| e.into_inner()).print(format_args!($($arg)*))
    };
}


#[macro_export]
macro_rules! egui_println {
    ($($arg:tt)*) => {
        egui_printer::get_eprinter().lock().unwrap_or_else(|e| e.into_inner()).println(format_args!($($arg)*))
    };
}

pub static EPRINTER: std::sync::OnceLock<std::sync::Mutex<EguiPrinter>> = std::sync::OnceLock::new();

pub fn get_eprinter() -> &'static std::sync::Mutex<EguiPrinter> {
    // 首次调用自动初始化，后续直接返回实例（OnceLock 保证仅初始化一次）
    EPRINTER.get_or_init(|| std::sync::Mutex::new(EguiPrinter::new()))
}