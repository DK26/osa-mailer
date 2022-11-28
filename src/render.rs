use anyhow::{anyhow, Context, Result};
use enum_iterator::Sequence;
use handlebars::Handlebars;
use path_slash::PathExt;
use regex::{Regex, RegexBuilder};
use std::{
    borrow::{Borrow, Cow},
    ffi::{OsStr, OsString},
    fs::{self, OpenOptions},
    ops::Deref,
    path::{Path, PathBuf},
    rc::Rc,
    str::FromStr,
};
use tera::Tera;

// A simple implementation of `% touch path` (ignores existing files)
// Inspired by: https://doc.rust-lang.org/rust-by-example/std_misc/fs.html
fn touch<P: AsRef<Path>>(path: P) -> Result<()> {
    OpenOptions::new().create(true).write(true).open(path)?;
    Ok(())
}

// This function attempts to be ignorant about any problems.
// It just tries to figure out if a given file path location.
// If the path doesn't exists, it assumes someone else will scream about it.
// On failure, it just returns the original Path.
#[inline]
fn new_canonicalize_path_buf<P: AsRef<Path>>(path: P) -> PathBuf {
    // Canonicalize seem to be having trouble on Windows with relative paths that include a backslash.
    // This work around is meant to make sure that before Canonicalize encounters the given path,
    // its backslashes will be replaced with regular ones so `canonicalize` will be able to handle it.
    let path: PathBuf = if path.as_ref().has_root() {
        path.as_ref().into()
    } else {
        (&*path.as_ref().to_slash_lossy()).into()
    };

    match fs::canonicalize(&path) {
        Ok(abs_path) => abs_path,
        // On failure of getting the full path, keep the relative path.
        //
        // Possible failures of `fs::canonicalize`:
        //  1. path does not exist.
        //  2. A non-final component in path is not a directory.
        Err(e) => match e.kind() {
            std::io::ErrorKind::NotFound => match touch(&path) {
                Ok(_) => {
                    let res = new_canonicalize_path_buf(&path);
                    match fs::remove_file(&res) {
                        Ok(_) => {
                            log::debug!(
                                "canonicalize(): Removed touched file: \"{}\"",
                                res.to_string_lossy()
                            )
                        }
                        Err(_) => {
                            log::error!(
                                "canonicalize(): Unable to remove file after touch: \"{}\"",
                                res.to_string_lossy()
                            )
                        }
                    };
                    res
                }
                Err(_) => path,
            },
            _ => path,
        },
    }
}

// TODO: Move to an external crate, improve and with some more ideas and publish on crates.io.
// TODO: `AbsolutePath` features should be implemented on `PathBuf` directly with proper traits, to avoid duplicating and interswitching between the types, making it seamless.
// Old Note: Should behave just like a `PathBuf` and therefore should have the same methods + New security features (Restrict trait?)
#[derive(Clone, Debug)]
pub(crate) struct AbsolutePath {
    path: PathBuf,
}

impl AsRef<Path> for AbsolutePath {
    #[inline]
    fn as_ref(&self) -> &Path {
        self.path.as_ref()
    }
}

impl<T: ?Sized + AsRef<OsStr>> From<&T> for AbsolutePath {
    /// Converts a borrowed [`OsStr`] to a [`AbsolutePath`].
    ///
    /// Allocates a [`AbsolutePath`] and copies the data into it.
    #[inline]
    fn from(s: &T) -> AbsolutePath {
        AbsolutePath {
            // path: new_full_path_buf(s.as_ref()).unwrap_or_else(|_| s.into()),
            path: new_canonicalize_path_buf(s.as_ref()),
        }
    }
}

impl From<OsString> for AbsolutePath {
    #[inline]
    fn from(s: OsString) -> Self {
        AbsolutePath {
            // path: new_full_path_buf(&s).unwrap_or_else(|_| s.into()),
            path: new_canonicalize_path_buf(s),
        }
    }
}

impl From<PathBuf> for AbsolutePath {
    #[inline]
    fn from(s: PathBuf) -> Self {
        AbsolutePath {
            // path: new_full_path_buf(&s).unwrap_or(s),
            path: new_canonicalize_path_buf(s),
        }
    }
}

impl FromStr for AbsolutePath {
    type Err = anyhow::Error;

    #[inline]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let res = AbsolutePath {
            // path: new_full_path_buf(s).unwrap_or_else(|_| s.into()),
            path: new_canonicalize_path_buf(s),
        };
        Ok(res)
    }
}

impl std::fmt::Display for AbsolutePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.path.to_string_lossy())
    }
}

// impl AsRef<Path> for &AbsolutePath {
//     #[inline]
//     fn as_ref(&self) -> &Path {
//         self.path.as_ref()
//     }
// }

impl AsRef<PathBuf> for AbsolutePath {
    #[inline]
    fn as_ref(&self) -> &PathBuf {
        &self.path
    }
}

impl AsRef<OsStr> for AbsolutePath {
    #[inline]
    fn as_ref(&self) -> &OsStr {
        self.path.as_ref()
    }
}

impl Borrow<Path> for AbsolutePath {
    #[inline]
    fn borrow(&self) -> &Path {
        self.path.borrow()
    }
}

impl Deref for AbsolutePath {
    type Target = Path;

    #[inline]
    fn deref(&self) -> &Path {
        self.path.deref()
    }
}

/// Scan the template for reference to other templates, such as:
/// `{% include %}`, `{% extend %}` or `{% import %}` calls
#[inline]
fn find_template_references<P: AsRef<Path>>(content: &str, cwd: Option<P>) -> Vec<AbsolutePath> {
    let re = Regex::new(
        r#"\{%\s+?(?:import|include|extend)\s+?"(?P<template>[a-zA-Z0-9.\-/\\_]+?)"\s.*?%\}"#,
    )
    .expect("Bad regex pattern.");

    let mut buf: Vec<AbsolutePath> = Vec::new();

    log::debug!("Scanning for template references...");

    for cap in re.captures_iter(content) {
        log::debug!("Detected reference: \"{}\"", &cap["template"]);
        // TODO: Make path relative to main template
        let path = if let Some(p) = &cwd {
            p.as_ref().with_file_name(&cap["template"]).into()
        } else {
            cap["template"].into()
        };

        buf.push(path);
    }
    buf
}

/// Supported template engines
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Sequence, strum_macros::Display)]
pub(crate) enum TemplateEngine {
    Tera,
    Liquid,
    Handlebars,
    None,
}

impl FromStr for TemplateEngine {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let res = match s.to_lowercase().as_str() {
            "tera" => TemplateEngine::Tera,
            "liquid" | "liq" => TemplateEngine::Liquid,
            "handlebars" | "hbs" => TemplateEngine::Handlebars,
            "none" => TemplateEngine::None,
            _ => {
                return Err(anyhow!(
                    "Please try one of the supported engines in `--engine-list`"
                ))
            }
        };
        Ok(res)
    }
}

// impl FromStr for TemplateEngine {
//     type Err = RenditError;

//     fn from_str(s: &str) -> Result<Self, Self::Err> {
//         let res = match s.to_lowercase().as_str() {
//             "tera" => TemplateEngine::Tera,
//             "liquid" | "liq" => TemplateEngine::Liquid,
//             "handlebars" | "hbs" => TemplateEngine::Handlebars,
//             "none" => TemplateEngine::None,
//             _ => return Err(RenditError::UnknownEngine(s.to_owned())),
//         };
//         Ok(res)
//     }
// }

pub fn rendered_path<P: AsRef<Path>>(input_path: P) -> PathBuf {
    let file_extension = input_path.as_ref().extension();

    match file_extension {
        Some(os_path_ext) => {
            let path_ext = os_path_ext.to_string_lossy().to_lowercase();

            if path_ext != "none" && path_ext.parse::<TemplateEngine>().is_ok() {
                input_path.as_ref().with_extension("")
            } else {
                let new_ext = format!("rendered.{path_ext}");
                input_path.as_ref().with_extension(new_ext)
            }
        }
        None => input_path.as_ref().with_extension(String::from("rendered")),
    }
}

impl From<&str> for Template {
    /// Inspect the String contents for a magic comment `<!--template engine_name-->`, and return the appropriate `Template` enum variation for rendering.
    fn from(contents: &str) -> Self {
        let re = RegexBuilder::new(r#"<!--template\s+(?P<engine>\w+)\s?-->"#)
            .case_insensitive(true)
            .build()
            .expect("Bad regex pattern.");

        let mut re_caps = re.captures_iter(contents);

        // We want to find only the first one without scanning the rest of the file
        let mut re_iter = re.find_iter(contents);

        if let Some(m) = re_iter.next() {
            let found_match = m.as_str();

            let contents = Rc::new(contents.replacen(found_match, "", 1).trim().to_owned());

            let cap = re_caps
                .next()
                .expect("Match without a capture? how is it possible?");

            let engine = cap["engine"].to_lowercase();

            log::debug!("Detected magic comment: `{engine}`");

            match engine.as_str() {
                "tera" => Template::Tera(contents),
                "hbs" | "handlebars" => Template::Handlebars(contents),
                "liq" | "liquid" => Template::Liquid(contents),
                unknown_engine => Template::Unknown(unknown_engine.to_owned(), contents),
            }
        } else {
            Template::NoEngine(Rc::new(contents.to_owned()))
        }
    }
}

impl<'arg> From<&TemplateData<'arg>> for Template {
    /// Loads a template file into a Template enum type.
    /// Decides on the engine type by first inspecting the file extension (`.tera`, `.hbs` or `.liq`).
    /// If no special extension is provided then the contents of the template are inspected for the magic comment `<!--TEMPLATE engine_name-->`.
    ///
    /// Engine Names: `tera`, `handlebars` or `hbs`, `liquid` or `liq`
    fn from(td: &TemplateData) -> Self {
        // Checking for template file extension to determine the template engine.
        // Notice the early returns.
        if let Some(template_file) = td.file_path {
            if let Some(extension) = template_file.extension() {
                // if let Some(ref extension) = template_file.parts.extension {
                let file_extension = &*extension.to_string_lossy();
                // let file_extension = extension.as_str();

                let contents = td.contents.clone();
                match file_extension {
                    "tera" => return Template::Tera(contents),
                    "hbs" => return Template::Handlebars(contents),
                    "liq" => return Template::Liquid(contents),
                    _ => {} // ignore unknown extensions
                };
            }
        }
        // Scan template contents for the magic comment to return the proper Template kind.
        (*td.contents.as_str()).into()
    }
}

pub(crate) struct TemplateData<'a> {
    pub(crate) contents: Rc<String>,
    pub(crate) file_path: Option<&'a AbsolutePath>,
}

// #[allow(unused)]
pub(crate) struct ContextData {
    pub(crate) context: serde_json::Value,
    pub(crate) file_path: Option<AbsolutePath>,
}

pub(crate) struct RenderedTemplate(pub(crate) Rc<String>);

pub(crate) enum DetectionMethod {
    Auto,
    Force(TemplateEngine),
}

impl From<Option<TemplateEngine>> for DetectionMethod {
    fn from(te: Option<TemplateEngine>) -> Self {
        match te {
            Some(engine) => DetectionMethod::Force(engine),
            None => DetectionMethod::Auto,
        }
    }
}

impl From<Option<&TemplateEngine>> for DetectionMethod {
    fn from(te: Option<&TemplateEngine>) -> Self {
        match te {
            Some(engine) => DetectionMethod::Force(*engine),
            None => DetectionMethod::Auto,
        }
    }
}

pub(crate) enum TemplateExtension<'a> {
    Auto,
    Force(&'a str),
}

impl<'a> From<Option<&'a String>> for TemplateExtension<'a> {
    fn from(s: Option<&'a String>) -> Self {
        match s {
            Some(ext) => TemplateExtension::Force(ext),
            None => TemplateExtension::Auto,
        }
    }
}

type Contents = Rc<String>;
type EngineName = String;

#[non_exhaustive]
enum Template {
    Tera(Contents),
    Handlebars(Contents),
    Liquid(Contents),
    Unknown(EngineName, Contents),
    NoEngine(Contents),
}

impl Template {
    fn get_engine(&self) -> &'static str {
        match self {
            Template::Tera(_) => "tera",
            Template::Handlebars(_) => "handlebars",
            Template::Liquid(_) => "liquid",
            Template::Unknown(_, _) => "unknown",
            Template::NoEngine(_) => "no_engine",
        }
    }
}

pub(crate) fn render<'a>(
    template_data: &'a TemplateData,
    context_data: &'a ContextData,
    engine_detection: DetectionMethod,
    template_extension: TemplateExtension,
) -> Result<RenderedTemplate> {
    // ) -> Result<RenderedTemplate<'a>> {
    // let default_language = "html";

    // let template_language = &*match template_data.file_path {
    //     Some(p) => match p.extension() {
    //         Some(ext) => ext.to_string_lossy(),
    //         None => Cow::Borrowed(default_language),
    //     },
    //     None => Cow::Borrowed(default_language),
    // };

    // let template_path = template_data.file_path.clone();

    let template = match engine_detection {
        DetectionMethod::Auto => {
            log::debug!("Detection method: Automatic");
            Template::from(template_data)
        }
        DetectionMethod::Force(engine) => {
            log::debug!("Detection method: Manual = `{engine}`");
            let contents = template_data.contents.clone();
            match engine {
                TemplateEngine::Tera => Template::Tera(contents),
                TemplateEngine::Liquid => Template::Liquid(contents),
                TemplateEngine::Handlebars => Template::Handlebars(contents),
                TemplateEngine::None => Template::NoEngine(contents),
            }
        }
    };

    log::debug!("Selected engine: `{}`", template.get_engine());

    let result = match template {
        Template::Tera(contents) => {
            let context = tera::Context::from_value(context_data.context.clone())
                .context("Tera rejected Context object.")?;

            // match Tera::one_off(&contents, &context, true) {
            //     Ok(rendered) => rendered,
            //     Err(e) => {
            //         if let Some(source) = e.source() {
            //             log::error!("{source}");
            //         }
            //         return Err(anyhow::Error::new(e).context("Unable to render template."));
            //     }
            // }

            let templates_root_file = if let Some(template_file) = template_data.file_path {
                Cow::Borrowed(template_file)
            } else {
                let abs_path: AbsolutePath = std::env::current_exe()
                    .context("Failed to get current exe path")?
                    .into();
                // Cow::Owned(abs_path.into_inner())
                Cow::Owned(abs_path)
            };

            let templates_home_dir = templates_root_file
                .parent()
                .context("Failed to get home directory")?;

            let templates_home_dir_glob = templates_home_dir.join("**");

            let templates_home_dir_glob = templates_home_dir_glob.join("*.*");

            let templates_home_dir_glob = templates_home_dir_glob.to_string_lossy();

            log::debug!("Tera templates path: {templates_home_dir_glob}");

            // TODO: Better to create an instance of `Tera::default()` and have a deep scan for the templates to add only the references ones into a HashSet, than to add every file in the template's directory.
            // let mut tera = Tera::default();

            // let template_references: Vec<(AbsolutePath, Option<String>)> =
            //     find_template_references(&contents, template_path)
            //         .into_iter()
            //         .map(|p| {
            //             let file_name = p.file_name().map(|fp| fp.to_string_lossy().to_string());
            //             (p, file_name)
            //         })
            //         .collect();

            // tera.add_template_files(template_references)
            //     .context("Tera failed loading partial template files")?;

            let mut tera =
                Tera::new(&templates_home_dir_glob).context("Unable to create Tera instance")?;

            // Force extension or auto detect (default `.html`)
            let template_type = if let TemplateExtension::Force(ext) = template_extension {
                log::debug!("Tera: Forcing extension \"{ext}\"");
                Cow::Borrowed(ext)
            } else if let Some(path) = template_data.file_path {
                match path.extension() {
                    Some(ext) => ext.to_string_lossy(),
                    None => Cow::Borrowed("html"),
                }
                // match path.parts.extension {
                //     Some(ref ext) => Cow::Borrowed(ext.as_str()),
                //     None => Cow::Borrowed("html"),
                // }
            } else {
                Cow::Borrowed("html")
            };

            log::debug!("Tera: Using extension \"{template_type}\"");
            let in_memory_template = format!("__in_memory__.{}", template_type);

            // Adds a virtual in-memory file for the main template. We need the `.html` extension to enforce HTML escaping.
            tera.add_raw_template(&in_memory_template, &contents)
                .context("Tera is unable to add the main template as raw template.")?;

            let rendered = tera
                .render(&in_memory_template, &context)
                .context("Tera is unable to render the template.")?;

            Rc::new(rendered)
        }
        Template::Handlebars(contents) => {
            let handlebars = Handlebars::new();
            let render = handlebars.render_template(&contents, &context_data.context);
            // match render {
            //     Ok(contents) => contents,
            //     Err(e) => {
            //         if let Some(source) = e.source() {
            //             if let Some(template_error) = source.downcast_ref::<TemplateError>() {
            //                 let template_error_string = format!("{template_error}");
            //                 pretty_print(&template_error_string, template_language)?;
            //             }
            //         }
            //         return Err(anyhow::Error::new(e).context("Unable to render template."));
            //     }
            // }
            let rendered = render.context("Handlebars is unable to render the template.")?;

            Rc::new(rendered)
        }
        Template::Liquid(contents) => {
            // TODO: Enable partials using `find_template_references()`
            let template = liquid::ParserBuilder::with_stdlib()
                .build()
                .context("Liquid is unable to build the parser.")?
                .parse(&contents);

            // let template = match template {
            //     Ok(t) => t,
            //     Err(e) => {
            //         let template_error_string = format!("{e}");
            //         pretty_print(&template_error_string, template_language)?;
            //         // eprintln!("{e}");
            //         return Err(anyhow::Error::new(e).context("Unable to parse template."));
            //     }
            // };
            let template = template.context("Liquid is unable to parse the template.")?;

            let globals = liquid::object!(&context_data.context);

            let rendered = template
                .render(&globals)
                .context("Liquid is unable to render the template.")?;

            Rc::new(rendered)
        }
        Template::Unknown(engine, _) => return Err(anyhow!("Unknown template engine: `{engine}`")),
        Template::NoEngine(raw) => raw,
    };
    Ok(RenderedTemplate(result))
}
