//! Apple Books Exporter - Configuration Management

use crate::models::Config;
use anyhow::{Context, Result};
use serde_json;
use std::fs;
use std::path::Path;

/// 默认配置文件路径
const DEFAULT_CONFIG_PATH: &str = "knowledge_config.json";

/// 加载配置
pub fn load_config(path: Option<&Path>) -> Result<Config> {
    let config_path = path.unwrap_or(Path::new(DEFAULT_CONFIG_PATH));

    if !config_path.exists() {
        // 返回默认配置
        return Ok(Config::default());
    }

    let content = fs::read_to_string(config_path)
        .with_context(|| format!("无法读取配置文件: {:?}", config_path))?;

    let config: Config = serde_json::from_str(&content)
        .with_context(|| format!("无法解析配置文件: {:?}", config_path))?;

    Ok(config)
}

/// 保存配置
pub fn save_config(config: &Config, path: Option<&Path>) -> Result<()> {
    let config_path = path.unwrap_or(Path::new(DEFAULT_CONFIG_PATH));

    // 确保目录存在
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("无法创建配置目录: {:?}", parent))?;
    }

    let content = serde_json::to_string_pretty(config)
        .with_context(|| "无法序列化配置")?;

    fs::write(config_path, content)
        .with_context(|| format!("无法写入配置文件: {:?}", config_path))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    

    #[test]
    fn test_load_default_config() {
        let config = load_config(None).unwrap();
        assert_eq!(config.llm.model, "mimo-v2.5-pro");
        assert_eq!(config.output_format, "obsidian");
    }

    #[test]
    fn test_save_and_load_config() {
        let temp_file = NamedTempFile::new().unwrap();
        let config = Config::default();

        save_config(&config, Some(temp_file.path())).unwrap();
        let loaded = load_config(Some(temp_file.path())).unwrap();

        assert_eq!(loaded.llm.model, config.llm.model);
    }
}