use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
};

use serde::{Deserialize, Serialize};

use crate::ARCHIVE_ROOT;

/// One archive file described by its month's manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogEntry {
    pub folder: String,
    pub file_name: String,
    pub path: String,
    pub work_id: String,
    pub title: String,
    pub circle: String,
    pub version: String,
    /// The update date dlminer recorded for this copy, `YYYY-MM-DD`; empty
    /// for a copy without an update record, such as the original release.
    #[serde(default)]
    pub updated: String,
    /// The work's release date, `YYYY-MM-DD`, or empty.
    #[serde(default)]
    pub released: String,
    pub size: u64,
    pub sha256: Option<String>,
}

impl CatalogEntry {
    /// The date that orders a work's copies: its update, else the release.
    pub fn dated(&self) -> &str {
        if self.updated.is_empty() {
            &self.released
        } else {
            &self.updated
        }
    }
}

#[derive(Deserialize)]
struct Manifest {
    #[serde(default)]
    files: BTreeMap<String, ManifestFile>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ManifestFile {
    work_id: Option<String>,
    title: Option<String>,
    circle: Option<String>,
    version: Option<String>,
    creation_date: Option<String>,
    updated_date: Option<String>,
    size: Option<u64>,
    hashes: Option<Hashes>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Hashes {
    sha256: Option<String>,
}

/// Reads one dlminer `_manifest.json`. Returns `None` when it is not a
/// manifest at all; missing or null fields become empty values.
pub fn parse_manifest(folder: &str, data: &[u8]) -> Option<Vec<CatalogEntry>> {
    let manifest: Manifest = serde_json::from_slice(data).ok()?;
    let text = |value: Option<String>| value.unwrap_or_default().trim().to_owned();
    Some(
        manifest
            .files
            .into_iter()
            .map(|(file_name, file)| CatalogEntry {
                path: format!("{ARCHIVE_ROOT}/{folder}/{file_name}"),
                folder: folder.to_owned(),
                work_id: text(file.work_id),
                title: text(file.title),
                circle: text(file.circle),
                version: text(file.version),
                updated: day(file.updated_date),
                released: day(file.creation_date),
                size: file.size.unwrap_or(0),
                sha256: file
                    .hashes
                    .and_then(|hashes| hashes.sha256)
                    .map(|hash| hash.trim().to_ascii_lowercase())
                    .filter(|hash| dla_remote::is_digest(hash)),
                file_name,
            })
            .collect(),
    )
}

/// The `YYYY-MM-DD` that starts a manifest date, or empty.
fn day(value: Option<String>) -> String {
    let value = value.unwrap_or_default();
    let date = value.trim().get(..10).unwrap_or_default();
    let shape = date.bytes().enumerate().all(|(index, byte)| match index {
        4 | 7 => byte == b'-',
        _ => byte.is_ascii_digit(),
    });
    if date.len() == 10 && shape {
        date.to_owned()
    } else {
        String::new()
    }
}

struct Keys {
    work_id: String,
    title: String,
    circle: String,
    file_name: String,
}

/// A month folder of the archive and how many catalogued files it holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderSummary {
    pub name: String,
    pub files: usize,
    /// False when the folder has no readable manifest yet, so its files
    /// are not catalogued rather than absent.
    pub manifest: bool,
}

/// All manifest entries of the archive, searchable in memory. A full archive
/// holds tens of thousands of entries, which a linear scan answers in
/// milliseconds without a separate search engine.
#[derive(Default)]
pub struct Catalog {
    entries: Vec<CatalogEntry>,
    keys: Vec<Keys>,
    folders: BTreeMap<String, Range<usize>>,
    without_manifest: BTreeSet<String>,
}

impl Catalog {
    /// `folders` lists every archive folder, including ones without a
    /// manifest, so the tree shows the whole archive.
    pub fn new(folders: impl IntoIterator<Item = String>, mut entries: Vec<CatalogEntry>) -> Self {
        entries.sort_by(|left, right| {
            (&left.folder, &left.file_name).cmp(&(&right.folder, &right.file_name))
        });
        entries.dedup_by(|right, left| {
            left.folder == right.folder && left.file_name == right.file_name
        });
        let mut ranges: BTreeMap<String, Range<usize>> =
            folders.into_iter().map(|name| (name, 0..0)).collect();
        let mut start = 0;
        while start < entries.len() {
            let folder = &entries[start].folder;
            let end = start + entries[start..].partition_point(|entry| &entry.folder == folder);
            ranges.insert(folder.clone(), start..end);
            start = end;
        }
        let keys = entries
            .iter()
            .map(|entry| Keys {
                work_id: fold(&entry.work_id),
                title: fold(&entry.title),
                circle: fold(&entry.circle),
                file_name: fold(&entry.file_name),
            })
            .collect();
        Self {
            entries,
            keys,
            folders: ranges,
            without_manifest: BTreeSet::new(),
        }
    }

    /// Marks folders that have no readable manifest, so the tree can say
    /// they are not catalogued yet rather than empty.
    pub fn without_manifest(mut self, folders: impl IntoIterator<Item = String>) -> Self {
        self.without_manifest.extend(folders);
        self
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn folders(&self) -> Vec<FolderSummary> {
        self.folders
            .iter()
            .map(|(name, range)| FolderSummary {
                name: name.clone(),
                files: range.len(),
                manifest: !self.without_manifest.contains(name),
            })
            .collect()
    }

    pub fn has_folder(&self, folder: &str) -> bool {
        self.folders.contains_key(folder)
    }

    /// The catalogued files of one folder, sorted by file name.
    pub fn in_folder(&self, folder: &str) -> &[CatalogEntry] {
        self.folders
            .get(folder)
            .map_or(&[], |range| &self.entries[range.clone()])
    }

    pub fn entry(&self, folder: &str, file_name: &str) -> Option<&CatalogEntry> {
        let entries = self.in_folder(folder);
        entries
            .binary_search_by(|entry| entry.file_name.as_str().cmp(file_name))
            .ok()
            .map(|index| &entries[index])
    }

    /// Ranks matches: an exact work code first, then code prefixes, then
    /// title, circle and file-name matches. Every word must match somewhere.
    pub fn search(&self, query: &str, limit: usize) -> Vec<&CatalogEntry> {
        let query = fold(query);
        let words: Vec<&str> = query.split_whitespace().collect();
        if words.is_empty() {
            return Vec::new();
        }
        let compact: String = words.concat();
        let mut hits: Vec<(u32, &CatalogEntry)> = self
            .entries
            .iter()
            .zip(&self.keys)
            .filter_map(|(entry, keys)| {
                let mut score = 0;
                for word in &words {
                    score += word_score(keys, word)?;
                }
                if keys.work_id == compact {
                    score += 1000;
                }
                Some((score, entry))
            })
            .collect();
        hits.sort_by(|(left_score, left), (right_score, right)| {
            right_score
                .cmp(left_score)
                .then_with(|| right.folder.cmp(&left.folder))
                .then_with(|| left.work_id.cmp(&right.work_id))
                // A work's copies, newest first.
                .then_with(|| right.dated().cmp(left.dated()))
                .then_with(|| left.file_name.cmp(&right.file_name))
        });
        hits.into_iter()
            .take(limit)
            .map(|(_, entry)| entry)
            .collect()
    }
}

fn word_score(keys: &Keys, word: &str) -> Option<u32> {
    if keys.work_id == word {
        Some(100)
    } else if keys.work_id.starts_with(word) {
        Some(80)
    } else if keys.work_id.contains(word) {
        Some(60)
    } else if keys.title.starts_with(word) {
        Some(50)
    } else if keys.title.contains(word) {
        Some(40)
    } else if keys.circle.contains(word) {
        Some(30)
    } else if keys.file_name.contains(word) {
        Some(20)
    } else {
        None
    }
}

/// Lower-cases and maps full-width ASCII (as typed with a Japanese input
/// method, e.g. `ＲＪ１０８`) and the ideographic space to plain ASCII.
fn fold(text: &str) -> String {
    text.chars()
        .map(|character| match character {
            '\u{3000}' => ' ',
            '\u{FF01}'..='\u{FF5E}' => {
                char::from_u32(character as u32 - 0xFEE0).unwrap_or(character)
            }
            other => other,
        })
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = r#"{
      "manifest_info": {"created_at": "2026-09-29"},
      "files": {
        "RJ108375__a.zip": {"work_id": "RJ108375", "title": "Summer Rain Stories", "circle": "Circle Amaoto",
          "version": "1.0", "size": 3145728, "relative_path": "/2013-01",
          "hashes": {"crc": "1", "md5": "2", "sha1": "3", "sha256": "E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855"}},
        "RJ108376__b.zip": {"work_id": "RJ108376", "title": "星降る夜のささやき", "circle": "サークル星空", "size": 10},
        "RJ01083750__c.zip": {"work_id": "RJ01083750", "title": null, "circle": "Circle Amaoto", "hashes": {"sha256": "not-a-hash"}}
      }
    }"#;

    fn catalog() -> Catalog {
        assert!(parse_manifest("2024-05", b"not json").is_none());
        let entries = parse_manifest("2013-01", MANIFEST.as_bytes()).unwrap();
        Catalog::new(["2013-01".to_owned(), "2024-05".to_owned()], entries)
            .without_manifest(["2024-05".to_owned()])
    }

    #[test]
    fn reads_dlminer_manifests_including_nulls() {
        let catalog = catalog();
        assert_eq!(catalog.len(), 3);
        let entry = catalog.entry("2013-01", "RJ108375__a.zip").unwrap();
        assert_eq!(entry.path, "/dlsite/2013-01/RJ108375__a.zip");
        assert_eq!(entry.size, 3_145_728);
        assert_eq!(
            entry.sha256.as_deref(),
            Some("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
        );
        let untitled = catalog.entry("2013-01", "RJ01083750__c.zip").unwrap();
        assert_eq!(
            (untitled.title.as_str(), untitled.sha256.as_deref()),
            ("", None)
        );
        let folders: Vec<_> = catalog
            .folders()
            .into_iter()
            .map(|folder| (folder.name, folder.files, folder.manifest))
            .collect();
        assert_eq!(
            folders,
            [
                ("2013-01".to_owned(), 3, true),
                ("2024-05".to_owned(), 0, false)
            ]
        );
        assert_eq!(catalog.in_folder("2024-05"), []);
        assert!(catalog.entry("2013-01", "missing.zip").is_none());
    }

    #[test]
    fn an_exact_code_ranks_first_then_prefixes() {
        let catalog = catalog();
        let codes: Vec<_> = catalog
            .search("rj108375", 10)
            .into_iter()
            .map(|entry| entry.work_id.as_str())
            .collect();
        assert_eq!(codes, ["RJ108375"]);
        let prefixed: Vec<_> = catalog
            .search("RJ10837", 10)
            .into_iter()
            .map(|entry| entry.work_id.as_str())
            .collect();
        assert_eq!(prefixed, ["RJ108375", "RJ108376"]);
    }

    #[test]
    fn lists_a_works_copies_newest_first_with_their_dates() {
        let manifest = r#"{"files": {
          "RJ01549336__a.zip": {"work_id": "RJ01549336", "creation_date": "2026-04-30", "updated_date": "", "version": "", "size": 1},
          "RJ01549336__c.zip": {"work_id": "RJ01549336", "creation_date": "2026-04-30", "updated_date": "2026-10-03", "version": "1.19", "size": 3},
          "RJ01549336__b.zip": {"work_id": "RJ01549336", "creation_date": "2026-04-30", "updated_date": "2026-05-08T00:00:00", "version": "1.044", "size": 2},
          "RJ01549337__d.zip": {"work_id": "RJ01549337", "creation_date": "not a date", "size": 4}
        }}"#;
        let catalog = Catalog::new(
            ["2026-04".to_owned()],
            parse_manifest("2026-04", manifest.as_bytes()).unwrap(),
        );
        let found: Vec<_> = catalog
            .search("RJ01549336", 10)
            .into_iter()
            .map(|entry| (entry.version.as_str(), entry.updated.as_str()))
            .collect();
        assert_eq!(
            found,
            [("1.19", "2026-10-03"), ("1.044", "2026-05-08"), ("", "")]
        );
        let original = catalog.entry("2026-04", "RJ01549336__a.zip").unwrap();
        assert_eq!(
            (original.released.as_str(), original.dated()),
            ("2026-04-30", "2026-04-30")
        );
        assert_eq!(
            catalog
                .entry("2026-04", "RJ01549337__d.zip")
                .unwrap()
                .dated(),
            ""
        );
    }

    #[test]
    fn finds_titles_circles_and_full_width_input() {
        let catalog = catalog();
        assert_eq!(catalog.search("星降る", 10)[0].work_id, "RJ108376");
        assert_eq!(catalog.search("amaoto", 10).len(), 2);
        assert_eq!(catalog.search("amaoto summer", 10)[0].work_id, "RJ108375");
        assert_eq!(
            catalog.search("ＲＪ１０８３７５", 10)[0].work_id,
            "RJ108375"
        );
        assert!(catalog.search("   ", 10).is_empty());
        assert!(catalog.search("nothing-like-this", 10).is_empty());
    }
}
