use super::EntryError;
use std::path::Path;

use crc::{Algorithm, Crc, CRC_32_ISO_HDLC};

// CRC_32_ISO_HDLC is compatible with Python 3
const CRC32_ALGORITHM: Algorithm<u32> = CRC_32_ISO_HDLC;

// TODO: 1. Read the Entry in its entirely and append to Vec of the same E-mail ID (Map<email_id, Vec<Entry>>)
// TODO: 2. Order the Vec per key by UTC (or file timestamp?)
// TODO: 3. Loop over the map for each Vec of entries for E-mail ID
// TODO: 4. Merge each Vec `context` context
// TODO: 5. Render template value with context
// TODO: 6. Have the final E-mail object sent

// #[derive(Debug, Clone)]
// pub struct EmailId(pub String);

// #[derive(Debug)]
// pub struct Context<'json_entry>(pub &'json_entry serde_json::Map<String, serde_json::Value>);

// impl<'json_entry> TryFrom<&'json_entry serde_json::Value> for Context<'json_entry> {
//     type Error = EntryError;

//     fn try_from(value: &'json_entry serde_json::Value) -> Result<Self, Self::Error> {
//         let context = get_obj_value(value, "context")?;
//         Ok(Context(context))
//     }
// }

// #[derive(Debug)]
// pub struct Email<'json_entry> {
//     pub id: EmailId, // Based off `email` key
//     system: &'json_entry str,
//     subsystem: &'json_entry str,
//     from: &'json_entry str,
//     to: Vec<&'json_entry str>,
//     cc: Vec<&'json_entry str>,
//     bcc: Vec<&'json_entry str>,
//     reply_to: Vec<&'json_entry str>,
//     subject: &'json_entry str,
//     // template: &'json_entry str,
//     alternative_content: &'json_entry str,
//     attachments: Vec<&'json_entry Path>,
//     // custom_key: &'json_entry str,
// }

// #[derive(Debug)]
// pub struct Entry<'json_entry> {
//     id: &'json_entry str,
//     utc: &'json_entry str,
//     notify_error: Vec<&'json_entry str>,
//     pub email: Email<'json_entry>,
//     pub context: Context<'json_entry>,
// }

// impl<'json_entry> TryFrom<&'json_entry serde_json::Value> for Entry<'json_entry> {
//     type Error = EntryError;

//     fn try_from(value: &'json_entry serde_json::Value) -> Result<Self, Self::Error> {
//         let id = get_str_value(value, "id")?;
//         let utc = get_str_value(value, "utc")?;
//         let notify_error = get_str_vec_value(value, "notify_error")?;

//         let entry = Entry {
//             id,
//             utc,
//             notify_error,
//             email: Email::try_from(value)?,
//             context: Context::try_from(value)?,
//         };

//         Ok(entry)
//     }
// }

// impl<'json_entry> TryFrom<&'json_entry serde_json::Value> for Email<'json_entry> {
//     type Error = EntryError;

//     fn try_from(value: &'json_entry serde_json::Value) -> Result<Self, Self::Error> {
//         let email = value.get("email").ok_or(EntryError::MissingEmailSection)?;

//         let system = get_str_value(email, "system")?;
//         let subsystem = get_str_value(email, "subsystem")?;
//         let from = get_str_value(email, "from")?;

//         let to = get_str_vec_value(email, "to")?;
//         let cc = get_str_vec_value(email, "cc")?;
//         let bcc = get_str_vec_value(email, "bcc")?;
//         let reply_to = get_str_vec_value(email, "reply_to")?;

//         let subject = get_str_value(email, "subject")?;

//         let alternative_content = get_str_value(email, "alternative_content")?;

//         let attachments = get_path_vec_value(email, "attachments")?;

//         let email_checksum = crc32_iso_hdlc_checksum(email.to_string().as_bytes());
//         let id = EmailId(format!("{:x}", email_checksum));
//         let new_email = Email {
//             id,
//             system,
//             subsystem,
//             from,
//             to,
//             cc,
//             bcc,
//             reply_to,
//             subject,
//             // template,
//             alternative_content,
//             attachments,
//         };

//         Ok(new_email)
//     }
// }

/// Returns a checksum calculated with CRC32 using the ISO HDLC algorithm for compatibility with Python.
pub fn crc32_iso_hdlc_checksum(bytes: &[u8]) -> u32 {
    let crc: Crc<u32> = Crc::<u32>::new(&CRC32_ALGORITHM);
    crc.checksum(bytes)
}

// fn get_str_value<'json_entry>(
//     value: &'json_entry serde_json::Value,
//     key: &'static str,
// ) -> Result<&'json_entry str, EntryError> {
//     let result = if let serde_json::Value::String(v) =
//         value.get(key).ok_or(EntryError::MissingField(key))?
//     {
//         v
//     } else {
//         return Err(EntryError::WrongFieldType(key));
//     };
//     Ok(result)
// }

// fn get_obj_value<'json_entry>(
//     value: &'json_entry serde_json::Value,
//     key: &'static str,
// ) -> Result<&'json_entry serde_json::Map<String, serde_json::Value>, EntryError> {
//     let result = if let serde_json::Value::Object(v) =
//         value.get(key).ok_or(EntryError::MissingField(key))?
//     {
//         v
//     } else {
//         return Err(EntryError::WrongFieldType(key));
//     };
//     Ok(result)
// }

//// Returns a Vec containing `&str` to a `Value`'s array Strings.
// fn get_str_vec_value<'json_entry>(
//     value: &'json_entry serde_json::Value,
//     key: &'static str,
// ) -> Result<Vec<&'json_entry str>, EntryError> {
//     value
//         .get(key)
//         .ok_or(EntryError::MissingField(key))?
//         .as_array()
//         .ok_or(EntryError::WrongFieldType(key))?
//         .iter()
//         .map(|v| {
//             if let serde_json::Value::String(ref iv) = v {
//                 Ok(iv.as_str())
//             } else {
//                 Err(EntryError::WrongArrayItem(key))
//             }
//         })
//         .collect()
// }

//// Returns a Vec containing `&Path` to a `Value`'s array Strings.
// fn get_path_vec_value<'json_entry>(
//     value: &'json_entry serde_json::Value,
//     key: &'static str,
// ) -> Result<Vec<&'json_entry Path>, EntryError> {
//     value
//         .get(key)
//         .ok_or(EntryError::MissingField(key))?
//         .as_array()
//         .ok_or(EntryError::WrongFieldType(key))?
//         .iter()
//         .map(|v| {
//             if let serde_json::Value::String(ref iv) = v {
//                 Ok(iv.as_ref())
//             } else {
//                 Err(EntryError::WrongArrayItem(key))
//             }
//         })
//         .collect()
// }

// from:
// +entries: [ { .. }, { .. } ]

// to:
// entries: [
//  { idx: N, items: [ { .. }, { .. } ] }
// ]

// from:
// +entries: [ { .. }, { .. } ]

// to:
// entries: [
//  { idx: N, items: [ { .. }, { .. } ] },
//  { idx: N, items: [ { .. }, { .. } ] }
// ]

// replace `+entries` with new `entries`
