use anyhow::Result;
use bincode::Options;
use fst::MapBuilder;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use crate::models::{Entry, FullTextIndexRecord, IndexPaths, ReferenceIndexRecord};

pub struct IndexBuilder {
    paths: IndexPaths,
}

impl IndexBuilder {
    #[must_use]
    pub fn new<P: AsRef<Path>>(base_path: P) -> Self {
        Self {
            paths: IndexPaths::new(base_path),
        }
    }

    pub fn build(&self, entries: &[Entry]) -> Result<()> {
        let entries = Self::normalize_entries(entries);
        let reference_records = Self::build_reference_index_records(&entries)?;
        let fulltext_records = Self::build_fulltext_index_records(&entries);

        self.save_reference_index(&reference_records)?;
        self.save_reference_blob(&entries)?;
        self.save_fulltext_blob(&entries)?;
        self.build_fulltext_offsets(&fulltext_records)?;
        self.build_fst_index(&entries)?;

        Ok(())
    }

    fn normalize_entries(entries: &[Entry]) -> Vec<Entry> {
        let mut sorted_entries = entries.to_vec();
        sorted_entries.sort_by(|left, right| {
            left.normalized_headword
                .cmp(&right.normalized_headword)
                .then(left.headword.cmp(&right.headword))
        });

        let mut normalized: Vec<Entry> = Vec::with_capacity(sorted_entries.len());

        for entry in sorted_entries {
            if entry.normalized_headword.is_empty() {
                continue;
            }

            if let Some(last_entry) = normalized.last_mut() {
                if last_entry.normalized_headword == entry.normalized_headword {
                    for sense in entry.senses {
                        last_entry.add_sense(sense, entry.entry_type.clone());
                    }
                    continue;
                }
            }

            normalized.push(entry);
        }

        normalized
    }

    fn build_fulltext_index_records(entries: &[Entry]) -> Vec<FullTextIndexRecord> {
        let mut offset = 0u64;
        let mut records = Vec::with_capacity(entries.len());

        for (entry_id, entry) in entries.iter().enumerate() {
            let search_text = entry.search_text().to_lowercase();
            #[allow(clippy::cast_possible_truncation)]
            let len = search_text.len() as u32;
            #[allow(clippy::cast_possible_truncation)]
            records.push(FullTextIndexRecord {
                entry_id: entry_id as u32,
                offset,
                len,
            });
            offset += u64::from(len);
        }

        records
    }

    fn build_reference_index_records(entries: &[Entry]) -> Result<Vec<ReferenceIndexRecord>> {
        let mut offset = 0u64;
        let mut records = Vec::with_capacity(entries.len());

        for entry in entries {
            let entry_bytes = bincode::DefaultOptions::new()
                .with_fixint_encoding()
                .serialize(entry)?;
            #[allow(clippy::cast_possible_truncation)]
            let len = entry_bytes.len() as u32;
            records.push(ReferenceIndexRecord { offset, len, padding: 0 });
            offset += u64::from(len);
        }

        Ok(records)
    }

    fn build_fst_index(&self, entries: &[Entry]) -> Result<()> {
        let file = File::create(&self.paths.fst_path)?;
        let mut builder = MapBuilder::new(file)?;

        for (idx, entry) in entries.iter().enumerate() {
            #[allow(clippy::cast_possible_truncation)]
            let value = ((idx as u64) << 32) | (entry.headword.len() as u64);
            builder.insert(entry.normalized_headword.as_ref(), value)?;
        }

        builder.finish()?;
        log::info!("FST index written to {}", self.paths.fst_path.display());
        Ok(())
    }

    fn save_reference_index(&self, records: &[ReferenceIndexRecord]) -> Result<()> {
        let file = File::create(&self.paths.reference_index_path)?;
        let mut writer = BufWriter::new(file);

        for record in records {
            let bytes = bytemuck::bytes_of(record);
            writer.write_all(bytes)?;
        }

        writer.flush()?;
        log::info!("Reference index file written to {}", self.paths.reference_index_path.display());
        Ok(())
    }

    fn save_reference_blob(&self, entries: &[Entry]) -> Result<()> {
        let file = File::create(&self.paths.reference_path)?;
        let mut writer = BufWriter::new(file);

        for entry in entries {
            let entry_bytes = bincode::DefaultOptions::new()
                .with_fixint_encoding()
                .serialize(entry)?;
            writer.write_all(&entry_bytes)?;
        }

        writer.flush()?;
        log::info!("Reference file written to {}", self.paths.reference_path.display());
        Ok(())
    }

    fn save_fulltext_blob(&self, entries: &[Entry]) -> Result<()> {
        let file = File::create(&self.paths.fulltext_path)?;
        let mut writer = BufWriter::new(file);

        for entry in entries {
            writer.write_all(entry.search_text().to_lowercase().as_bytes())?;
        }

        writer.flush()?;
        log::info!("Full-text blob written to {}", self.paths.fulltext_path.display());
        Ok(())
    }

    fn build_fulltext_offsets(&self, fulltext_records: &[FullTextIndexRecord]) -> Result<()> {
        let file = File::create(&self.paths.fulltext_entry_map_path)?;
        let mut writer = BufWriter::new(file);

        let num_entries = fulltext_records.len() as u64;
        writer.write_all(&num_entries.to_le_bytes())?;

        // Write each offset as u64. They are already sorted by construction.
        for record in fulltext_records {
            writer.write_all(&record.offset.to_le_bytes())?;
        }

        writer.flush()?;
        #[allow(clippy::cast_precision_loss)]
        let size_mb = (fulltext_records.len() * 8) as f64 / (1024.0 * 1024.0);
        log::info!(
            "Full-text offsets written to {} ({} entries, {size_mb:.1}MB)",
            self.paths.fulltext_entry_map_path.display(),
            fulltext_records.len()
        );
        Ok(())
    }
}
