use anyhow::Result;
use encoding_rs::SHIFT_JIS;
use fxhash::FxHashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use crate::models::{normalize_lookup_key, Attribute, Entry, Sense};

pub struct EijiroParser;

impl EijiroParser {
    pub fn parse_file<P: AsRef<Path>>(path: P) -> Result<Vec<Entry>> {
        let file = File::open(&path)?;
        let total_bytes = file.metadata()?.len();
        let mut reader = BufReader::new(file);
        let mut entries: Vec<Entry> = Vec::new();
        let mut entry_index_by_key: FxHashMap<String, usize> = FxHashMap::default();
        let mut buf = Vec::new();
        let mut bytes_read: u64 = 0;
        let progress_interval = 25_000usize;

        let mut line_no = 0usize;
        let mut current_entry_index: Option<usize> = None;

        loop {
            buf.clear();
            let read = reader.read_until(b'\n', &mut buf)?;
            if read == 0 {
                break;
            }

            bytes_read += read as u64;
            line_no += 1;

            while let Some(&b) = buf.last() {
                if b == b'\n' || b == b'\r' {
                    buf.pop();
                } else {
                    break;
                }
            }

            let line_utf8 = decode_cp932(&buf, line_no);

            if line_no.is_multiple_of(progress_interval) {
                log::info!(
                    "Parsed {}/{} bytes ({:.1}%)...",
                    bytes_read,
                    total_bytes,
                    progress_ratio(bytes_read, total_bytes)
                );
            }

            if let Some(example) = parse_continuation_line(&line_utf8) {
                if let Some(entry_index) = current_entry_index {
                    if let Some(entry) = entries.get_mut(entry_index) {
                        if let Some(last_sense) = entry.senses.last_mut() {
                            last_sense.examples.push(example.into_boxed_str());
                        }
                    }
                }
                continue;
            }

            if let Some(parsed) = parse_head_line(&line_utf8) {
                let (headword, entry_type, sense) = parsed;
                let normalized = normalize_lookup_key(&headword);

                if let Some(&entry_index) = entry_index_by_key.get(&normalized) {
                    if let Some(entry) = entries.get_mut(entry_index) {
                        entry.add_sense(sense, entry_type.map(String::into_boxed_str));
                    }
                    current_entry_index = Some(entry_index);
                } else {
                    let entry = Entry::new(headword, normalized.clone(), entry_type, sense);
                    let entry_index = entries.len();
                    entries.push(entry);
                    entry_index_by_key.insert(normalized, entry_index);
                    current_entry_index = Some(entry_index);
                }
            }
        }

        log::info!(
            "Finished parsing {}/{} bytes ({:.1}%) and {} entries.",
            bytes_read,
            total_bytes,
            progress_ratio(bytes_read, total_bytes),
            entries.len()
        );
        Ok(entries)
    }
}

#[allow(clippy::cast_precision_loss)]
fn progress_ratio(current: u64, total: u64) -> f64 {
    if total == 0 {
        return 100.0;
    }

    (current as f64 / total as f64) * 100.0
}

fn decode_cp932(bytes: &[u8], line_no: usize) -> String {
    let (decoded, _, had_errors) = SHIFT_JIS.decode(bytes);
    if had_errors {
        log::warn!("Line {line_no} contained bytes that required replacement during CP932 decoding.");
    }
    decoded.into_owned()
}

fn parse_head_line(line: &str) -> Option<(String, Option<String>, Sense)> {
    let line = line.trim_start_matches('■').trim();

    if line.is_empty() {
        return None;
    }

    let colon_idx = line.find(':')?;

    let left = line[..colon_idx].trim();
    let right = line[colon_idx + 1..].trim();

    let (headword, entry_type) = extract_headword_and_type(left);
    let sense = parse_sense(right);

    Some((headword, entry_type, sense))
}

fn parse_continuation_line(line: &str) -> Option<String> {
    if let Some(rest) = line.strip_prefix("■・") {
        return Some(rest.trim().to_string());
    }

    if let Some(rest) = line.strip_prefix('@') {
        return Some(rest.trim().to_string());
    }

    None
}

fn extract_headword_and_type(left: &str) -> (String, Option<String>) {
    if let Some(start) = left.find('{') {
        if let Some(end) = left[start + 1..].find('}') {
            let headword = left[..start].trim().to_string();
            let entry_type = left[start + 1..start + 1 + end].trim().to_string();
            return (headword, Some(entry_type));
        }
    }

    (left.trim().to_string(), None)
}

fn parse_sense(body: &str) -> Sense {
    let mut segments = body.split('◆');
    let first_segment = segments.next().unwrap_or("").trim();
    let complements = segments
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .map(|segment| segment.to_string().into_boxed_str())
        .collect();

    let (description, attributes) = extract_attributes(first_segment);

    Sense {
        description: description.into_boxed_str(),
        complements,
        attributes,
        examples: Vec::new(),
    }
}

fn extract_attributes(text: &str) -> (String, Vec<Attribute>) {
    let mut remaining = text.trim();
    let mut description = String::new();
    let mut attributes = Vec::new();

    while let Some(open_idx) = remaining.find('【') {
        description.push_str(remaining[..open_idx].trim_end());

        let after_open = &remaining[open_idx + '【'.len_utf8()..];
        let Some(close_idx) = after_open.find('】') else {
            description.push_str(remaining[open_idx..].trim());
            return (normalize_text(&description), attributes);
        };

        let name = after_open[..close_idx].trim().to_string();
        let after_name = &after_open[close_idx + '】'.len_utf8()..];
        let (value, rest) = split_attribute_value(after_name);
        attributes.push(Attribute {
            name: name.into_boxed_str(),
            value: value.trim().trim_start_matches('、').trim().to_string().into_boxed_str(),
        });

        remaining = rest.trim_start_matches('、').trim_start();
    }

    description.push_str(remaining.trim());
    (normalize_text(&description), attributes)
}

fn split_attribute_value(text: &str) -> (&str, &str) {
    let mut boundary = text.len();

    for marker in ["、【", "◆", "■"] {
        if let Some(index) = text.find(marker) {
            boundary = boundary.min(index);
        }
    }

    (&text[..boundary], &text[boundary..])
}

fn normalize_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_head_line() {
        let line = r"■'hello world' : hello world";
        let result = parse_head_line(line);
        assert!(result.is_some());
        let (headword, entry_type, sense) = result.unwrap();
        assert_eq!(headword, r"'hello world'");
        assert_eq!(entry_type, None);
        assert_eq!(sense.description.as_ref(), "hello world");
    }

    #[test]
    fn test_parse_head_line_with_type_and_tags() {
        let line = r"■a {noun} : 【レベル】1、【発音】a";
        let result = parse_head_line(line).unwrap();
        assert_eq!(result.0, "a");
        assert_eq!(result.1, Some("noun".to_string()));
        assert_eq!(result.2.description.as_ref(), "");
        assert_eq!(result.2.attributes.len(), 2);
    }

    #[test]
    fn test_parse_continuation_line() {
        assert_eq!(parse_continuation_line("■・example"), Some("example".to_string()));
        assert_eq!(parse_continuation_line("@underline"), Some("underline".to_string()));
    }
}
