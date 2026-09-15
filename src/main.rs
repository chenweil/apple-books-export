//! Apple Books Exporter - CLI Entry Point

use apple_books_exporter::speech::{
    self, machine as speech_machine, CachedVoiceCatalogSource, ProfileDraft, ProfileOutcome,
    SpeechError, SpeechStore, SpeechStoreError, VoiceCatalogError, VoiceCatalogResponse,
    SENSEAUDIO_PROVIDER,
};
use apple_books_exporter::{
    build_enrich_prompt, generate_card, load_config, parse_llm_result, sanitize_filename,
    save_config, Annotation, AnnotationResponse, BookListResponse, CardStyle, DoctorResponse,
    ErrorResponse, ExportFormat, ExportResponse, ExportWriteError, LLMCache, LLMProvider,
    MachineError, DB,
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
#[command(version)]
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
    List {
        /// 以稳定的机器可读 JSON 输出
        #[arg(long)]
        json: bool,
    },

    /// 获取一本书的标注详情
    Annotations {
        /// 书籍稳定标识
        #[arg(long)]
        asset_id: Option<String>,

        /// 以稳定的机器可读 JSON 输出
        #[arg(long)]
        json: bool,
    },

    /// 导出笔记为 Markdown
    Export {
        /// 书籍序号（人类 CLI）
        index: Option<usize>,

        /// 书籍稳定标识（机器 CLI）
        #[arg(long)]
        asset_id: Option<String>,

        /// 以稳定的机器可读 JSON 输出
        #[arg(long)]
        json: bool,

        /// 显式允许覆盖已有文件
        #[arg(long)]
        overwrite: bool,

        /// 输出目录
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// 输出格式 (obsidian|markdown)
        #[arg(short, long, default_value = "obsidian")]
        format: String,
    },

    /// 诊断 binary 和 Apple Books 数据库可用性
    Doctor {
        /// 以稳定的机器可读 JSON 输出
        #[arg(long)]
        json: bool,
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

    /// 语音功能（Voice Profile）
    Speech {
        #[command(subcommand)]
        command: SpeechCommands,
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

#[derive(Subcommand)]
enum SpeechCommands {
    /// 浏览并缓存当前账号可用的 SenseAudio Voice Catalog
    Voices {
        /// 忽略 24 小时缓存并显式刷新
        #[arg(long)]
        refresh: bool,

        /// 以稳定的机器可读 JSON 输出
        #[arg(long)]
        json: bool,
    },

    /// 管理全局 Voice Profile
    Profile {
        #[command(subcommand)]
        command: SpeechProfileCommands,
    },
}

#[derive(Subcommand)]
enum SpeechProfileCommands {
    /// 显示全局 Voice Profile（不联网，不写文件）
    Show {
        /// 以稳定的机器可读 JSON 输出
        #[arg(long)]
        json: bool,
    },

    /// 设置全局 Voice Profile（本地校验，不联网）
    Set {
        /// Speech Provider（首版只支持 senseaudio）
        #[arg(long)]
        provider: Option<String>,

        /// 模型名
        #[arg(long)]
        model: Option<String>,

        /// 具体音色 ID（必填，精确匹配，不做模糊匹配）
        #[arg(long)]
        voice_id: Option<String>,

        /// 情感展示标签（provider 拥有，不进入供应商请求）
        #[arg(long)]
        emotion_label: Option<String>,

        /// 风格展示标签（provider 拥有，不进入供应商请求）
        #[arg(long)]
        style_label: Option<String>,

        /// 语速，0.5-2.0，最多两位小数
        // allow_hyphen_values 让负值和非法前缀进入本地校验，得到稳定结构化错误，
        // 而不是被 clap 当成未知 flag。
        #[arg(long, allow_hyphen_values = true)]
        speed: Option<String>,

        /// 音量，0.01-10.0，最多两位小数
        #[arg(long, allow_hyphen_values = true)]
        volume: Option<String>,

        /// 声调，-12 到 12 的整数
        #[arg(long, allow_hyphen_values = true)]
        pitch: Option<String>,

        /// API Key 环境变量名（只保存名字，不保存密钥）
        #[arg(long)]
        api_key_env: Option<String>,

        /// 以稳定的机器可读 JSON 输出
        #[arg(long)]
        json: bool,
    },

    /// 恢复默认全局 Voice Profile（不联网）
    Reset {
        /// 以稳定的机器可读 JSON 输出
        #[arg(long)]
        json: bool,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::List { json: true } => {
            return finish_machine(cmd_list_json());
        }
        Commands::List { json: false } => cmd_list(false),
        Commands::Annotations {
            asset_id,
            json: true,
        } => {
            return finish_machine(cmd_annotations_json(asset_id.as_deref()));
        }
        Commands::Annotations {
            asset_id,
            json: false,
        } => match asset_id {
            Some(asset_id) => cmd_annotations(&asset_id),
            None => Err(anyhow::anyhow!("请提供 --asset-id")),
        },
        Commands::Export {
            index,
            asset_id,
            json: true,
            overwrite,
            output,
            format,
        } => {
            if index.is_some() {
                return finish_machine(Err(MachineError::invalid_argument(
                    "The JSON export command does not accept a positional book index.",
                )));
            }
            return finish_machine(cmd_export_json(
                asset_id.as_deref(),
                output,
                &format,
                overwrite,
            ));
        }
        Commands::Export {
            index,
            asset_id,
            json: false,
            overwrite,
            output,
            format,
        } => {
            if asset_id.is_some() {
                Err(anyhow::anyhow!("--asset-id 需要与 --json 一起使用"))
            } else if overwrite {
                Err(anyhow::anyhow!("--overwrite 仅用于 JSON 导出入口"))
            } else {
                match index {
                    Some(index) => cmd_export(index, output, &format),
                    None => Err(anyhow::anyhow!("请提供书籍序号")),
                }
            }
        }
        Commands::Doctor { json: true } => {
            return finish_machine(cmd_doctor_json());
        }
        Commands::Doctor { json: false } => cmd_doctor(),
        Commands::Enrich {
            book,
            index: single_index,
            all,
            force,
            output,
            format,
        } => cmd_enrich(&cli.config, book, single_index, all, force, output, &format).await,
        Commands::Card {
            book,
            index: single_index,
            all,
            style,
            output,
        } => cmd_card(&cli.config, book, single_index, all, &style, output),
        Commands::Cache { book } => cmd_cache(&cli.config, book),
        Commands::Speech { command } => cmd_speech(command).await,
        Commands::Config {
            base_url,
            api_key,
            model,
            provider: _,
        } => cmd_config(&cli.config, base_url, api_key, model),
    };

    if let Err(error) = result {
        eprintln!("Error: {error:#}");
        std::process::exit(1);
    }
}

fn finish_machine(result: Result<String, MachineError>) {
    match result {
        Ok(json) => println!("{json}"),
        Err(error) => {
            let response = ErrorResponse::new(error);
            match serde_json::to_string(&response) {
                Ok(json) => eprintln!("{json}"),
                Err(_) => eprintln!(
                    "{{\"schema_version\":1,\"error\":{{\"code\":\"PROTOCOL_SERIALIZATION_FAILED\",\"message\":\"Failed to serialize machine error response.\"}}}}"
                ),
            }
            std::process::exit(1);
        }
    }
}

async fn cmd_speech(command: SpeechCommands) -> anyhow::Result<()> {
    match command {
        SpeechCommands::Voices { refresh, json: true } => {
            finish_machine(speech_voices_json(refresh).await);
            Ok(())
        }
        SpeechCommands::Voices {
            refresh,
            json: false,
        } => cmd_speech_voices(refresh).await,
        SpeechCommands::Profile { command } => match command {
            SpeechProfileCommands::Show { json: true } => {
                finish_machine(speech_profile_show_json());
                Ok(())
            }
            SpeechProfileCommands::Show { json: false } => cmd_speech_profile_show(),
            SpeechProfileCommands::Set {
                provider,
                model,
                voice_id,
                emotion_label,
                style_label,
                speed,
                volume,
                pitch,
                api_key_env,
                json,
            } => {
                let draft = ProfileDraft {
                    provider,
                    model,
                    voice_id,
                    emotion_label,
                    style_label,
                    speed,
                    volume,
                    pitch,
                    api_key_env,
                };
                if json {
                    finish_machine(speech_profile_set_json(draft));
                    Ok(())
                } else {
                    cmd_speech_profile_set(draft)
                }
            }
            SpeechProfileCommands::Reset { json: true } => {
                finish_machine(speech_profile_reset_json());
                Ok(())
            }
            SpeechProfileCommands::Reset { json: false } => cmd_speech_profile_reset(),
        },
    }
}

/// Speech 状态根只由用户主目录推导，不跟随当前工作目录、`--config` 或导出目录。
fn speech_store() -> Result<SpeechStore, SpeechError> {
    let home = apple_books_exporter::home_dir().ok_or(SpeechError::Storage(
        SpeechStoreError::Unavailable {
            path: std::path::PathBuf::from("~"),
            message: "the user home directory is not available".to_string(),
        },
    ))?;
    Ok(SpeechStore::from_home(&home))
}

fn speech_profile_show_json() -> Result<String, MachineError> {
    let store = speech_store().map_err(|error| speech_machine::error_response(&error))?;
    let outcome = speech::show_profile(&store).map_err(|error| speech_machine::error_response(&error))?;
    serialize_profile_outcome(&outcome)
}

fn speech_profile_set_json(draft: ProfileDraft) -> Result<String, MachineError> {
    let store = speech_store().map_err(|error| speech_machine::error_response(&error))?;
    // Profile only reads the cache-backed source; it never refreshes or makes
    // an implicit provider request.
    let source = CachedVoiceCatalogSource::new(store.clone());
    let outcome = speech::set_profile(&store, &draft, &source, chrono::Utc::now())
        .map_err(|error| speech_machine::error_response(&error))?;
    serialize_profile_outcome(&outcome)
}

fn speech_profile_reset_json() -> Result<String, MachineError> {
    let store = speech_store().map_err(|error| speech_machine::error_response(&error))?;
    let outcome = speech::reset_profile(&store).map_err(|error| speech_machine::error_response(&error))?;
    serialize_profile_outcome(&outcome)
}

fn serialize_profile_outcome(outcome: &ProfileOutcome) -> Result<String, MachineError> {
    serde_json::to_string(&outcome.to_machine_response())
        .map_err(|error| MachineError::protocol_serialization_failed(error.to_string()))
}

fn cmd_speech_profile_show() -> anyhow::Result<()> {
    let outcome = speech::show_profile(&speech_store().map_err(speech_error)?).map_err(speech_error)?;
    print_profile_outcome("Voice Profile", &outcome);
    Ok(())
}

fn cmd_speech_profile_set(draft: ProfileDraft) -> anyhow::Result<()> {
    let store = speech_store().map_err(speech_error)?;
    let source = CachedVoiceCatalogSource::new(store.clone());
    let outcome = speech::set_profile(
        &store,
        &draft,
        &source,
        chrono::Utc::now(),
    )
    .map_err(speech_error)?;
    print_profile_outcome("Voice Profile 已保存", &outcome);
    Ok(())
}

async fn speech_voices_json(refresh: bool) -> Result<String, MachineError> {
    let outcome = load_speech_voice_catalog(refresh)
        .await
        .map_err(|error| speech_machine::error_response(&error))?;
    serde_json::to_string(&VoiceCatalogResponse::new(&outcome))
        .map_err(|error| MachineError::protocol_serialization_failed(error.to_string()))
}

async fn cmd_speech_voices(refresh: bool) -> anyhow::Result<()> {
    let outcome = load_speech_voice_catalog(refresh)
        .await
        .map_err(speech_error)?;
    print_voice_catalog(&outcome);
    Ok(())
}

async fn load_speech_voice_catalog(
    refresh: bool,
) -> Result<speech::VoiceCatalogOutcome, SpeechError> {
    let store = speech_store()?;
    let config = store.load_config().map_err(SpeechError::Storage)?;
    let client = speech::SenseAudioClient::from_environment(&config.api_key_env)
        .map_err(|error| SpeechError::VoiceCatalog(VoiceCatalogError::Provider(error)))?;
    let outcome = speech::load_or_refresh_voice_catalog(
        &store,
        SENSEAUDIO_PROVIDER,
        chrono::Utc::now(),
        refresh,
        || client.fetch_catalog(),
    )
    .await
    .map_err(SpeechError::VoiceCatalog)?;
    Ok(outcome)
}

fn cmd_speech_profile_reset() -> anyhow::Result<()> {
    let outcome =
        speech::reset_profile(&speech_store().map_err(speech_error)?).map_err(speech_error)?;
    print_profile_outcome("Voice Profile 已恢复默认值", &outcome);
    Ok(())
}

fn speech_error(error: SpeechError) -> anyhow::Error {
    match error {
        SpeechError::Storage(SpeechStoreError::Unavailable { path, message }) => anyhow::anyhow!(
            "Speech 状态目录不可用（{}）：{message}",
            path.display()
        ),
        SpeechError::Storage(SpeechStoreError::InvalidConfig(profile_error)) => {
            anyhow::anyhow!("语音配置无效：{}", profile_error.message())
        }
        SpeechError::Storage(SpeechStoreError::UnsupportedSchemaVersion(version)) => {
            anyhow::anyhow!("不支持的语音配置 schema 版本：{version}")
        }
        SpeechError::Profile(profile_error) => anyhow::anyhow!("{}", profile_error.message()),
        SpeechError::VoiceCatalog(error) => anyhow::anyhow!("{error}"),
        SpeechError::VoiceUnavailable { provider, voice_id } => anyhow::anyhow!(
            "voice '{voice_id}' 对 Speech Provider '{provider}' 不可用；请先刷新 Voice Catalog 后重新选择可用音色"
        ),
    }
}

/// Human catalog output groups provider entries by their returned display name
/// while retaining every concrete ID and provider-owned label.
fn print_voice_catalog(outcome: &speech::VoiceCatalogOutcome) {
    use std::collections::BTreeMap;

    let catalog = &outcome.catalog;
    println!(
        "Voice Catalog ({}) — fetched at {}{}",
        catalog.provider,
        catalog
            .fetched_at
            .to_rfc3339(),
        if outcome.stale { " [STALE]" } else { "" }
    );

    let mut groups: BTreeMap<&str, Vec<&speech::CatalogVoice>> = BTreeMap::new();
    for voice in &catalog.voices {
        groups
            .entry(voice.voice_name.as_str())
            .or_default()
            .push(voice);
    }
    for (voice_name, voices) in groups {
        println!("{voice_name}");
        for voice in voices {
            println!("  - [{}] {}", voice.source_type.as_str(), voice.voice_id);
            if let Some(label) = voice.emotion_label.as_deref() {
                println!("      Emotion: {label}");
            }
            if let Some(label) = voice.style_label.as_deref() {
                println!("      Style: {label}");
            }
            if !voice.description.is_empty() {
                println!("      Provider labels: {}", voice.description.join(", "));
            }
        }
    }
    for warning in &outcome.warnings {
        println!("Warning [{}]: {}", warning.reason, warning.message);
    }
}

/// 人类可读的 Profile 输出。只显示环境变量**名**，永远不显示密钥值。
fn print_profile_outcome(verb: &str, outcome: &ProfileOutcome) {
    let profile = &outcome.config.profile;
    println!("{verb}");
    println!("  Provider: {}", profile.provider);
    println!("  Model: {}", profile.model);
    println!("  Voice ID: {}", profile.voice_id);
    println!(
        "  Emotion: {}",
        profile.emotion_label.as_deref().unwrap_or("-")
    );
    println!("  Style: {}", profile.style_label.as_deref().unwrap_or("-"));
    println!("  Speed: {}", profile.speed);
    println!("  Volume: {}", profile.volume);
    println!("  Pitch: {}", profile.pitch);
    println!(
        "  Audio: {} {}Hz {}bps {}ch",
        profile.audio.format, profile.audio.sample_rate, profile.audio.bitrate, profile.audio.channel
    );
    println!("  Verification: {}", profile.verification.status.as_str());
    if let Some(verified_at) = profile.verification.verified_at.as_deref() {
        println!("  Verified at: {verified_at}");
    }
    println!(
        "  API Key: 来自环境变量 {}（不保存密钥）",
        outcome.config.api_key_env
    );
    println!("  Config: {}", outcome.config_path.display());
    for warning in &outcome.warnings {
        println!("  Warning [{}]: {}", warning.reason, warning.message);
    }
}

fn cmd_list_json() -> Result<String, MachineError> {
    let db = DB::open_apple_books().map_err(MachineError::from_database_error)?;
    let books = db
        .list_books()
        .map_err(|error| MachineError::database_unreadable(error.to_string()))?;
    serialize_book_list(&books)
        .map_err(|error| MachineError::protocol_serialization_failed(error.to_string()))
}

fn cmd_list(json: bool) -> anyhow::Result<()> {
    let db = DB::open_apple_books()?;
    let books = db.list_books()?;

    if json {
        println!("{}", serialize_book_list(&books)?);
        return Ok(());
    }

    println!("Apple Books Exporter v{}", env!("CARGO_PKG_VERSION"));
    println!("正在加载书籍列表...\n");

    if books.is_empty() {
        println!("未找到有笔记的书籍。");
        return Ok(());
    }

    // 打印表头
    println!(
        "{:<5} {:<50} {:<20} {:>8}",
        "序号", "书名", "作者", "笔记数"
    );
    println!("{}", "─".repeat(90));

    // 打印书籍列表
    for (i, book) in books.iter().enumerate() {
        let title = truncate_str(&book.title, 48);
        let author = truncate_str(&book.author, 20);
        println!(
            "{:<5} {:<50} {:<20} {:>8}",
            i + 1,
            title,
            author,
            book.note_count
        );
    }

    println!("\n共 {} 本书", books.len());

    Ok(())
}

fn serialize_book_list(books: &[apple_books_exporter::Book]) -> serde_json::Result<String> {
    serde_json::to_string(&BookListResponse::new(books))
}

fn cmd_annotations_json(asset_id: Option<&str>) -> Result<String, MachineError> {
    let asset_id = asset_id.ok_or_else(MachineError::missing_asset_id)?;
    let db = DB::open_apple_books().map_err(MachineError::from_database_error)?;
    let book = db
        .get_book_info(asset_id)
        .map_err(|error| MachineError::database_unreadable(error.to_string()))?
        .ok_or_else(|| MachineError::invalid_asset_id(asset_id))?;
    let annotations = db
        .get_annotations(asset_id)
        .map_err(|error| MachineError::database_unreadable(error.to_string()))?;
    serde_json::to_string(&AnnotationResponse::new(&book, &annotations))
        .map_err(|error| MachineError::protocol_serialization_failed(error.to_string()))
}

fn cmd_annotations(asset_id: &str) -> anyhow::Result<()> {
    let db = DB::open_apple_books()?;
    let book = db
        .get_book_info(asset_id)?
        .ok_or_else(|| anyhow::anyhow!("无效的 asset_id：{asset_id}"))?;
    let annotations = db.get_annotations(asset_id)?;

    println!("{} - {}", book.title, book.author);
    for annotation in annotations {
        if let Some(text) = annotation.selected_text {
            println!("> {text}");
        }
        if let Some(note) = annotation.note {
            println!("笔记：{note}");
        }
        println!();
    }
    Ok(())
}

fn cmd_export_json(
    asset_id: Option<&str>,
    output: Option<PathBuf>,
    format: &str,
    overwrite: bool,
) -> Result<String, MachineError> {
    let asset_id = asset_id.ok_or_else(MachineError::missing_asset_id)?;
    let db = DB::open_apple_books().map_err(MachineError::from_database_error)?;
    let mut book = db
        .get_book_info(asset_id)
        .map_err(|error| MachineError::database_unreadable(error.to_string()))?
        .ok_or_else(|| MachineError::invalid_asset_id(asset_id))?;
    let annotations = db
        .get_annotations(asset_id)
        .map_err(|error| MachineError::database_unreadable(error.to_string()))?;
    book.note_count = annotations.len() as u32;
    let output_dir = output.unwrap_or_else(|| {
        apple_books_exporter::home_dir()
            .unwrap_or_default()
            .join("books-exported")
    });
    let export_format = ExportFormat::from(format);
    let llm_results = vec![None; annotations.len()];
    let generated_files = apple_books_exporter::export_book_checked(
        &book,
        &annotations,
        &llm_results,
        &output_dir,
        export_format,
        overwrite,
    )
    .map_err(|error| match error {
        ExportWriteError::OutputFileExists(path) => MachineError::output_file_exists(&path),
        ExportWriteError::Other(error) => MachineError::output_unwritable(error.to_string()),
    })?;
    serde_json::to_string(&ExportResponse::new(
        &book,
        annotations.len(),
        export_format,
        &output_dir,
        &generated_files,
    ))
    .map_err(|error| MachineError::protocol_serialization_failed(error.to_string()))
}

fn cmd_export(index: usize, output: Option<PathBuf>, format: &str) -> anyhow::Result<()> {
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
    apple_books_exporter::export_book(
        &book,
        &annotations,
        &llm_results,
        &output_dir,
        export_format,
    )?;

    println!("导出完成！");
    println!("输出目录：{:?}", output_dir);

    Ok(())
}

fn cmd_doctor_json() -> Result<String, MachineError> {
    if std::env::consts::OS != "macos" || !matches!(std::env::consts::ARCH, "aarch64" | "x86_64") {
        return Err(MachineError::binary_incompatible());
    }
    let db = DB::open_apple_books().map_err(MachineError::from_database_error)?;
    db.validate()
        .map_err(|error| MachineError::database_unreadable(error.to_string()))?;
    let (annotation_path, library_path) = db.paths();
    serde_json::to_string(&DoctorResponse::ready(&annotation_path, &library_path))
        .map_err(|error| MachineError::protocol_serialization_failed(error.to_string()))
}

fn cmd_doctor() -> anyhow::Result<()> {
    let db = DB::open_apple_books()?;
    db.validate()?;
    let (annotation_path, library_path) = db.paths();
    println!(
        "Binary: {} {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    println!("Annotation database: {}", annotation_path.display());
    println!("Library database: {}", library_path.display());
    println!("Status: ok");
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
    let mut llm_results: Vec<Option<apple_books_exporter::LLMResult>> =
        vec![None; annotations.len()];
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
                        let file_name = sanitize_filename(highlight);
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
    apple_books_exporter::export_book(
        &book_info,
        &annotations,
        &llm_results,
        &output_dir,
        export_format,
    )?;

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

        println!(
            "[{}/{}] 生成卡片：{}",
            idx + 1,
            to_process.len(),
            highlight.chars().take(30).collect::<String>()
        );

        match generate_card(
            highlight,
            explanation,
            &book_info.title,
            card_style,
            &output_path,
        ) {
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
    println!(
        "\n{:<5} {:<40} {:<30} {}",
        "序号", "标签", "更新日期", "问题"
    );
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
    }
    if let Some(m) = model {
        config.llm.model = m;
    }
    if let Some(key) = api_key {
        config.llm.api_key = key;
    }

    // 保存配置
    save_config(&config, Some(config_path.as_path()))?;
    println!("配置已保存！\n");

    // 显示当前配置（api_key 隐藏显示）
    println!("当前配置：");
    println!("  Provider: {}", config.llm.provider);
    println!("  Base URL: {}", config.llm.base_url);
    println!("  Model: {}", config.llm.model);
    let api_key_display = if config.llm.api_key.is_empty() {
        "(未设置)".to_string()
    } else if config.llm.api_key == "***" {
        "(已设置，仅在命令行显示 *** 隐藏)".to_string()
    } else {
        format!("{}", "***".repeat(4))
    };
    println!("  API Key: {}", api_key_display);
    println!("  Batch Size: {}", config.llm.batch_size);
    println!("  Max Retries: {}", config.llm.max_retries);
    println!("  Output Format: {}", config.output_format);
    println!("  Card Style: {}", config.card_style);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use apple_books_exporter::Book;

    #[test]
    fn serializes_versioned_book_list_json() {
        let books = vec![Book {
            asset_id: "book-1".to_string(),
            title: "纳瓦尔宝典".to_string(),
            author: "Eric Jorgenson".to_string(),
            note_count: 218,
        }];

        let output = serialize_book_list(&books).expect("book list should serialize");
        let value: serde_json::Value =
            serde_json::from_str(&output).expect("book list should be valid JSON");

        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["books"][0]["asset_id"], "book-1");
        assert_eq!(value["books"][0]["title"], "纳瓦尔宝典");
        assert_eq!(value["books"][0]["author"], "Eric Jorgenson");
        assert_eq!(value["books"][0]["note_count"], 218);
    }
}
