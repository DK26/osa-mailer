mod errors;
mod parsing;

use std::collections::HashMap;

use errors::EntryError;
use serde_json::{json, map::Values, Map};

fn main() -> anyhow::Result<()> {
    let entry = r#"
    {
        "id": "50bf9e7e",
        "utc": "2022-08-11T15:12:59.995532",
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
            "instructions": [
                "Remove unused software",
                "Delete temporary files",
                "Use a drive-cleaner application",
                "Add additional hard-drive"
            ],
            "motd": "We are very excited to inform you about our new project that allows you to time-travel. Please refer the web-site below to find out more"
        }
    }"#;

    let mut all: serde_json::Value = serde_json::from_str(entry).expect("msg");
    let template = all["template"].take();
    // let template = &all["template"];
    println!("{template:#}");
    println!("{}", template["instructions"]);

    let email = parsing::Email::try_from(&all)?;

    println!("{email:#?}");

    let entry = parsing::Entry::try_from(&all)?;
    println!("{entry:#?}");

    let mut replacements = HashMap::<&str, Vec<&serde_json::Value>>::new();

    fn scan_accumulations_into<'json_entry>(
        object_value: &'json_entry Map<String, serde_json::Value>,
        replacements: &mut HashMap<&'json_entry str, Vec<&'json_entry serde_json::Value>>,
    ) {
        for (key, value) in object_value {
            if key.starts_with('+') {
                let value_vec = replacements.entry(key).or_insert_with(Vec::new);
                value_vec.push(value);
            } else if let Some(object) = value.as_object() {
                scan_accumulations_into(object, replacements);
            }
        }
    }

    scan_accumulations_into(template.as_object().unwrap(), &mut replacements);
    scan_accumulations_into(template.as_object().unwrap(), &mut replacements);

    println!("{replacements:#?}");
    //     let x = json!({"idx": "1", "items": []});

    Ok(())
}
