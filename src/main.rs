#[macro_use]
extern crate lazy_static;

use anyhow::{anyhow, Context, Result};
use std::{env, fs, rc::Rc};

use crate::render::{ContextData, TemplateData};

// https://stackoverflow.com/questions/65356683/how-to-mutate-serde-json-value-by-adding-additional-fields

mod entries;
mod errors;
mod render;

const ENTRY_DIR: &str = "entries";
const ENTRY_EXT: &str = ".json";
const TEMPLATE_DIR: &str = "templates";

fn main() -> anyhow::Result<()> {
    let current_exe =
        env::current_exe().expect("Unable to get the current binary file from the OS.");
    let current_exe_dir = current_exe
        .parent()
        .expect("Unable to get current binary file directory");

    let entries_path = current_exe_dir.join(ENTRY_DIR);

    let entry_parse_results = entries::load_entries(entries_path, ENTRY_EXT);

    eprintln!("Entry parsing errors: {:?}", entry_parse_results.err);

    let entries_pool = entry_parse_results.ok;

    let emails_map = entries::map_emails(entries_pool); // Each E-Mail ID with its E-mail contents, in order

    let composed_emails = entries::compose_emails(&emails_map);

    // println!("composed_emails = {composed_emails:#?}");

    let templates_path = current_exe_dir.join(TEMPLATE_DIR);

    for email in composed_emails {
        let email_template_path: render::AbsolutePath = templates_path
            .join(&email.header.template)
            .join("template.html")
            .into();

        let template_context = &email.context;

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
            context: serde_json::Value::Object(template_context.clone()),
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
                println!("Rendered Template: \n{html_payload}");
                // TODO: Send E-mail
            }

            Err(e) => {
                eprintln!("{:?}", e);
                continue;
            }
        }
    } // Each E-mail

    Ok(())
}
