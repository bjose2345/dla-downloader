use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    ops::Range,
};

use serde::{Deserialize, Serialize};

use crate::{
    ARCHIVE_ROOT,
    arrivals::{month_of, month_range, server_time_seconds},
};

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
    /// When dlminer added the file to the archive (RFC 3339, UTC), or empty
    /// in manifests written before it recorded that.
    #[serde(default)]
    pub added: String,
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
    added_date: Option<String>,
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
                added: text(file.added_date),
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

/// A file found by when dlminer added it to the archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Recent<'a> {
    pub entry: &'a CatalogEntry,
    /// The archive already had another copy of this work, so this one is a
    /// newer version rather than a new work.
    pub update: bool,
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
    /// Entries with a date added, as `(time, index)`, newest first.
    arrivals: Vec<(i64, usize)>,
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
        let mut arrivals: Vec<(i64, usize)> = entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| Some((server_time_seconds(&entry.added)?, index)))
            .collect();
        arrivals.sort_by(|(left_time, left), (right_time, right)| {
            right_time.cmp(left_time).then_with(|| left.cmp(right))
        });
        Self {
            entries,
            keys,
            folders: ranges,
            without_manifest: BTreeSet::new(),
            arrivals,
        }
    }

    /// Files added at or after `since` (seconds since 1970), newest first.
    pub fn arrived_since(&self, since: i64) -> Vec<Recent<'_>> {
        self.arrived_between(since, i64::MAX)
    }

    /// How many files were added in each month, as `YYYY-MM` folders of a
    /// tree.
    /// `offset` (seconds east of UTC) puts the month boundaries at the
    /// viewer's midnight.
    pub fn added_months(&self, offset: i64) -> Vec<FolderSummary> {
        let mut months: BTreeMap<String, usize> = BTreeMap::new();
        for (time, _) in &self.arrivals {
            *months.entry(month_of(time + offset)).or_default() += 1;
        }
        months
            .into_iter()
            .map(|(name, files)| FolderSummary {
                name,
                files,
                manifest: true,
            })
            .collect()
    }

    /// Files added in a `YYYY-MM` month (see [`Self::added_months`]), newest
    /// first.
    pub fn added_in(&self, month: &str, offset: i64) -> Vec<Recent<'_>> {
        month_range(month).map_or_else(Vec::new, |(start, end)| {
            self.arrived_between(start - offset, end - offset)
        })
    }

    /// Files added from `start` up to, not including, `end`, newest first.
    fn arrived_between(&self, start: i64, end: i64) -> Vec<Recent<'_>> {
        let found: Vec<(i64, &CatalogEntry)> = self
            .arrivals
            .iter()
            .skip_while(|(time, _)| *time >= end)
            .take_while(|(time, _)| *time >= start)
            .map(|(time, index)| (*time, &self.entries[*index]))
            .collect();
        let works: HashSet<&str> = found
            .iter()
            .map(|(_, entry)| entry.work_id.as_str())
            .filter(|work| !work.is_empty())
            .collect();
        // When the first part of each copy of those works was added; `None`
        // for a copy without a date, which counts as having been there before.
        let mut copies: HashMap<&str, HashMap<&str, Option<i64>>> = HashMap::new();
        for entry in self
            .entries
            .iter()
            .filter(|entry| works.contains(entry.work_id.as_str()))
        {
            let time = server_time_seconds(&entry.added);
            copies
                .entry(entry.work_id.as_str())
                .or_default()
                .entry(copy_key(&entry.file_name))
                .and_modify(|first| *first = first.zip(time).map(|(left, right)| left.min(right)))
                .or_insert(time);
        }
        found
            .into_iter()
            .map(|(time, entry)| {
                let own = copy_key(&entry.file_name);
                let update = copies.get(entry.work_id.as_str()).is_some_and(|keys| {
                    keys.iter()
                        .any(|(key, first)| *key != own && first.is_none_or(|first| first < time))
                });
                Recent { entry, update }
            })
            .collect()
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

/// Which copy of a work a stored file is: `<code>__<id>.<ext>` by its id, so
/// the parts of a split set share one.
fn copy_key(name: &str) -> &str {
    let id = name.split_once("__").map_or(name, |(_, id)| id);
    id.split('.').next().unwrap_or(id)
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
    fn lists_files_by_date_added_and_tells_updates_from_new_works() {
        let manifest = r#"{"files": {
          "RJ1__old.zip": {"work_id": "RJ1", "title": "Older work", "added_date": "2026-06-01T10:00:00Z"},
          "RJ1__new.zip": {"work_id": "RJ1", "title": "Older work", "updated_date": "2026-10-12", "version": "1.2", "added_date": "2026-10-18T09:14:00Z"},
          "RJ2__a.part1.exe": {"work_id": "RJ2", "added_date": "2026-10-17T08:00:00Z"},
          "RJ2__a.part2.rar": {"work_id": "RJ2", "added_date": "2026-10-17T08:01:00Z"},
          "RJ3__x.zip": {"work_id": "RJ3", "added_date": "2026-10-16T12:00:00Z"},
          "RJ3__y.zip": {"work_id": "RJ3", "added_date": "2026-10-16T12:00:00Z"},
          "RJ4__before.zip": {"work_id": "RJ4"},
          "RJ4__later.zip": {"work_id": "RJ4", "added_date": "2026-10-16T07:00:00Z"},
          "RJ9__unreadable.zip": {"work_id": "RJ9", "added_date": "yesterday"}
        }}"#;
        let catalog = Catalog::new(
            ["2026-10".to_owned()],
            parse_manifest("2026-10", manifest.as_bytes()).unwrap(),
        );
        assert_eq!(
            catalog.entry("2026-10", "RJ1__new.zip").unwrap().added,
            "2026-10-18T09:14:00Z"
        );
        let since = server_time_seconds("2026-10-11T09:14:00Z").unwrap();
        let recent: Vec<_> = catalog
            .arrived_since(since)
            .into_iter()
            .map(|recent| (recent.entry.file_name.as_str(), recent.update))
            .collect();
        assert_eq!(
            recent,
            [
                // Another copy was already in the archive.
                ("RJ1__new.zip", true),
                // The second part of the same copy is not an update.
                ("RJ2__a.part2.rar", false),
                ("RJ2__a.part1.exe", false),
                // Copies added together are both new.
                ("RJ3__x.zip", false),
                ("RJ3__y.zip", false),
                // A copy without a date counts as having been there before.
                ("RJ4__later.zip", true),
            ]
        );
        assert!(catalog.arrived_since(i64::MAX).is_empty());

        // Months by date added, with the boundaries at the viewer's midnight.
        let months = |offset: i64| -> Vec<(String, usize)> {
            catalog
                .added_months(offset)
                .into_iter()
                .map(|month| (month.name, month.files))
                .collect()
        };
        assert_eq!(
            months(0),
            [("2026-06".to_owned(), 1), ("2026-10".to_owned(), 6)]
        );
        // Ten hours west of UTC, 07:00 on 16 October is still the 15th, and
        // 10:00 on 1 June is midnight of the 1st; nothing moves across a month.
        assert_eq!(months(-10 * 3600), months(0));
        assert_eq!(months(15 * 3600)[0], ("2026-06".to_owned(), 1));
        let june: Vec<_> = catalog
            .added_in("2026-06", 0)
            .into_iter()
            .map(|recent| (recent.entry.file_name.as_str(), recent.update))
            .collect();
        // The original copy is not an update, though a newer one came later.
        assert_eq!(june, [("RJ1__old.zip", false)]);
        assert_eq!(catalog.added_in("2026-10", 0).len(), 6);
        assert!(catalog.added_in("2026-07", 0).is_empty());
        assert!(catalog.added_in("not a month", 0).is_empty());
        // Twelve hours west, 09:14 UTC on 1 November is still October.
        let late = Catalog::new(
            ["2026-10".to_owned()],
            parse_manifest(
                "2026-10",
                br#"{"files": {"RJ5__z.zip": {"work_id": "RJ5", "added_date": "2026-11-01T09:14:00Z"}}}"#,
            )
            .unwrap(),
        );
        assert_eq!(late.added_in("2026-10", -12 * 3600).len(), 1);
        assert_eq!(late.added_in("2026-11", 0).len(), 1);
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
