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
        let asset_counts = count_annotations_by_asset(&self.annotation_conn)?;

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
        fetch_annotations(&self.annotation_conn, asset_id)
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
    let home = crate::utils::home_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
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
    let home = crate::utils::home_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
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

/// Apple Books 里存在大量空壳标注:`ZANNOTATIONSELECTEDTEXT` 与 `ZANNOTATIONNOTE`
/// 都为空,只剩一个位置。导出流程本来就会跳过它们,所以在数据层就滤掉,
/// 让列表计数、`get_annotations().len()` 和实际导出条数三者一致 ——
/// 否则会出现列表说 302、导出提示 307 的矛盾。
///
/// 不要改用 `ZANNOTATIONTYPE` 判断:该字段与内容并不对应,
/// 实测 type 1 / type 3 的行几乎都没有任何文本。
const HAS_CONTENT: &str =
    "(COALESCE(ZANNOTATIONNOTE, '') <> '' OR COALESCE(ZANNOTATIONSELECTEDTEXT, '') <> '')";

/// 统计每本书「有内容」的标注数。
fn count_annotations_by_asset(conn: &Connection) -> Result<Vec<(String, u32)>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT ZANNOTATIONASSETID, COUNT(Z_PK) as note_count
        FROM ZAEANNOTATION
        WHERE ZANNOTATIONDELETED = 0 AND {HAS_CONTENT}
        GROUP BY ZANNOTATIONASSETID
        ORDER BY ZANNOTATIONASSETID"
    ))?;

    let counts = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(counts)
}

/// 读取一本书的标注,口径与 [`count_annotations_by_asset`] 一致。
fn fetch_annotations(conn: &Connection, asset_id: &str) -> Result<Vec<Annotation>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT
            ZANNOTATIONSELECTEDTEXT,
            ZANNOTATIONNOTE,
            ZANNOTATIONLOCATION,
            ZANNOTATIONCREATIONDATE,
            ZANNOTATIONTYPE
        FROM ZAEANNOTATION
        WHERE ZANNOTATIONASSETID = ? AND ZANNOTATIONDELETED = 0 AND {HAS_CONTENT}
        ORDER BY ZANNOTATIONCREATIONDATE"
    ))?;

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

#[cfg(test)]
mod tests {
    use super::*;

    /// 建一个只含本测试关心字段的 ZAEANNOTATION。
    fn fixture_db(rows: &[(&str, Option<&str>, Option<&str>, i32)]) -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE ZAEANNOTATION (
                Z_PK INTEGER PRIMARY KEY AUTOINCREMENT,
                ZANNOTATIONASSETID TEXT,
                ZANNOTATIONSELECTEDTEXT TEXT,
                ZANNOTATIONNOTE TEXT,
                ZANNOTATIONLOCATION TEXT,
                ZANNOTATIONCREATIONDATE REAL,
                ZANNOTATIONTYPE INTEGER,
                ZANNOTATIONDELETED INTEGER
            );",
        )
        .unwrap();
        for (asset, text, note, deleted) in rows {
            conn.execute(
                "INSERT INTO ZAEANNOTATION
                 (ZANNOTATIONASSETID, ZANNOTATIONSELECTEDTEXT, ZANNOTATIONNOTE,
                  ZANNOTATIONLOCATION, ZANNOTATIONCREATIONDATE, ZANNOTATIONTYPE,
                  ZANNOTATIONDELETED)
                 VALUES (?1, ?2, ?3, 'epubcfi(/6/2)', 0.0, 2, ?4)",
                rusqlite::params![asset, text, note, deleted],
            )
            .unwrap();
        }
        conn
    }

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

    /// 列表里的数字必须跟真正能导出的条数一致。既无正文也无批注的空壳标注
    /// 会被导出流程跳过(见 exporter::tests::test_main_note_skips_location_only_annotations),
    /// 所以也不能计入 note_count,否则列表说 307、导出只有 302。
    #[test]
    fn count_excludes_annotations_without_text_or_note() {
        let conn = fixture_db(&[
            ("book-a", Some("划线的原文"), None, 0),
            ("book-a", Some("原文"), Some("我的批注"), 0),
            ("book-a", None, Some("只有批注"), 0),
            ("book-a", None, None, 0),
            ("book-a", Some(""), Some(""), 0),
        ]);

        let counts = count_annotations_by_asset(&conn).unwrap();

        assert_eq!(counts, vec![("book-a".to_string(), 3)]);
    }

    #[test]
    fn count_excludes_deleted_annotations() {
        let conn = fixture_db(&[
            ("book-a", Some("留下的"), None, 0),
            ("book-a", Some("删掉的"), None, 1),
        ]);

        let counts = count_annotations_by_asset(&conn).unwrap();

        assert_eq!(counts, vec![("book-a".to_string(), 1)]);
    }

    /// 一本书如果只剩空壳标注,就不该出现在「有笔记的书籍」列表里。
    #[test]
    fn count_omits_books_whose_annotations_are_all_empty() {
        let conn = fixture_db(&[
            ("book-empty", None, None, 0),
            ("book-real", Some("有内容"), None, 0),
        ]);

        let counts = count_annotations_by_asset(&conn).unwrap();

        assert_eq!(counts, vec![("book-real".to_string(), 1)]);
    }

    #[test]
    fn fetch_skips_annotations_without_text_or_note() {
        let conn = fixture_db(&[
            ("book-a", Some("划线的原文"), None, 0),
            ("book-a", None, None, 0),
            ("book-a", None, Some("只有批注"), 0),
        ]);

        let annotations = fetch_annotations(&conn, "book-a").unwrap();

        assert_eq!(annotations.len(), 2);
        assert!(annotations
            .iter()
            .all(|a| a.selected_text.is_some() || a.note.is_some()));
    }

    /// 这是当初出问题的地方:列表数字来自 count,导出提示的数字来自
    /// get_annotations().len(),两条路径口径不同就会一个说 302、一个说 307。
    #[test]
    fn count_and_fetch_agree_on_the_same_data() {
        let conn = fixture_db(&[
            ("book-a", Some("高亮一"), None, 0),
            ("book-a", Some("高亮二"), Some("批注"), 0),
            ("book-a", None, None, 0),
            ("book-a", Some(""), None, 0),
            ("book-a", Some("被删的"), None, 1),
        ]);

        let counted = count_annotations_by_asset(&conn)
            .unwrap()
            .into_iter()
            .find(|(id, _)| id == "book-a")
            .map(|(_, n)| n as usize)
            .unwrap();
        let fetched = fetch_annotations(&conn, "book-a").unwrap().len();

        assert_eq!(counted, fetched, "列表计数与实际取到的条数必须一致");
        assert_eq!(counted, 2);
    }
}