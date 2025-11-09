use spider::website::Website;
use spider::tokio;
use fake_useragent::UserAgents;
use scraper::{Html, Selector};
use reqwest;
use std::fs;
use std::io::Write;
use futures::stream::{self, StreamExt};

const CONCURRENT_LIMIT: usize = 8; // 同时下载 8 张图片
const DEFAULT_DOWNLOAD_DIR: &str = "no_title";
const LINK_FILTER: &str = "https://i.ibb.co/";
const TITLE_CSS_FILTER: &str = "#album > div.content-width > div:nth-child(3) > h1 > a";

#[tokio::main]
async fn main() {
    println!("输入 imgbb album 链接 > ");
    let mut website_name = String::new();
    std::io::stdin().read_line(&mut website_name).unwrap();
    // 去除换行符和查询参数
    let website_name = website_name.trim().split('?').next().unwrap().to_string();

    println!("正在解析链接...");
    let client = reqwest::Client::new();
    // 标题
    let mut title = String::new(); 

    let mut website: Website = Website::new(&website_name);
    website.with_user_agent(Some(UserAgents::new().random()));
    website.scrape().await;

    println!("正在提取图片链接...");
    let mut img_links: Vec<String> = Vec::new();
    let img_selector = Selector::parse("link").unwrap();
    let title_selector = Selector::parse(TITLE_CSS_FILTER).unwrap();

    for page in website.get_pages().unwrap() {
        let html = page.get_html();
        let document = Html::parse_document(&html);

        // 从href解析图片链接
        for img in document.select(&img_selector) {
            if let Some(src) = img.value().attr("href") {
                if src.starts_with(LINK_FILTER) {
                    img_links.push(src.to_string());
                }
            }
        }
        
        // 解析标题
        if title.is_empty() {
            if let Some(title_elem) = document.select(&title_selector).next() {
                // 去除路径非法字符
                let _t = title_elem
                    .inner_html()
                    .replace("/", "_")
                    .replace("\\", "_")
                    .replace(":", "_")
                ;
                if !_t.is_empty(){
                    println!("专辑标题: {}", _t);
                    title = _t;
                }
            }
        }
    }

    if title.is_empty() {
        println!("专辑标题为空，使用默认标题 {}", DEFAULT_DOWNLOAD_DIR);
        title = DEFAULT_DOWNLOAD_DIR.to_string();
    }
    println!("共找到 {} 张图片。开始下载...", img_links.len());
    fs::create_dir_all(&title).unwrap();

    // 并发下载
    stream::iter(img_links.into_iter().enumerate())
        .map(|(_, link)| {
            let client = client.clone();
            let title = title.clone();
            async move {
                match download_image(&client, title, &link).await {
                    Ok(_) => println!("✅ 下载成功 {}", link),
                    Err(e) => eprintln!("❌ 下载失败 {}: {}", link, e),
                }
            }
        })
        .buffer_unordered(CONCURRENT_LIMIT)
        .collect::<Vec<_>>()
        .await;

    println!("所有图片下载完成。");
}

async fn download_image(client: &reqwest::Client, download_dir: String, url: &str) -> Result<(), Box<dyn std::error::Error>> {
    let resp = client.get(url).send().await?.bytes().await?;
    let filename = format!(
        "{}/{}", 
        download_dir,
        url.rsplit('/').next().unwrap_or("image")
    );
    let mut file = fs::File::create(&filename)?;
    file.write_all(&resp)?;
    Ok(())
}
