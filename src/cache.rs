//! Apple Books Exporter - LLM Result Cache

use crate::models::CacheEntry;
use anyhow::{Context, Result};
use md5;
use regex::Regex;
use serde_json;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// LLM 结果缓存
pub struct LLMCache {
    path: PathBuf,
    data: HashMap<String, CacheEntry>,
}

impl LLMCache {
    /// 创建新的缓存
    pub fn new(path: &Path) -> Self {
        let mut cache = Self {
            path: path.to_path_buf(),
            data: HashMap::new(),
        };
        cache.load();
        cache
    }

    /// 加载缓存文件
    fn load(&mut self) {
        if !self.path.exists() {
            return;
        }

        match fs::read_to_string(&self.path) {
            Ok(content) => {
                if let Ok(data) = serde_json::from_str(&content) {
                    self.data = data;
                }
            }
            Err(_) => {
                self.data = HashMap::new();
            }
        }
    }

    /// 保存缓存文件
    fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("无法创建缓存目录: {:?}", parent))?;
        }

        let content = serde_json::to_string_pretty(&self.data)
            .with_context(|| "无法序列化缓存")?;

        fs::write(&self.path, content)
            .with_context(|| format!("无法写入缓存文件: {:?}", self.path))?;

        Ok(())
    }

    /// 生成缓存 key: {book_id[:8]}_{md5(normalized_highlight)}
    fn make_key(book_id: &str, highlight: &str) -> String {
        let normalized = normalize_text(highlight);
        let md5_hash = md5::compute(normalized.as_bytes());
        format!("{}_{:x}", &book_id[..book_id.len().min(8)], md5_hash)
    }

    /// 获取缓存结果
    pub fn get(&self, book_id: &str, highlight: &str) -> Option<&CacheEntry> {
        let key = Self::make_key(book_id, highlight);
        self.data.get(&key)
    }

    /// 检查是否已缓存
    pub fn is_cached(&self, book_id: &str, highlight: &str) -> bool {
        let key = Self::make_key(book_id, highlight);
        self.data.contains_key(&key)
    }

    /// 存储缓存结果
    pub fn put(
        &mut self,
        book_id: &str,
        highlight: &str,
        file: &str,
        book_name: &str,
        explanation: &str,
        tags: &[String],
        question: &str,
    ) -> Result<()> {
        let key = Self::make_key(book_id, highlight);
        let normalized = normalize_text(highlight);

        let entry = CacheEntry {
            highlight: normalized,
            file: file.to_string(),
            book_id: book_id.to_string(),
            book: book_name.to_string(),
            explanation: explanation.to_string(),
            tags: tags.to_vec(),
            question: question.to_string(),
            updated: chrono::Utc::now().format("%Y-%m-%d").to_string(),
        };

        self.data.insert(key, entry);
        self.save()
    }

    /// 移除缓存条目
    pub fn remove(&mut self, book_id: &str, highlight: &str) -> Result<()> {
        let key = Self::make_key(book_id, highlight);
        if self.data.remove(&key).is_some() {
            self.save()?;
        }
        Ok(())
    }

    /// 获取缓存条目总数
    pub fn count(&self) -> usize {
        self.data.len()
    }

    /// 获取某本书的所有缓存条目
    pub fn get_all_for_book(&self, book_id: &str) -> Vec<(&String, &CacheEntry)> {
        let prefix = &book_id[..book_id.len().min(8)];
        self.data
            .iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .collect()
    }
}

/// 文本标准化：去除首尾空白，合并连续空白为单空格
fn normalize_text(text: &str) -> String {
    let re = Regex::new(r"\s+").unwrap();
    re.replace_all(text.trim(), " ").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_cache_operations() {
        let temp_file = NamedTempFile::new().unwrap();
        let mut cache = LLMCache::new(temp_file.path());

        // 测试存储
        let tags = vec!["tag1".to_string(), "tag2".to_string()];
        cache
            .put(
                "test_book_id",
                "test highlight text",
                "test.md",
                "Test Book",
                "test explanation",
                &tags,
                "test question",
            )
            .unwrap();

        // 测试获取
        assert!(cache.is_cached("test_book_id", "test highlight text"));

        // 测试计数
        assert_eq!(cache.count(), 1);
    }

    #[test]
    fn test_normalize_text() {
        assert_eq!(normalize_text("  hello   world  "), "hello world");
        assert_eq!(normalize_text("a\n\nb"), "a b");
    }
}