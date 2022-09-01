mod entries;
mod errors;

use std::{any::Any, collections::HashMap};

use entries::crc32_iso_hdlc_checksum;
use errors::EntryError;
use serde::{Deserialize, Serialize};

use chrono::DateTime;

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
#[derive(Serialize, Deserialize, Debug)]
struct Entry {
    id: String,
    utc: String,
    notify_error: Vec<String>,
    email: Email,
    template: serde_json::Value,
}

impl Entry {
    pub fn email_id(&self) -> u32 {
        let email_string = serde_json::to_string(&self.email).expect("This would be a paradox.");
        crc32_iso_hdlc_checksum(email_string.as_bytes())
    }
}

fn main() -> anyhow::Result<()> {
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
        "template": {
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
        "id": "50bf9e7e",
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
        "template": {
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

    // serde_json::from_str(input).unwrap();
    let entries_pool: Vec<Entry> = vec![
        serde_json::from_str(entry_1).unwrap(),
        serde_json::from_str(entry_2).unwrap(),
    ];

    let mut emails: HashMap<u32, Vec<Entry>> = HashMap::new();

    // Accumulate entries of the same E-mail
    for entry in entries_pool {
        let email_id = entry.email_id();
        let entries = emails.entry(email_id).or_insert_with(Vec::new);
        entries.push(entry)
    }

    // Order entries by their UTC time
    for (_, value) in emails.iter_mut() {
        value.sort_by(|a, b| {
            let a_time = DateTime::parse_from_rfc3339(&a.utc).unwrap();
            let b_time = DateTime::parse_from_rfc3339(&b.utc).unwrap();
            a_time.cmp(&b_time)
        })
    }

    println!("Debug Emails: {emails:#?}");

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

    let mut entry_1_value: serde_json::Value = serde_json::from_str(entry_1).expect("msg");
    let mut entry_2_value: serde_json::Value = serde_json::from_str(entry_2).expect("msg");
    // let template = entry_1_value["template"].take();

    // println!("{template:#}");
    // println!("{}", template["instructions"]);

    let entry_1 = entries::Entry::try_from(&entry_1_value)?;
    let entry_2 = entries::Entry::try_from(&entry_2_value)?;

    assert_eq!(entry_1.email.id.0, entry_2.email.id.0);

    println!("{entry_1:#?}");

    let entry = entries::Entry::try_from(&entry_1_value)?;
    println!("{entry:#?}");

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

    println!("{replacements:#?}");

    let t = serde_json::to_value(replacements.get("+entries").unwrap())
        .expect("Failed to create value XD");

    println!("\n\n\n{t:#}");

    Ok(())
}
