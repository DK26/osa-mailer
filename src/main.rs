// https://stackoverflow.com/questions/65356683/how-to-mutate-serde-json-value-by-adding-additional-fields

mod entries;
mod errors;

use std::{collections::HashMap, path::Path};

use entries::crc32_iso_hdlc_checksum;
use errors::EntryError;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;

use chrono::{DateTime, FixedOffset};
use walkdir::{DirEntry, WalkDir};

const ENTRY_DIR: &str = "entries";
const ENTRY_EXT: &str = ".json";

#[derive(Serialize, Debug)]
struct AccumulatedValue {
    number: u32,
    value: serde_json::Value,
}

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
struct Email {
    system: String,
    subsystem: String,
    from: String,
    to: Vec<String>,
    cc: Vec<String>,
    bcc: Vec<String>,
    reply_to: Vec<String>,
    subject: String,
    template: String,
    alternative_content: String,
    attachments: Vec<String>,
    custom_key: String,
}

/// A Composed E-mail is one that has all of its context gathered and ordered.
#[derive(Serialize, Deserialize, Debug, Default)]
struct ComposedEmail {
    header: Email,
    context: serde_json::Map<String, serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug)]
struct Entry {
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
struct EntryContent {
    id: String,
    content: String,
}

#[derive(Debug)]
struct EntryParseError {
    entry_content: EntryContent,
    error: serde_json::Error,
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
struct EntryParseResults {
    ok: Vec<Entry>,
    err: Vec<EntryParseError>,
}

fn load_entries<P: AsRef<Path>>(dir: P, extension: &str) -> EntryParseResults {
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
fn map_emails(entries_pool: Vec<Entry>) -> EmailEntries {
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

fn compose_emails(email_entries: &EmailEntries) -> Vec<ComposedEmail> {
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

fn main() -> anyhow::Result<()> {
    let current_exe =
        env::current_exe().expect("Unable to get the current binary file from the OS.");
    let current_exe_dir = current_exe
        .parent()
        .expect("Unable to get current binary file directory");

    let entries_path = current_exe_dir.join(ENTRY_DIR);

    let entry_parse_results = load_entries(entries_path, ENTRY_EXT);

    eprintln!("Entry parsing errors: {:?}", entry_parse_results.err);

    let entries_pool = entry_parse_results.ok;

    let emails_map = map_emails(entries_pool); // Each E-Mail ID with its E-mail contents, in order

    let res = compose_emails(&emails_map);

    println!("res = {res:#?}");

    Ok(())
}
