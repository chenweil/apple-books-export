//! Apple Books Exporter - SQLite Database Access

use crate::models::Book;
use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension};
use std::path::PathBuf;

/// 数据库访问层
pub struct DB {
    conn: Connection,
}

impl DB {
    /// 创建新的数据库连接
    pub fn new(db_path: &PathBuf) -> Result<Self> {
        let conn = Connection::open(db_path)
            .with_context(|| format!("无法打开数据库: {:?}", db_path))?;

        // 设置 PRAGMA 优化
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;

        Ok(Self { conn })
    }

    /// 打开 Apple Books 数据库（自动查找最新文件）
    pub fn open_apple_books() -> Result<Self> {
        let db_path = crate::config::get_bklibrary_db_path()
            .ok_or_else(|| anyhow::anyhow!("未找到 Apple Books 数据库"))?;

        Self::new(&db_path)
    }

    /// 列出所有有笔记的书籍
    pub fn list_books(&self) -> Result<Vec<Book>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT 
                ZASSETID,
                ZTITLE,
                ZAUTHOR,
                COUNT(ZANNOTATION.ZROWID) as note_count
            FROM ZBKLIBRARYASSET ZASSET
            LEFT JOIN ZANNOTATION ZANNOTATION 
                ON ZASSET.ZPK = ZANNOTATION.ZASSET
            WHERE ZANNOTATION.ZANNOTATIONTYPE IS NOT NULL
               OR ZANNOTATION.ZSELECTEDTEXT IS NOT NULL
            GROUP BY ZASSET.ZPK
            ORDER BY note_count DESC
            "
        )?;

        let books = stmt
            .query_map([], |row| {
                Ok(Book {
                    asset_id: row.get(0)?,
                    title: row.get(1)?,
                    author: row.get(2)?,
                    note_count: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<Book>, _>>()?;

        Ok(books)
    }

    /// 获取书籍的笔记列表
    pub fn get_annotations(&self, asset_id: &str) -> Result<Vec<crate::models::Annotation>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT 
                ZANNOTATION.ZASSET,
                ZANNOTATION.ZSELECTEDTEXT,
                ZANNOTATION.ZNOTE,
                ZANNOTATION.ZLOCATION,
                ZANNOTATION.ZANNOTATIONTYPE,
                ZANNOTATION.ZCREATIONDATE
            FROM ZANNOTATION ZANNOTATION
            WHERE ZANNOTATION.ZASSET = ?
            ORDER BY ZANNOTATION.ZCREATIONDATE DESC
            "
        )?;

        let annotations = stmt
            .query_map([asset_id], |row| {
                Ok(crate::models::Annotation {
                    asset_id: row.get(0)?,
                    selected_text: row.get(1)?,
                    note: row.get(2)?,
                    location: row.get(3)?,
                    annotation_type: row.get(4)?,
                    creation_date: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<crate::models::Annotation>, _>>()?;

        Ok(annotations)
    }

    /// 获取书籍信息
    pub fn get_book(&self, asset_id: &str) -> Result<Option<Book>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT ZASSETID, ZTITLE, ZAUTHOR
            FROM ZBKLIBRARYASSET
            WHERE ZASSETID = ?
            "
        )?;

        stmt.query_row([asset_id], |row| {
            Ok(Book {
                asset_id: row.get(0)?,
                title: row.get(1)?,
                author: row.get(2)?,
                note_count: 0, // 需要单独查询
            })
        })
        .optional()
        .context("查询书籍信息失败")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_apple_books() {
        // 跳过测试，因为需要实际的 Apple Books 数据库
        // 在 CI 环境中无法访问
    }
}