#[macro_use]
extern crate lazy_static;

use anyhow::{anyhow, Context, Result};
use lettre::transport::smtp::authentication::Credentials;
use secstr::SecUtf8;
use std::{env, fs, rc::Rc};

use crate::render::{ContextData, TemplateData};

// https://stackoverflow.com/questions/65356683/how-to-mutate-serde-json-value-by-adding-additional-fields

mod entries;
mod errors;
mod render;
mod send;

const ENTRY_DIR: &str = "entries";
const ENTRY_EXT: &str = ".json";
const TEMPLATE_DIR: &str = "templates";

fn main() -> anyhow::Result<()> {
    let current_exe =
        env::current_exe().context("Unable to get the current binary file from the OS.")?;
    let current_exe_dir = current_exe
        .parent()
        .context("Unable to get current binary file directory")?;

    let entries_path = current_exe_dir.join(ENTRY_DIR);

    let entry_parse_results = entries::load_entries(entries_path, ENTRY_EXT);

    eprintln!("Entry parsing errors: {:?}", entry_parse_results.err);

    let entries_pool = entry_parse_results.ok;

    let emails_map = entries::map_emails(&entries_pool); // Each E-Mail ID with its E-mail contents, in order

    let composed_emails = entries::compose_emails(&emails_map);

    println!(
        "composed_emails = {}",
        serde_json::to_string_pretty(&composed_emails).unwrap()
    );

    let templates_path = current_exe_dir.join(TEMPLATE_DIR);

    // TODO: Make static and use CLI ARGUMENTS instead
    let server = env::var("SERVER").unwrap_or_else(|_| "localhost".to_string());
    let port: u16 = env::var("PORT")
        .unwrap_or_else(|_| "25".to_string())
        .parse()?;

    let auth: send::Authentication = env::var("AUTH")
        .unwrap_or_else(|_| "noauth".to_string())
        .parse()?;

    // Establish one connection to send all E-mails
    println!("Mail-Relay: \"{server}:{port}\" [{auth}]");
    let mut connection = send::Connection::new(&server, port, auth);

    if let (Ok(username), Ok(password)) = (env::var("USERNAME"), env::var("PASSWORD")) {
        connection.establish(Some(Credentials::new(username, password)))
    } else {
        connection.establish(None);
    }

    for email in composed_emails {
        let email_template_images_root = templates_path.join(&email.header.template);

        let email_template_path: render::AbsolutePath =
            email_template_images_root.join("template.html").into();

        let template_data = TemplateData {
            contents: {
                let contents = fs::read_to_string(&email_template_path).with_context(|| {
                    format!(
                        "Unable to load template file \"{}\"",
                        email_template_path.display()
                    )
                })?;
                Rc::new(contents)
            },
            file_path: { Some(&email_template_path) },
        };

        let context_data = ContextData {
            context: serde_json::Value::Object(email.context.clone()),
            file_path: None,
        };

        let rendered_template_result = render::render(
            &template_data,
            &context_data,
            render::DetectionMethod::Auto,
            render::TemplateExtension::Auto,
        );

        match rendered_template_result {
            Ok(rendered_template) => {
                let html_payload = rendered_template.0.clone();

                let to = email.header.to.join(", ");
                let cc = email.header.cc.join(", ");
                let bcc = email.header.bcc.join(", ");
                let reply_to = email.header.reply_to.join(", ");
                let attachments = email.header.attachments.join(", ");

                // Send E-mail
                let message = send::Message::new()
                    .from(&email.header.from)
                    .to_addresses(&to)
                    .cc_addresses(&cc)
                    .bcc_addresses(&bcc)
                    .reply_to_addresses(&reply_to)
                    .subject(&email.header.subject)
                    .alternative_content(&email.header.alternative_content)
                    .content(&html_payload, Some(&email_template_images_root))
                    .attachments(&attachments);

                // Lower privilege.
                // let connection = connection;
                match connection.send(message.into()) {
                    Ok(_) => {
                        println!("Email sent successfully!");

                        // Get E-mail ID, retrieve its Entries and remove them
                        if let Some(email_entries) = emails_map.get(&email.id) {
                            for entry in email_entries {
                                if let Some(ref entry_path) = entry.path {
                                    let _ = fs::remove_file(entry_path);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("{e}");
                        continue;
                    }
                }
            }

            Err(e) => {
                eprintln!("{:?}", e);
                continue;
            }
        }
    } // Each E-mail

    Ok(())
}
