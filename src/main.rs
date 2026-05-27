//! Apple Books Exporter - CLI Entry Point

use clap::{Parser, Subcommand};
use std::path::PathBuf;

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

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::List => cmd_list(&cli.config),
        Commands::Export { index, output, format } => cmd_export(&cli.config, index, output, &format),
        Commands::Enrich { book, index: single_index, all, force, output, format } => {
            cmd_enrich(&cli.config, book, single_index, all, force, output, &format)
        }
        Commands::Card { book, index: single_index, all, style, output } => {
            cmd_card(&cli.config, book, single_index, all, &style, output)
        }
        Commands::Cache { book } => cmd_cache(&cli.config, book),
        Commands::Config { base_url, api_key, model, provider } => {
            cmd_config(&cli.config, base_url, api_key, model, provider)
        }
    }
}

fn cmd_list(_config_path: &PathBuf) -> anyhow::Result<()> {
    println!("Apple Books Exporter v0.1.0");
    println!("正在加载书籍列表...");

    // TODO: Implement database connection and book listing
    println!("未实现：需要连接 Apple Books 数据库");

    Ok(())
}

fn cmd_export(_config_path: &PathBuf, index: usize, _output: Option<PathBuf>, _format: &str) -> anyhow::Result<()> {
    println!("导出命令：书籍 #{}", index);
    println!("未实现：需要完整实现导出功能");
    Ok(())
}

fn cmd_enrich(
    _config_path: &PathBuf,
    book: usize,
    single_index: Option<usize>,
    all: bool,
    force: bool,
    _output: Option<PathBuf>,
    _format: &str,
) -> anyhow::Result<()> {
    println!("AI 增强命令：书籍 #{}", book);
    if let Some(idx) = single_index {
        println!("处理单条笔记 #{}", idx);
    }
    if all {
        println!("模式：全量处理");
    }
    if force {
        println!("模式：强制重新生成");
    }
    println!("未实现：需要完整实现 AI 增强功能");
    Ok(())
}

fn cmd_card(
    _config_path: &PathBuf,
    book: usize,
    _single_index: Option<usize>,
    _all: bool,
    style: &str,
    _output: Option<PathBuf>,
) -> anyhow::Result<()> {
    println!("图片卡片命令：书籍 #{}", book);
    println!("样式：{}", style);
    println!("未实现：需要完整实现图片卡片功能");
    Ok(())
}

fn cmd_cache(_config_path: &PathBuf, book: usize) -> anyhow::Result<()> {
    println!("缓存命令：书籍 #{}", book);
    println!("未实现：需要完整实现缓存功能");
    Ok(())
}

fn cmd_config(
    _config_path: &PathBuf,
    base_url: Option<String>,
    api_key: Option<String>,
    model: Option<String>,
    provider: Option<String>,
) -> anyhow::Result<()> {
    println!("配置命令");
    if let Some(url) = base_url {
        println!("Base URL: {}", url);
    }
    if let Some(key) = api_key {
        println!("API Key: {}", key);
    }
    if let Some(m) = model {
        println!("Model: {}", m);
    }
    if let Some(p) = provider {
        println!("Provider: {}", p);
    }
    println!("未实现：需要完整实现配置保存功能");
    Ok(())
}