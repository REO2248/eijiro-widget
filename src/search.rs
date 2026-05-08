use anyhow::{Context, Result};
use bincode::Options;
use fst::IntoStreamer;
use fst::Map;
use fst::Streamer;
use fxhash::FxHashSet;
use memmap2::Mmap;
use std::fs::File;
use std::io::Cursor;

use crate::models::{Entry, IndexPaths, ReferenceIndexRecord, SearchResult, SearchType, normalize_lookup_key};

pub struct PrefixSearchEngine {
    reference_index_mmap: Mmap,
    reference_blob: Mmap,
    fst_mmap: Mmap,
}

pub struct FullTextSearchEngine {
    reference_index_mmap: Mmap,
    reference_blob: Mmap,
    fulltext_blob: Mmap,
    fulltext_entry_map_mmap: Mmap,
}

impl PrefixSearchEngine {
    pub fn load(paths: &IndexPaths) -> Result<Self> {
        let reference_index_mmap = Self::load_mmap(&paths.reference_index_path, "reference index")?;
        let reference_blob = Self::load_mmap(&paths.reference_path, "reference")?;
        let fst_mmap = Self::load_mmap(&paths.fst_path, "FST")?;

        Ok(Self {
            reference_index_mmap,
            reference_blob,
            fst_mmap,
        })
    }

    fn load_mmap(path: &std::path::Path, name: &str) -> Result<Mmap> {
        let file = File::open(path)
            .with_context(|| format!("Failed to open the {} file: {}", name, path.display()))?;
        unsafe { Mmap::map(&file) }
            .with_context(|| format!("Failed to memory-map the {name} file"))
    }

    pub fn search_prefix(&self, query: &str, limit: usize) -> Result<SearchResult> {
        let query_normalized = normalize_lookup_key(query);

        if limit == 0 {
            return Ok(SearchResult {
                entries: Vec::new(),
                query: query.to_string(),
                search_type: SearchType::PrefixMatch,
            });
        }

        if !query.is_empty() && query_normalized.is_empty() {
            return Ok(SearchResult {
                entries: Vec::new(),
                query: query.to_string(),
                search_type: SearchType::PrefixMatch,
            });
        }

        let fst_map = Map::new(self.fst_mmap.as_ref())
            .context("Failed to parse FST map")?;
        
        let mut stream = if query_normalized.is_empty() {
            fst_map.range().ge("").into_stream()
        } else {
            fst_map.range().ge(query_normalized.as_str()).into_stream()
        };

        let mut candidates: Vec<(usize, usize)> = Vec::with_capacity(limit);

        while let Some((key, value)) = stream.next() {
            if !query_normalized.is_empty() && !key.starts_with(query_normalized.as_bytes()) {
                break;
            }

            let entry_index = (value >> 32) as usize;
            let headword_len = (value & 0xFFFF_FFFF) as usize;
            candidates.push((entry_index, headword_len));

            if candidates.len() >= limit {
                break;
            }
        }

        candidates.sort_unstable_by(|left, right| {
            left.1.cmp(&right.1).then(left.0.cmp(&right.0))
        });

        let reference_indices = self.reference_indices();

        let entries: Vec<Entry> = candidates
            .into_iter()
            .filter_map(|(idx, _)| {
                let record = reference_indices.get(idx)?;
                self.entry_from_record(record, idx).ok()
            })
            .collect();

        Ok(SearchResult {
            entries,
            query: query.to_string(),
            search_type: SearchType::PrefixMatch,
        })
    }

    fn reference_indices(&self) -> &[ReferenceIndexRecord] {
        let (prefix, slice, suffix) = bytemuck::pod_align_to::<u8, ReferenceIndexRecord>(&self.reference_index_mmap);
        if !prefix.is_empty() || !suffix.is_empty() {
            log::warn!(
                "Reference index alignment mismatch: prefix={}, suffix={}",
                prefix.len(),
                suffix.len()
            );
        }
        slice
    }

    fn entry_from_record(&self, record: &ReferenceIndexRecord, entry_id: usize) -> Result<Entry> {
        let start = usize::try_from(record.offset).context("Record offset is too large for this system")?;
        let end = start + record.len as usize;

        if end > self.reference_blob.len() {
            anyhow::bail!("Reference record {entry_id} extends beyond blob (offset={}, len={}, blob_size={})",
                record.offset, record.len, self.reference_blob.len());
        }

        let entry = bincode::DefaultOptions::new()
            .with_fixint_encoding()
            .deserialize_from(Cursor::new(&self.reference_blob[start..end]))
            .with_context(|| format!("Failed to deserialize entry {entry_id}"))?;

        Ok(entry)
    }

}

impl FullTextSearchEngine {
    pub fn load(paths: &IndexPaths) -> Result<Self> {
        let reference_index_mmap = Self::load_mmap(&paths.reference_index_path, "reference index")?;
        let reference_blob = Self::load_mmap(&paths.reference_path, "reference")?;
        let fulltext_blob = Self::load_mmap(&paths.fulltext_path, "fulltext")?;
        let fulltext_entry_map_mmap =
            Self::load_mmap(&paths.fulltext_entry_map_path, "fulltext entry map")?;

        Ok(Self {
            reference_index_mmap,
            reference_blob,
            fulltext_blob,
            fulltext_entry_map_mmap,
        })
    }

    fn load_mmap(path: &std::path::Path, name: &str) -> Result<Mmap> {
        let file = File::open(path)
            .with_context(|| format!("Failed to open the {name} file: {}", path.display()))?;
        unsafe { Mmap::map(&file) }
            .with_context(|| format!("Failed to memory-map the {name} file"))
    }

    fn fulltext_offsets(&self) -> &[u64] {
        let data = &self.fulltext_entry_map_mmap;
        if data.len() < 8 {
            return &[];
        }

        let num_entries_bytes: [u8; 8] = match data[..8].try_into() {
            Ok(b) => b,
            Err(_) => return &[],
        };
        let num_entries = u64::from_le_bytes(num_entries_bytes);

        #[allow(clippy::cast_possible_truncation)]
        let expected_len = 8 + (num_entries as usize * 8);
        let actual_len = data.len().min(expected_len);
        let entries_data = &data[8..actual_len];

        let (prefix, u64_slice, suffix) = bytemuck::pod_align_to::<u8, u64>(entries_data);
        if !prefix.is_empty() || !suffix.is_empty() {
            log::warn!(
                "Full-text entry map alignment mismatch: prefix={}, suffix={}",
                prefix.len(),
                suffix.len()
            );
        }
        u64_slice
    }

    pub fn search_fulltext(&self, query: &str, limit: usize) -> Result<SearchResult> {
        let query_lower = query.to_lowercase();
        if query_lower.is_empty() || limit == 0 {
            return Ok(SearchResult {
                entries: Vec::new(),
                query: query.to_string(),
                search_type: SearchType::FullText,
            });
        }

        let ac = aho_corasick::AhoCorasickBuilder::new()
            .match_kind(aho_corasick::MatchKind::LeftmostFirst)
            .build([&query_lower])
            .context("Failed to build Aho-Corasick automaton")?;

        let mut matched_entry_ids = Vec::new();
        let mut seen_entries = FxHashSet::default();
        let offsets = self.fulltext_offsets();

        for matched in ac.find_iter(&self.fulltext_blob) {
            #[allow(clippy::cast_possible_truncation)]
            let match_pos = matched.start() as u64;

            let entry_id = match offsets.binary_search(&match_pos) {
                Ok(idx) => idx,
                Err(idx) => {
                    if idx == 0 {
                        continue; // Should not happen if data is valid
                    }
                    idx - 1
                }
            };

            #[allow(clippy::cast_possible_truncation)]
            let entry_id_u32 = entry_id as u32;
            if seen_entries.insert(entry_id_u32) {
                matched_entry_ids.push(entry_id_u32);
                if matched_entry_ids.len() >= limit {
                    break;
                }
            }
        }

        let mut entries = Vec::with_capacity(matched_entry_ids.len());
        for entry_id in matched_entry_ids {
            if let Some(entry) = self.entry_at(entry_id as usize)? {
                entries.push(entry);
            }
        }

        Ok(SearchResult {
            entries,
            query: query.to_string(),
            search_type: SearchType::FullText,
        })
    }

    fn reference_indices(&self) -> &[ReferenceIndexRecord] {
        let (prefix, slice, suffix) =
            bytemuck::pod_align_to::<u8, ReferenceIndexRecord>(&self.reference_index_mmap);
        if !prefix.is_empty() || !suffix.is_empty() {
            log::warn!(
                "Reference index alignment mismatch: prefix={}, suffix={}",
                prefix.len(),
                suffix.len()
            );
        }
        slice
    }

    fn entry_from_record(&self, record: &ReferenceIndexRecord, entry_id: usize) -> Result<Entry> {
        let start =
            usize::try_from(record.offset).context("Record offset is too large for this system")?;
        let end = start + record.len as usize;

        if end > self.reference_blob.len() {
            anyhow::bail!("Reference record {entry_id} extends beyond blob (offset={}, len={}, blob_size={})",
                record.offset, record.len, self.reference_blob.len());
        }

        let entry = bincode::DefaultOptions::new()
            .with_fixint_encoding()
            .deserialize_from(Cursor::new(&self.reference_blob[start..end]))
            .with_context(|| format!("Failed to deserialize entry {entry_id}"))?;

        Ok(entry)
    }

    fn entry_at(&self, entry_id: usize) -> Result<Option<Entry>> {
        let indices = self.reference_indices();
        let Some(record) = indices.get(entry_id) else {
            return Ok(None);
        };
        self.entry_from_record(record, entry_id).map(Some)
    }
}
