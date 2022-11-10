mod entries;
mod errors;

use std::{
    any::Any,
    collections::{BTreeMap, HashMap},
};

use entries::crc32_iso_hdlc_checksum;
use errors::EntryError;
use serde::{Deserialize, Serialize};

use chrono::{DateTime, FixedOffset};

#[derive(Serialize, Debug)]
struct AccumulatedValue<'json_entry> {
    idx: u32,
    // items: Vec<&'json_entry serde_json::Value>,
    items: &'json_entry serde_json::Value,
}

#[derive(Serialize, Deserialize, Debug)]
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
#[derive(Serialize, Deserialize, Debug)]
struct ComposedEmail {
    header: Email,
    context: serde_json::Map<String, serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug)]
struct Entry {
    id: String,
    // utc: String,
    // #[serde(with = )]
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

    vec![
        serde_json::from_str(entry_1).unwrap(),
        serde_json::from_str(entry_2).unwrap(),
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

fn compose_emails(email_entries: &EmailEntries) -> Vec<ComposedEmail> {
    let composed_emails: Vec<ComposedEmail>;

    for (email_id, entries) in email_entries {}
    todo!()
}

fn main() -> anyhow::Result<()> {
    let entries_pool = load_files();
    let emails_map = map_emails(entries_pool); // Each E-Mail ID with its E-mail contents, in order
    println!("Debug Emails: {emails_map:#?}");

    // TODO: 1. Build the structure around E-mail details by ID
    // TODO: 2. Merge `Accumulated` for the Vec of each E-mail ID

    // https://stackoverflow.com/questions/65356683/how-to-mutate-serde-json-value-by-adding-additional-fields
    // pub fn merge(v: &Value, fields: &HashMap<String, String>) -> Value {
    //     match v {
    //         Value::Object(m) => {
    //             let mut m = m.clone();
    //             for (k, v) in fields {
    //                 m.insert(k.clone(), Value::String(v.clone()));
    //             }
    //             Value::Object(m)
    //         }
    //         v => v.clone(),
    //     }
    // }

    // let mut entry_1_value: serde_json::Value = serde_json::from_str(entry_1).expect("msg");
    // let mut entry_2_value: serde_json::Value = serde_json::from_str(entry_2).expect("msg");
    // let template = entry_1_value["template"].take();

    // println!("{template:#}");
    // println!("{}", template["instructions"]);

    // let entry_1 = entries::Entry::try_from(&entry_1_value)?;
    // let entry_2 = entries::Entry::try_from(&entry_2_value)?;

    // assert_eq!(entry_1.email.id.0, entry_2.email.id.0);

    // println!("{entry_1:#?}");

    // let entry = entries::Entry::try_from(&entry_1_value)?;
    // println!("{entry:#?}");

    let mut replacements = HashMap::<&str, Vec<AccumulatedValue>>::new();

    let t = HashMap::<Vec<&'static str>, &'static str>::new();
    fn scan_accumulations_into<'json_entry>(
        object_value: &'json_entry serde_json::Map<String, serde_json::Value>,
        replacements: &mut HashMap<&'json_entry str, Vec<AccumulatedValue<'json_entry>>>,
    ) {
        for (key, value) in object_value {
            if key.starts_with('+') {
                let value_vec = replacements.entry(key).or_insert_with(Vec::new);

                value_vec.push(AccumulatedValue {
                    idx: (value_vec.len() + 1) as u32,
                    items: value,
                });
            } else if let Some(object) = value.as_object() {
                scan_accumulations_into(object, replacements);
            }
        }
    }

    // scan_accumulations_into(template.as_object().unwrap(), &mut replacements);
    // scan_accumulations_into(template.as_object().unwrap(), &mut replacements);

    println!("Replacement:\n{replacements:#?}");

    // let t = serde_json::to_value(replacements.get("+entries").unwrap())
    //     .expect("Failed to create value XD");

    // println!("\n\n\n{t:#}");

    // let acc_object: serde_json::Value = serde_json::Value::Object(())
    // let acc_value: serde_json::Value = serde_json::Value::Object(Map<String, Value>);
    // let test_value: serde_json::Value = AccumulatedValue {
    //     idx: todo!(),
    //     items: todo!(),
    // }
    // .into();

    // fn scan_map(
    //     object_value: &serde_json::Map<String, serde_json::Value>,
    //     target_object: &mut serde_json::Map<String, serde_json::Value>,
    // ) {
    //     for (key, value) in object_value {
    //         let v = match value {
    //             serde_json::Value::Null => todo!(),
    //             serde_json::Value::Bool(_) => todo!(),
    //             serde_json::Value::Number(_) => todo!(),
    //             serde_json::Value::String(_) => todo!(),
    //             serde_json::Value::Array(v) => {
    //                 for i in v {
    //                     // scan_map(i, target_object)
    //                 }
    //                 todo!()
    //             }
    //             serde_json::Value::Object(m) => scan_map(m, target_object),
    //         };
    //         println!("Cloning key {key} = {v:?}\n");
    //         target_object.insert(key.clone(), v);

    //         if let serde_json::Value::Object(m) = value {
    //             scan_map(m, target_object)
    //         }
    //         target_object.insert(key.clone(), value.clone());
    //     }

    // fn process_value(
    //     value: &serde_json::Value,
    //     target_object: &mut serde_json::Map<String, serde_json::Value>,
    // ) {
    //     match value {
    //         serde_json::Value::Null => todo!(),
    //         serde_json::Value::Bool(_) => todo!(),
    //         serde_json::Value::Number(_) => todo!(),
    //         serde_json::Value::String(_) => todo!(),
    //         serde_json::Value::Array(v) => {
    //             for i in v {
    //                 process_value(i, target_object)
    //             }
    //         }
    //         serde_json::Value::Object(m) => {
    //             for (k, v) in m {
    //                 // TODO: Check for `+`
    //                 process_value(v, target_object)
    //             }
    //         }
    //     }
    // }

    // let map_val = serde_json::to_value(emails_map).unwrap();
    // if let serde_json::Value::Object(m) = map_val {
    //     let mut new_map = serde_json::Map::new();
    //     scan_map(&m, &mut new_map);
    // }

    // let _ = copy_map();

    Ok(())
}
