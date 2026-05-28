//! Apple Books Exporter - SQLite Database Access

use crate::models::{Annotation, Book};
use anyhow::{Context, Result};
use rusqlite::Connection;
use std::collections::HashMap;
use std::path::PathBuf;

/// 数据库访问层
pub struct DB {
    annotation_conn: Connection,
    library_conn: Connection,
}

impl DB {
    /// 打开 Apple Books 数据库
    pub fn open_apple_books() -> Result<Self> {
        let annotation_path = find_annotation_db()?;
        let library_path = find_library_db()?;

        let annotation_conn = Connection::open(&annotation_path)
            .with_context(|| format!("无法打开注释数据库：{}", annotation_path.display()))?;
        let library_conn = Connection::open(&library_path)
            .with_context(|| format!("无法打开书籍库数据库：{}", library_path.display()))?;

        Ok(DB {
            annotation_conn,
            library_conn,
        })
    }

    /// 列出所有有笔记的书籍
    pub fn list_books(&self) -> Result<Vec<Book>> {
        // 1. 从注释数据库获取所有唯一的 asset_id 及其笔记数
        let mut stmt = self.annotation_conn.prepare(
            "SELECT ZANNOTATIONASSETID, COUNT(Z_PK) as note_count
            FROM ZAEANNOTATION
            WHERE ZANNOTATIONDELETED = 0
            GROUP BY ZANNOTATIONASSETID
            ORDER BY ZANNOTATIONASSETID",
        )?;

        let asset_counts: Vec<(String, u32)> = stmt
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        // 2. 从书籍库数据库获取书籍信息
        let mut book_map: HashMap<String, Book> = HashMap::new();
        let _stmt = self.library_conn.prepare(
            "SELECT ZASSETID, ZTITLE, ZAUTHOR, ZSORTKEY FROM ZBKLIBRARYASSET WHERE ZASSETID IN (?)",
        )?;

        for (asset_id, note_count) in asset_counts {
            if let Ok(mut book) = self.get_book_info(&asset_id) {
                if let Some(b) = book.as_mut() {
                    b.note_count = note_count;
                    book_map.insert(asset_id, b.clone());
                }
            }
        }

        // 3. 按 ZSORTKEY 排序
        let mut books: Vec<Book> = book_map.into_values().collect();
        books.sort_by(|a, b| a.title.cmp(&b.title));

        Ok(books)
    }

    /// 获取书籍的笔记
    pub fn get_annotations(&self, asset_id: &str) -> Result<Vec<Annotation>> {
        let mut stmt = self.annotation_conn.prepare(
            "SELECT 
                ZANNOTATIONSELECTEDTEXT,
                ZANNOTATIONNOTE,
                ZANNOTATIONLOCATION,
                ZANNOTATIONCREATIONDATE,
                ZANNOTATIONTYPE
            FROM ZAEANNOTATION
            WHERE ZANNOTATIONASSETID = ? AND ZANNOTATIONDELETED = 0
            ORDER BY ZANNOTATIONCREATIONDATE",
        )?;

        let annotations = stmt
            .query_map([asset_id], |row| {
                Ok(Annotation {
                    asset_id: asset_id.to_string(),
                    selected_text: row.get(0)?,
                    note: row.get(1)?,
                    location: row.get(2)?,
                    annotation_type: row.get(4)?,
                    creation_date: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<Annotation>, _>>()?;

        Ok(annotations)
    }

    /// 获取书籍信息
    pub fn get_book_info(&self, asset_id: &str) -> Result<Option<Book>> {
        let mut stmt = self.library_conn.prepare(
            "SELECT ZTITLE, ZAUTHOR FROM ZBKLIBRARYASSET WHERE ZASSETID = ? LIMIT 1",
        )?;
        match stmt.query_row([asset_id], |row| {
            Ok(Book {
                asset_id: asset_id.to_string(),
                title: row.get(0)?,
                author: row.get(1)?,
                note_count: 0,
            })
        }) {
            Ok(book) => Ok(Some(book)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

/// 查找注释数据库
fn find_annotation_db() -> Result<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_default();
    let base_path = format!(
        "{}/Library/Containers/com.apple.iBooksX/Data/Documents/AEAnnotation",
        home
    );
    let base_path = std::path::Path::new(&base_path);

    if !base_path.exists() {
        anyhow::bail!("未找到 Apple Books 注释数据库目录：{}", base_path.display());
    }

    let mut latest = None;
    let mut latest_time = 0i64;

    for entry in std::fs::read_dir(base_path)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().map_or(false, |ext| ext == "sqlite") {
            let metadata = entry.metadata()?;
            let mtime = metadata.modified()?.elapsed()?.as_secs() as i64;
            if mtime > latest_time {
                latest_time = mtime;
                latest = Some(path);
            }
        }
    }

    latest.ok_or_else(|| anyhow::anyhow!("未找到注释数据库文件"))
}

/// 查找书籍库数据库
fn find_library_db() -> Result<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_default();
    let base_path = format!(
        "{}/Library/Containers/com.apple.iBooksX/Data/Documents/BKLibrary",
        home
    );
    let base_path = std::path::Path::new(&base_path);

    if !base_path.exists() {
        anyhow::bail!("未找到 Apple Books 书籍库数据库目录：{}", base_path.display());
    }

    let mut latest = None;
    let mut latest_time = 0i64;

    for entry in std::fs::read_dir(base_path)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().map_or(false, |ext| ext == "sqlite") {
            let metadata = entry.metadata()?;
            let mtime = metadata.modified()?.elapsed()?.as_secs() as i64;
            if mtime > latest_time {
                latest_time = mtime;
                latest = Some(path);
            }
        }
    }

    latest.ok_or_else(|| anyhow::anyhow!("未找到书籍库数据库文件"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_annotation_db() {
        let path = find_annotation_db();
        assert!(path.is_ok());
    }

    #[test]
    fn test_find_library_db() {
        let path = find_library_db();
        assert!(path.is_ok());
    }
}