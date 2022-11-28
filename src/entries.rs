use serde::{Deserialize, Serialize};
use std::fs;
use std::{collections::HashMap, path::Path};

use chrono::{DateTime, FixedOffset};
use walkdir::{DirEntry, WalkDir};

use crc::{Algorithm, Crc, CRC_32_ISO_HDLC};

// CRC_32_ISO_HDLC is compatible with Python 3
const CRC32_ALGORITHM: Algorithm<u32> = CRC_32_ISO_HDLC;

/// Returns a checksum calculated with CRC32 using the ISO HDLC algorithm for compatibility with Python.
pub fn crc32_iso_hdlc_checksum(bytes: &[u8]) -> u32 {
    let crc: Crc<u32> = Crc::<u32>::new(&CRC32_ALGORITHM);
    crc.checksum(bytes)
}

// from:
// +entries: [ { .. }, { .. } ]

// to:
// entries: [
//  { n: N, v: [ { .. }, { .. } ] }
// ]

// from:
// +entries: [ { .. }, { .. } ]

// to:
// entries: [
//  { n: N, v: [ { .. }, { .. } ] },
//  { n: N, v: [ { .. }, { .. } ] }
// ]

// replace `+entries` with new `entries`

#[derive(Serialize, Debug)]
struct AccumulatedValue {
    number: u32,
    value: serde_json::Value,
}

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub(crate) struct Email {
    pub(crate) system: String,
    pub(crate) subsystem: String,
    pub(crate) from: String,
    pub(crate) to: Vec<String>,
    pub(crate) cc: Vec<String>,
    pub(crate) bcc: Vec<String>,
    pub(crate) reply_to: Vec<String>,
    pub(crate) subject: String,
    pub(crate) template: String,
    pub(crate) alternative_content: String,
    pub(crate) attachments: Vec<String>,
    pub(crate) custom_key: String,
}

/// A Composed E-mail is one that has all of its context gathered and ordered.
#[derive(Serialize, Deserialize, Debug, Default)]
pub(crate) struct ComposedEmail {
    pub(crate) header: Email,
    pub(crate) context: serde_json::Map<String, serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Entry {
    id: String,
    utc: DateTime<FixedOffset>,
    notify_error: Vec<String>,
    email: Email,
    context: serde_json::Map<String, serde_json::Value>,
}

impl Entry {
    /// Calculate the E-Mail ID for the current entry.
    pub fn email_id(&self) -> u32 {
        let email_string = serde_json::to_string(&self.email)
            .expect("Deserialized from JSON but cannot be serialized into JSON?");
        crc32_iso_hdlc_checksum(email_string.as_bytes())
    }
}

#[derive(Debug, Clone)]
pub(crate) struct EntryContent {
    id: String,
    content: String,
}

#[derive(Debug)]
pub(crate) struct EntryParseError {
    pub(crate) entry_content: EntryContent,
    pub(crate) error: serde_json::Error,
}

fn parse_entities(
    entries: &Vec<EntryContent>,
    parsed_entries: &mut Vec<Entry>,
    parse_errors: &mut Vec<EntryParseError>,
) {
    for entry in entries {
        match serde_json::from_str::<Entry>(&entry.content) {
            Ok(v) => parsed_entries.push(v),
            Err(e) => parse_errors.push(EntryParseError {
                entry_content: entry.clone(),
                error: e,
            }),
        }
    }
}

fn is_entry(entry: &DirEntry, extension: &str) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|s| s.to_lowercase().ends_with(extension))
        .unwrap_or(false)
}

/// The results of parsing the entry files
#[derive(Debug)]
pub(crate) struct EntryParseResults {
    pub(crate) ok: Vec<Entry>,
    pub(crate) err: Vec<EntryParseError>,
}

pub(crate) fn load_entries<P: AsRef<Path>>(dir: P, extension: &str) -> EntryParseResults {
    let mut raw_entries = Vec::new();
    for entry in WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| is_entry(e, extension))
    {
        let entry_content = fs::read_to_string(entry.path());

        match entry_content {
            Ok(v) => {
                raw_entries.push(EntryContent {
                    id: entry.path().display().to_string(),
                    content: v,
                });
                let _ = fs::remove_file(entry.path());
            }
            Err(_) => continue,
        }
    }

    let mut result = Vec::new();
    let mut errors = Vec::new();

    parse_entities(&raw_entries, &mut result, &mut errors);

    EntryParseResults {
        ok: result,
        err: errors,
    }
}

type EmailEntries = HashMap<u32, Vec<Entry>>;

/// Arrange all entries for each E-Mail ID in an ordered manure.
pub(crate) fn map_emails(entries_pool: Vec<Entry>) -> EmailEntries {
    let mut email_entries: EmailEntries = HashMap::new();

    // Accumulate entries of the same E-mail
    for entry in entries_pool {
        // Calculate ID for each E-Mail entry
        let email_id = entry.email_id();

        // Retrieve entries vector for E-Mail ID (or create one if doesn't exists)
        let entries = email_entries.entry(email_id).or_insert_with(Vec::new);

        // Append new Entry to the E-Mail ID
        entries.push(entry)
    }

    // Order entries by their UTC time
    for (_, value) in email_entries.iter_mut() {
        value.sort_by(|a, b| a.utc.cmp(&b.utc))
    }

    email_entries
}

type JsonObject = serde_json::Map<String, serde_json::Value>;

fn copy_and_accumulate(source: &JsonObject, target: &mut JsonObject) {
    // Scan all key/value elements in the source JSON object
    for (k, v) in source {
        // Detected an accumulation sign in key name

        if let Some(key_name) = k.strip_prefix('+') {
            // Remove the prefixed version key from the target JSON object
            target.remove(k);

            // FIXME: Return error if key without `+` prefix already exists within the same JSON Object (DuplicationError)
            let value_vec = target
                .entry(key_name)
                .or_insert_with(|| serde_json::Value::Array(Vec::new()));

            if let serde_json::Value::Array(value_vec) = value_vec {
                value_vec.push(serde_json::json!(AccumulatedValue {
                    number: (value_vec.len() + 1) as u32,
                    value: v.clone(),
                }));
            }
        } else if let serde_json::Value::Object(json_obj_borrowed) = v {
            let nested_target = target
                .entry(k)
                .or_insert_with(|| serde_json::Value::Object(json_obj_borrowed.to_owned()));

            if let serde_json::Value::Object(ref mut iv) = nested_target {
                copy_and_accumulate(json_obj_borrowed, iv);
            }
        } else {
            target.entry(k).or_insert_with(|| v.clone());
        }
    }
}

pub(crate) fn compose_emails(email_entries: &EmailEntries) -> Vec<ComposedEmail> {
    let mut composed_emails = Vec::new();

    for entries in email_entries.values() {
        let first_entry = entries
            .get(0)
            .expect("The vector was created empty when it was inserted to the map.");

        let email = first_entry.email.clone();

        let mut final_context = first_entry.context.clone();

        for entry in entries {
            let entry_context = &entry.context;
            copy_and_accumulate(entry_context, &mut final_context);
        }

        let composed_email = ComposedEmail {
            header: email,
            context: final_context,
        };
        composed_emails.push(composed_email);
    }
    composed_emails
}
