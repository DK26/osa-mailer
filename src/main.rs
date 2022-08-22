use std::path::{Path, PathBuf};

use crc::{Algorithm, Crc, CRC_32_ISO_HDLC};

// CRC_32_ISO_HDLC is compatible with Python 3
const CRC32_ALGORITHM: Algorithm<u32> = CRC_32_ISO_HDLC;

#[derive(Debug)]
struct Email<'json_entry> {
    id: String, // Based off `email` key
    system: &'json_entry str,
    subsystem: &'json_entry str,
    from: &'json_entry str,
    to: Vec<&'json_entry str>,
    cc: Vec<&'json_entry str>,
    bcc: Vec<&'json_entry str>,
    reply_to: Vec<&'json_entry str>,
    subject: &'json_entry str,
    template: &'json_entry str,
    alternative_content: &'json_entry str,
    attachments: Vec<&'json_entry Path>,
    // custom_key: &'json_entry str,
}

#[derive(thiserror::Error, Debug)]
enum EntryError {
    #[error("Entry does not contain `email` section")]
    MissingEmailSection,

    #[error("The `email` section is missing the `{0}` field")]
    MissingField(&'static str),

    #[error("The field `{0}` is containing a wrong type")]
    WrongFieldType(&'static str),

    #[error("Wrong item type in array `{0}`")]
    WrongArrayItem(&'static str),
}

impl<'json_entry> TryFrom<&'json_entry serde_json::Value> for Email<'json_entry> {
    type Error = EntryError;

    fn try_from(value: &'json_entry serde_json::Value) -> Result<Self, Self::Error> {
        let email = value.get("email").ok_or(EntryError::MissingEmailSection)?;

        // let system_value = email.get("system").ok_or(EntryError::MissingField("system"))?;

        let system = get_str_value(email, "system")?;
        let subsystem = get_str_value(email, "subsystem")?;
        let from = get_str_value(email, "from")?;

        let to = get_str_vec_value(email, "to")?;
        let cc = get_str_vec_value(email, "cc")?;
        let bcc = get_str_vec_value(email, "bcc")?;
        let reply_to = get_str_vec_value(email, "reply_to")?;

        let subject = get_str_value(email, "subject")?;
        let template = get_str_value(email, "template")?;

        let alternative_content = get_str_value(email, "alternative_content")?;

        let attachments = get_path_vec_value(email, "attachments")?;

        let email_checksum = crc32_iso_hdlc_checksum(email.to_string().as_bytes());
        let id = format!("{:x}", email_checksum);
        let new_email = Email {
            id,
            system,
            subsystem,
            from,
            to,
            cc,
            bcc,
            reply_to,
            subject,
            template,
            alternative_content,
            attachments,
        };

        Ok(new_email)
    }
}

struct NotifyError(Vec<String>);

struct Entry {
    id: String,
    utc: String,
    template: serde_json::Value,
}

/// Returns a checksum calculated with CRC32 using the ISO HDLC algorithm for compatibility with Python.
fn crc32_iso_hdlc_checksum(bytes: &[u8]) -> u32 {
    let crc: Crc<u32> = Crc::<u32>::new(&CRC32_ALGORITHM);
    crc.checksum(bytes)
}

fn get_str_value<'json_entry>(
    value: &'json_entry serde_json::Value,
    key: &'static str,
) -> Result<&'json_entry str, EntryError> {
    let result = if let serde_json::Value::String(v) =
        value.get(key).ok_or(EntryError::MissingField(key))?
    {
        v
    } else {
        return Err(EntryError::WrongFieldType(key));
    };
    Ok(result)
}

fn get_str_vec_value<'json_entry>(
    value: &'json_entry serde_json::Value,
    key: &'static str,
) -> Result<Vec<&'json_entry str>, EntryError> {
    value
        .get(key)
        .ok_or(EntryError::MissingField(key))?
        .as_array()
        .ok_or(EntryError::WrongFieldType(key))?
        .iter()
        .map(|v| {
            if let serde_json::Value::String(ref iv) = v {
                Ok(iv.as_str())
            } else {
                Err(EntryError::WrongArrayItem(key))
            }
        })
        .collect()
}

fn get_path_vec_value<'json_entry>(
    value: &'json_entry serde_json::Value,
    key: &'static str,
) -> Result<Vec<&'json_entry Path>, EntryError> {
    value
        .get(key)
        .ok_or(EntryError::MissingField(key))?
        .as_array()
        .ok_or(EntryError::WrongFieldType(key))?
        .iter()
        .map(|v| {
            if let serde_json::Value::String(ref iv) = v {
                Ok(iv.as_ref())
            } else {
                Err(EntryError::WrongArrayItem(key))
            }
        })
        .collect()
}

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

    let email = Email::try_from(&all)?;

    println!("{email:#?}");

    Ok(())
}
