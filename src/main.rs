// https://stackoverflow.com/questions/65356683/how-to-mutate-serde-json-value-by-adding-additional-fields

mod entries;
mod errors;

use std::collections::HashMap;

use entries::crc32_iso_hdlc_checksum;
use errors::EntryError;
use serde::{Deserialize, Serialize};

use chrono::{DateTime, FixedOffset};

#[derive(Serialize, Debug)]
struct AccumulatedValue {
    n: u32,
    v: serde_json::Value,
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

fn load_files() -> Vec<Entry> {
    let entry_1 = r#"
    {
        "id": "50bf9e7e",
        "utc": "2022-09-01T22:44:11.852662+00:00",
        "notify_error": [
            "Developers <dev-team@somemail.com>"
        ],
        "email": {
            "system": "MyExternalSystem",
            "subsystem": "[ID:12345] Trigger: Server Disk Out-of-Space",
            "from": "Mail System <tech-support@somemail.com>",
            "to": [
                "Rick S. <someone@somemail.com>"
            ],
            "cc": [],
            "bcc": [],
            "reply_to": [
                "System Admin <admin@somemail.com>",
                "Project Lead <lead@somemail.com>"
            ],
            "subject": "Warning: Your server's disk is out-of-space",
            "template": "ops_department",
            "alternative_content": "Unable to render HTML. Please refer to the Ops department for details.",
            "attachments": [
                "guides/disk-capacity-guidelines.pdf"
            ],
            "custom_key": ""
        },
        "context": {
            "message": {
                "head": "Detected Problems in Your Server",
                "body": "We have detected a disk capacity problem with one or more of your servers. Please refer to the instructions below"
            },
            "table": {
                "type": 1,
                "+entries": [
                    {
                        "idx": 1,
                        "label": "Hostname",
                        "value": "MailServer01"
                    },
                    {
                        "idx": 2,
                        "label": "IP Address",
                        "value": "192.168.0.1"
                    },
                    {
                        "idx": 3,
                        "label": "Disk Capacity Percentage",
                        "value": 95
                    }
                ]
            },
            "+dummy": 1,
            "instructions": [
                "Remove unused software",
                "Delete temporary files",
                "Use a drive-cleaner application",
                "Add additional hard-drive"
            ],
            "motd": "We are very excited to inform you about our new project that allows you to time-travel. Please refer the web-site below to find out more"
        }
    }"#;

    let entry_2 = r#"
    {
        "id": "50bf9e7zz",
        "utc": "2022-09-01T22:44:09.302646+00:00",
        "notify_error": [
            "Developers <dev-team@somemail.com>"
        ],
        "email": {
            "system": "MyExternalSystem",
            "subsystem": "[ID:12345] Trigger: Server Disk Out-of-Space",
            "from": "Mail System <tech-support@somemail.com>",
            "to": [
                "Rick S. <someone@somemail.com>"
            ],
            "cc": [],
            "bcc": [],
            "reply_to": [
                "System Admin <admin@somemail.com>",
                "Project Lead <lead@somemail.com>"
            ],
            "subject": "Warning: Your server's disk is out-of-space",
            "template": "ops_department",
            "alternative_content": "Unable to render HTML. Please refer to the Ops department for details.",
            "attachments": [
                "guides/disk-capacity-guidelines.pdf"
            ],
            "custom_key": ""
        },
        "context": {
            "message": {
                "head": "Detected Problems in Your Server",
                "body": "We have detected a disk capacity problem with one or more of your servers. Please refer to the instructions below"
            },
            "table": {
                "type": 1,
                "+entries": [
                    {
                        "idx": 1,
                        "label": "Hostname",
                        "value": "MailServer02"
                    },
                    {
                        "idx": 2,
                        "label": "IP Address",
                        "value": "192.168.0.2"
                    },
                    {
                        "idx": 3,
                        "label": "Disk Capacity Percentage",
                        "value": 87
                    }
                ]
            },
            "+dummy": 2,
            "instructions": [
                "Remove unused software",
                "Delete temporary files",
                "Use a drive-cleaner application",
                "Add additional hard-drive"
            ],
            "motd": "We are very excited to inform you about our new project that allows you to time-travel. Please refer the web-site below to find out more"
        }
    }"#;

    let entry_3 = r#"
    {
        "id": "50bf9e7zzv",
        "utc": "2022-09-01T22:44:10.302646+00:00",
        "notify_error": [
            "Developers <dev-team@somemail.com>"
        ],
        "email": {
            "system": "MyExternalSystem",
            "subsystem": "[ID:12345] Trigger: Server Disk Out-of-Space",
            "from": "Mail System <tech-support@somemail.com>",
            "to": [
                "Dave. K <dikaveman@somemail.com>"
            ],
            "cc": [],
            "bcc": [],
            "reply_to": [
                "System Admin <admin@somemail.com>",
                "Project Lead <lead@somemail.com>"
            ],
            "subject": "Warning: Your server's disk is out-of-space",
            "template": "ops_department",
            "alternative_content": "Unable to render HTML. Please refer to the Ops department for details.",
            "attachments": [
                "guides/disk-capacity-guidelines.pdf"
            ],
            "custom_key": ""
        },
        "context": {
            "message": {
                "head": "Detected Problems in Your Server",
                "body": "We have detected a disk capacity problem with one or more of your servers. Please refer to the instructions below"
            },
            "table": {
                "type": 1,
                "+entries": [
                    {
                        "idx": 1,
                        "label": "Hostname",
                        "value": "GameServer01"
                    },
                    {
                        "idx": 2,
                        "label": "IP Address",
                        "value": "172.14.0.2"
                    },
                    {
                        "idx": 3,
                        "label": "Disk Capacity Percentage",
                        "value": 99
                    }
                ]
            },
            "+dummy": 2,
            "instructions": [
                "Remove unused software",
                "Delete temporary files",
                "Use a drive-cleaner application",
                "Add additional hard-drive"
            ],
            "motd": "We are very excited to inform you about our new project that allows you to time-travel. Please refer the web-site below to find out more"
        }
    }"#;

    vec![
        serde_json::from_str(entry_1).unwrap(),
        serde_json::from_str(entry_2).unwrap(),
        serde_json::from_str(entry_3).unwrap(),
    ]
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
    // FIXME: Make sure the end result doesn't have the `+` key
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
                    n: (value_vec.len() + 1) as u32,
                    v: v.clone(),
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
            .expect("The vector was created empty when inserted to the map.");

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
    let entries_pool = load_files();
    let emails_map = map_emails(entries_pool); // Each E-Mail ID with its E-mail contents, in order

    let res = compose_emails(&emails_map);

    println!("res = {res:#?}");

    Ok(())
}
