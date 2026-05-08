
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Entry {
    pub headword: Box<str>,
    pub normalized_headword: Box<str>,
    pub entry_type: Option<Box<str>>,
    pub senses: Vec<Sense>,
}

impl Entry {
    #[must_use]
    pub fn new(
        headword: String,
        normalized_headword: String,
        entry_type: Option<String>,
        sense: Sense,
    ) -> Self {
        Self {
            headword: headword.into_boxed_str(),
            normalized_headword: normalized_headword.into_boxed_str(),
            entry_type: entry_type.map(String::into_boxed_str),
            senses: vec![sense],
        }
    }

    pub fn add_sense(&mut self, sense: Sense, entry_type: Option<Box<str>>) {
        if self.entry_type.is_none() {
            self.entry_type = entry_type;
        }
        self.senses.push(sense);
    }

    #[must_use]
    pub fn search_text(&self) -> String {
        let mut parts = vec![self.headword.to_string()];

        if let Some(entry_type) = &self.entry_type {
            parts.push(entry_type.to_string());
        }

        for sense in &self.senses {
            parts.push(sense.search_text());
        }

        parts.join(" ")
    }

}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Sense {
    pub description: Box<str>,
    pub complements: Vec<Box<str>>,
    pub attributes: Vec<Attribute>,
    pub examples: Vec<Box<str>>,
}

impl Sense {
    #[must_use]
    pub fn search_text(&self) -> String {
        let mut parts = vec![self.description.to_string()];

        parts.extend(self.complements.iter().map(ToString::to_string));
        parts.extend(self.attributes.iter().map(|attribute| attribute.value.to_string()));
        parts.extend(self.examples.iter().map(ToString::to_string));

        parts.join(" ")
    }

    #[must_use]
    pub fn display_text(&self) -> String {
        let mut text = self.description.to_string();

        if !self.complements.is_empty() {
            text.push_str(" | ");
            text.push_str(&self.complements.join("; "));
        }

        if !self.attributes.is_empty() {
            let rendered_attributes = self
                .attributes
                .iter()
                .map(Attribute::display_text)
                .collect::<Vec<_>>()
                .join(", ");
            text.push_str(" | ");
            text.push_str(&rendered_attributes);
        }

        if !self.examples.is_empty() {
            text.push_str(" | ");
            text.push_str(&self.examples.join(" / "));
        }

        text
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Attribute {
    pub name: Box<str>,
    pub value: Box<str>,
}

impl Attribute {
    #[must_use]
    pub fn display_text(&self) -> String {
        format!("{}:{}", self.name, self.value)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[repr(C)]
pub struct FullTextIndexRecord {
    pub entry_id: u32,
    pub offset: u64,
    pub len: u32,
}

impl FullTextIndexRecord {
    pub const RECORD_SIZE: usize = 16;

    #[must_use]
    pub fn from_bytes(data: &[u8], index: usize) -> Option<Self> {
        let start = index * Self::RECORD_SIZE;
        let end = start + Self::RECORD_SIZE;
        if end > data.len() {
            return None;
        }
        let bytes = &data[start..end];
        let entry_id = u32::from_le_bytes(bytes[..4].try_into().ok()?);
        let offset = u64::from_le_bytes(bytes[4..12].try_into().ok()?);
        let len = u32::from_le_bytes(bytes[12..16].try_into().ok()?);
        Some(Self { entry_id, offset, len })
    }

    #[must_use]
    pub fn search_text<'a>(&self, blob: &'a [u8]) -> Option<&'a str> {
        let start = usize::try_from(self.offset).ok()?;
        let end = start + self.len as usize;
        if end > blob.len() {
            return None;
        }
        std::str::from_utf8(&blob[start..end]).ok()
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ReferenceIndexRecord {
    pub offset: u64,
    pub len: u32,
    pub padding: u32,
}

impl ReferenceIndexRecord {
    pub const RECORD_SIZE: usize = std::mem::size_of::<Self>();

    #[must_use]
    pub fn from_bytes(data: &[u8], index: usize) -> Option<Self> {
        let start = index * Self::RECORD_SIZE;
        let end = start + Self::RECORD_SIZE;
        let bytes = data.get(start..end)?;

        let (prefix, slice, suffix) = bytemuck::pod_align_to::<u8, Self>(bytes);
        if prefix.is_empty() && suffix.is_empty() && !slice.is_empty() {
            Some(slice[0])
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
pub struct IndexPaths {
    pub fst_path: PathBuf,
    pub reference_index_path: PathBuf,
    pub reference_path: PathBuf,
    pub fulltext_path: PathBuf,
    pub fulltext_entry_map_path: PathBuf,
}

impl IndexPaths {
    #[must_use]
    pub fn new<P: AsRef<Path>>(base_path: P) -> Self {
        let base = base_path.as_ref();
        Self {
            fst_path: base.join("headwords.fst"),
            reference_index_path: base.join("reference_index.bin"),
            reference_path: base.join("reference.bin"),
            fulltext_path: base.join("fulltext.bin"),
            fulltext_entry_map_path: base.join("fulltext_entry_map.bin"),
        }
    }
}

#[must_use]
pub fn normalize_lookup_key(text: &str) -> String {
    text.chars()
        .filter_map(|character| {
            if character.is_ascii_alphanumeric() {
                Some(character.to_ascii_lowercase())
            } else {
                None
            }
        })
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub entries: Vec<Entry>,
    pub query: String,
    pub search_type: SearchType,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SearchType {
    PrefixMatch,
    FullText,
}
