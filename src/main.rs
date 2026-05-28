//! Apple Books Exporter - CLI Entry Point

use apple_books_exporter::{
    build_enrich_prompt, generate_card, parse_llm_result, Annotation, CardStyle, DB, ExportFormat,
    LLMCache, LLMProvider, load_config, save_config, sanitize_filename,
};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// 安全截断字符串（按字符数，不是字节数）
fn truncate_str(s: &str, max_chars: usize) -> String {
    if s.chars().count() > max_chars {
        s.chars().take(max_chars - 3).collect::<String>() + "..."
    } else {
        s.to_string()
    }
}

#[derive(Parser)]
#[command(name = "apple-books-exporter")]
#[command(about = "Export Apple Books notes and highlights to Markdown", long_about = None)]
#[command(version = "0.1.0")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// 配置文件路径
    #[arg(long, default_value = "knowledge_config.json")]
    config: PathBuf,
}

#[derive(Subcommand)]
enum Commands {
    /// 列出所有有笔记的书籍
    List,

    /// 导出笔记为 Markdown
    Export {
        /// 书籍序号
        index: usize,

        /// 输出目录
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// 输出格式 (obsidian|markdown)
        #[arg(short, long, default_value = "obsidian")]
        format: String,
    },

    /// AI 增强笔记（调用 LLM）
    Enrich {
        /// 书籍序号
        book: usize,

        /// 处理单条笔记（1-based）
        #[arg(short, long)]
        index: Option<usize>,

        /// 处理整本书所有笔记
        #[arg(long)]
        all: bool,

        /// 强制重新生成（跳过缓存）
        #[arg(long)]
        force: bool,

        /// 输出目录
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// 输出格式 (obsidian|markdown)
        #[arg(short, long, default_value = "obsidian")]
        format: String,
    },

    /// 导出图片卡片
    Card {
        /// 书籍序号
        book: usize,

        /// 处理单条笔记（1-based）
        #[arg(short, long)]
        index: Option<usize>,

        /// 批量导出所有笔记
        #[arg(long)]
        all: bool,

        /// 卡片样式 (dark|light|minimal)
        #[arg(short, long, default_value = "dark")]
        style: String,

        /// 输出目录
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// 查看缓存状态
    Cache {
        /// 书籍序号
        book: usize,
    },

    /// 配置 LLM 和输出选项
    Config {
        /// LLM Base URL
        #[arg(long)]
        base_url: Option<String>,

        /// API Key
        #[arg(long)]
        api_key: Option<String>,

        /// Model name
        #[arg(long)]
        model: Option<String>,

        /// Provider name
        #[arg(long)]
        provider: Option<String>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::List => cmd_list(&cli.config),
        Commands::Export { index, output, format } => cmd_export(&cli.config, index, output, &format),
        Commands::Enrich { book, index: single_index, all, force, output, format } => {
            cmd_enrich(&cli.config, book, single_index, all, force, output, &format).await
        }
        Commands::Card { book, index: single_index, all, style, output } => {
            cmd_card(&cli.config, book, single_index, all, &style, output)
        }
        Commands::Cache { book } => cmd_cache(&cli.config, book),
        Commands::Config { base_url, api_key, model, provider: _ } => {
            cmd_config(&cli.config, base_url, api_key, model)
        }
    }
}

fn cmd_list(_config_path: &PathBuf) -> anyhow::Result<()> {
    println!("Apple Books Exporter v0.1.0");
    println!("正在加载书籍列表...\n");

    let db = DB::open_apple_books()?;
    let books = db.list_books()?;

    if books.is_empty() {
        println!("未找到有笔记的书籍。");
        return Ok(());
    }

    // 打印表头
    println!("{:<5} {:<50} {:<20} {:>8}", "序号", "书名", "作者", "笔记数");
    println!("{}", "─".repeat(90));

    // 打印书籍列表
    for (i, book) in books.iter().enumerate() {
        let title = truncate_str(&book.title, 48);
        let author = truncate_str(&book.author, 20);
        println!("{:<5} {:<50} {:<20} {:>8}", i + 1, title, author, book.note_count);
    }

    println!("\n共 {} 本书", books.len());

    Ok(())
}

fn cmd_export(_config_path: &PathBuf, index: usize, output: Option<PathBuf>, format: &str) -> anyhow::Result<()> {
    println!("导出命令：书籍 #{}", index);

    let db = DB::open_apple_books()?;
    let books = db.list_books()?;

    if index == 0 || index > books.len() {
        anyhow::bail!("无效的书籍序号：{}", index);
    }

    let book = &books[index - 1];
    println!("书籍：{} - {}", book.title, book.author);
    println!("笔记数：{}", book.note_count);

    // 获取笔记
    let annotations = db.get_annotations(&book.asset_id)?;
    println!("实际笔记数：{}", annotations.len());

    // 确定输出目录
    let output_dir = match output {
        Some(p) => p,
        None => {
            let home = std::env::var("HOME").unwrap_or_default();
            PathBuf::from(format!("{}/books-exported", home))
        }
    };
    std::fs::create_dir_all(&output_dir)?;

    // 解析格式
    let export_format = match format.to_lowercase().as_str() {
        "obsidian" => ExportFormat::Obsidian,
        "markdown" => ExportFormat::Markdown,
        _ => ExportFormat::Obsidian,
    };

    // 导出
    let llm_results: Vec<Option<apple_books_exporter::LLMResult>> = vec![None; annotations.len()];
    apple_books_exporter::export_book(&book, &annotations, &llm_results, &output_dir, export_format)?;

    println!("导出完成！");
    println!("输出目录：{:?}", output_dir);

    Ok(())
}

async fn cmd_enrich(
    config_path: &PathBuf,
    book: usize,
    single_index: Option<usize>,
    all: bool,
    force: bool,
    output: Option<PathBuf>,
    format: &str,
) -> anyhow::Result<()> {
    println!("AI 增强命令：书籍 #{}", book);

    // 加载配置
    let config = load_config(Some(config_path.as_path()))?;
    println!("LLM 配置：{} @ {}", config.llm.model, config.llm.base_url);

    // 打开数据库
    let db = DB::open_apple_books()?;
    let books = db.list_books()?;

    if book == 0 || book > books.len() {
        anyhow::bail!("无效的书籍序号：{}", book);
    }

    let book_info = &books[book - 1];
    println!("书籍：{} - {}", book_info.title, book_info.author);
    println!("笔记数：{}", book_info.note_count);

    // 获取笔记
    let annotations = db.get_annotations(&book_info.asset_id)?;
    println!("实际笔记数：{}", annotations.len());

    // 过滤：只处理有选中文字的笔记
    let mut to_process: Vec<(usize, &apple_books_exporter::Annotation)> = Vec::new();
    for (i, ann) in annotations.iter().enumerate() {
        if let Some(text) = &ann.selected_text {
            if !text.trim().is_empty() {
                to_process.push((i, ann));
            }
        }
    }

    if to_process.is_empty() {
        println!("没有可处理的笔记（需要有选中文字）");
        return Ok(());
    }

    // 如果指定了单条笔记，只处理那一条
    if let Some(idx) = single_index {
        if idx > 0 && idx <= to_process.len() {
            to_process = vec![to_process[idx - 1]];
        } else {
            anyhow::bail!("无效的笔记序号：{}", idx);
        }
    }

    // 如果 all 模式，处理所有；否则默认只处理前 5 条
    if !all && single_index.is_none() {
        let limit = 5.min(to_process.len());
        to_process = to_process.into_iter().take(limit).collect();
        println!("默认只处理前 {} 条笔记（使用 --all 处理全部）", limit);
    }

    println!("将处理 {} 条笔记", to_process.len());

    // 初始化 LLM Provider
    let provider = LLMProvider::new(&config.llm);

    // 初始化缓存
    let cache_path = config_path.with_file_name("llm_cache.json");
    let mut cache = LLMCache::new(&cache_path);
    println!("缓存路径：{:?} (共 {} 条)", cache_path, cache.count());

    // 处理每条笔记
    let mut llm_results: Vec<Option<apple_books_exporter::LLMResult>> = vec![None; annotations.len()];
    let mut cached_count = 0;
    let mut processed_count = 0;
    let mut error_count = 0;

    for (idx, ann) in &to_process {
        let ann = *ann;
        let highlight = ann.selected_text.as_ref().unwrap();

        // 检查缓存
        if !force && cache.is_cached(&book_info.asset_id, highlight) {
            if let Some(entry) = cache.get(&book_info.asset_id, highlight) {
                llm_results[*idx] = Some(apple_books_exporter::LLMResult {
                    explanation: entry.explanation.clone(),
                    tags: entry.tags.clone(),
                    question: entry.question.clone(),
                });
                cached_count += 1;
                println!(
                    "[{}/{}] 命中缓存: {}...",
                    idx + 1,
                    to_process.len(),
                    highlight.chars().take(30).collect::<String>()
                );
                continue;
            }
        }

        // 构建提示词
        let prompt = build_enrich_prompt(highlight, ann.note.as_deref());

        println!(
            "[{}/{}] 调用 LLM: {}...",
            idx + 1,
            to_process.len(),
            highlight.chars().take(30).collect::<String>()
        );

        // 调用 LLM
        match provider.complete(&prompt, None).await {
            Ok(response) => {
                match parse_llm_result(&response) {
                    Ok(result) => {
                        let result_clone = result.clone();
                        llm_results[*idx] = Some(result);
                        // 存入缓存
                        let file_name = apple_books_exporter::exporter::sanitize_filename(highlight);
                        cache.put(
                            &book_info.asset_id,
                            highlight,
                            &file_name,
                            &book_info.title,
                            &result_clone.explanation,
                            &result_clone.tags,
                            &result_clone.question,
                        )?;
                        processed_count += 1;
                        println!("  ✓ 成功");
                    }
                    Err(e) => {
                        error_count += 1;
                        println!("  ✗ 解析失败: {}", e);
                    }
                }
            }
            Err(e) => {
                error_count += 1;
                println!("  ✗ LLM 调用失败: {}", e);
            }
        }

        // 短暂延迟避免限流
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }

    println!("\n处理完成：");
    println!("  命中缓存: {}", cached_count);
    println!("  新处理: {}", processed_count);
    println!("  失败: {}", error_count);

    // 确定输出目录
    let output_dir = match output {
        Some(p) => p,
        None => {
            let home = std::env::var("HOME").unwrap_or_default();
            PathBuf::from(format!("{}/books-exported", home))
        }
    };
    std::fs::create_dir_all(&output_dir)?;

    // 解析格式
    let export_format = match format.to_lowercase().as_str() {
        "obsidian" => ExportFormat::Obsidian,
        "markdown" => ExportFormat::Markdown,
        _ => ExportFormat::Obsidian,
    };

    // 导出
    apple_books_exporter::export_book(&book_info, &annotations, &llm_results, &output_dir, export_format)?;

    println!("导出完成！");
    println!("输出目录：{:?}", output_dir);

    Ok(())
}

fn cmd_card(
    config_path: &PathBuf,
    book: usize,
    single_index: Option<usize>,
    all: bool,
    style: &str,
    output: Option<PathBuf>,
) -> anyhow::Result<()> {
    println!("图片卡片命令：书籍 #{}", book);

    // 加载配置
    let _config = load_config(Some(config_path.as_path()))?;

    // 打开数据库
    let db = DB::open_apple_books()?;
    let books = db.list_books()?;

    if book == 0 || book > books.len() {
        anyhow::bail!("无效的书籍序号：{}", book);
    }

    let book_info = &books[book - 1];
    println!("书籍：{} - {}", book_info.title, book_info.author);

    // 获取笔记
    let mut annotations = db.get_annotations(&book_info.asset_id)?;
    // 只保留有文本的标注（高亮和笔记）
    annotations.retain(|a| a.selected_text.is_some());

    if annotations.is_empty() {
        println!("该书籍没有可生成卡片的笔记/高亮");
        return Ok(());
    }

    println!("笔记数：{}", annotations.len());

    // 确定要处理的笔记
    let to_process: Vec<(usize, &Annotation)> = if let Some(idx) = single_index {
        if idx == 0 || idx > annotations.len() {
            anyhow::bail!("无效的笔记序号：{}", idx);
        }
        vec![(idx - 1, &annotations[idx - 1])]
    } else if all {
        annotations.iter().enumerate().collect()
    } else {
        // 默认只处理第一条
        vec![(0, &annotations[0])]
    };

    // 确定输出目录
    let output_dir = match output {
        Some(p) => p,
        None => {
            let home = std::env::var("HOME").unwrap_or_default();
            PathBuf::from(format!("{}/cards", home))
        }
    };
    std::fs::create_dir_all(&output_dir)?;

    // 解析样式
    let card_style = CardStyle::from_str(style);
    println!("样式：{:?}\n", card_style);

    // 加载缓存（获取 LLM 增强结果）
    let cache_path = config_path.with_file_name("llm_cache.json");
    let cache = LLMCache::new(&cache_path);

    // 生成卡片
    let mut success = 0;
    for (idx, ann) in &to_process {
        let highlight = ann.selected_text.as_ref().unwrap();

        // 从缓存获取解释
        let explanation = cache
            .get(&book_info.asset_id, highlight)
            .map(|e| e.explanation.as_str());

        // 生成文件名
        let filename = format!("card_{:02}_{}.png", idx + 1, sanitize_filename(highlight));
        let output_path = output_dir.join(&filename);

        println!("[{}/{}] 生成卡片：{}", idx + 1, to_process.len(), highlight.chars().take(30).collect::<String>());

        match generate_card(highlight, explanation, &book_info.title, card_style, &output_path) {
            Ok(()) => {
                println!("  ✓ 已保存：{:?}", output_path);
                success += 1;
            }
            Err(e) => {
                println!("  ✗ 失败：{}", e);
            }
        }
    }

    println!("\n处理完成：成功 {} 张卡片", success);
    println!("输出目录：{:?}", output_dir);

    Ok(())
}

fn cmd_cache(config_path: &PathBuf, book: usize) -> anyhow::Result<()> {
    println!("缓存命令：书籍 #{}", book);

    // 加载配置
    let _config = load_config(Some(config_path.as_path()))?;

    // 打开数据库获取书籍信息
    let db = DB::open_apple_books()?;
    let books = db.list_books()?;

    if book == 0 || book > books.len() {
        anyhow::bail!("无效的书籍序号：{}", book);
    }

    let book_info = &books[book - 1];
    println!("书籍：{} - {}", book_info.title, book_info.author);

    // 加载缓存
    let cache_path = config_path.with_file_name("llm_cache.json");
    let cache = LLMCache::new(&cache_path);

    // 获取该书的缓存条目
    let entries = cache.get_all_for_book(&book_info.asset_id);

    if entries.is_empty() {
        println!("该书籍暂无缓存条目");
        return Ok(());
    }

    println!("缓存条目数：{}", entries.len());
    println!("\n{:<5} {:<40} {:<30} {}", "序号", "标签", "更新日期", "问题");
    println!("{}", "─".repeat(120));

    for (i, (_key, entry)) in entries.iter().enumerate() {
        let tags = entry.tags.join(", ");
        let tags_display = truncate_str(&tags, 28);
        let question_display = truncate_str(&entry.question, 35);
        println!(
            "{:<5} {:<40} {:<30} {}",
            i + 1,
            tags_display,
            entry.updated,
            question_display
        );
    }

    Ok(())
}

fn cmd_config(
    config_path: &PathBuf,
    base_url: Option<String>,
    model: Option<String>,
    api_key: Option<String>,
) -> anyhow::Result<()> {
    println!("配置命令");

    // 读取现有配置
    let mut config = load_config(Some(config_path.as_path()))?;

    // 更新配置
    if let Some(url) = base_url {
        config.llm.base_url = url;
        println!("Base URL: {}", config.llm.base_url);
    }
    if let Some(m) = model {
        config.llm.model = m;
        println!("Model: {}", config.llm.model);
    }
    if let Some(key) = api_key {
        config.llm.api_key = key;
        println!("API Key: {}", "***".repeat(8));
    }

    // 保存配置
    save_config(&config, Some(config_path.as_path()))?;
    println!("配置已保存！");

    Ok(())
}