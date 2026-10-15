use anyhow::{anyhow, Context, Result};
use enum_iterator::Sequence;
use handlebars::Handlebars;
use path_slash::PathExt;
use regex::RegexBuilder;
use std::{
    borrow::{Borrow, Cow},
    ffi::{OsStr, OsString},
    fs,
    ops::Deref,
    path::{Path, PathBuf},
    rc::Rc,
    str::FromStr,
};
use tera::Tera;

// TODO: Add feature: (function) Dynamic QRCode
// TODO: Add feature: (function) Defang Values
// TODO: Template configurations (default + selected) + support for zipped templates (Which could include license and other metadata)

// This function attempts to be ignorant about any problems.
// It just tries to figure out if a given file path location.
// If the path doesn't exists, it assumes someone else will scream about it.
// On failure, it just returns the original Path.
//
// Note: no file is ever created as a side effect (the old touch-fallback
// could leave an empty file behind and turn a missing template into a
// render of an empty file; plan gap G1-12).
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
        Err(_) => path,
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

            let templates_home_dir_glob = templates_home_dir_glob.join("*.html");

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

#[cfg(test)]
mod tests {
    use super::*;

    fn context(pairs: &[(&str, serde_json::Value)]) -> serde_json::Map<String, serde_json::Value> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    fn render_contents(
        contents: &str,
        file_path: Option<&AbsolutePath>,
        ctx: &[(&str, serde_json::Value)],
    ) -> Result<String> {
        let template_data = TemplateData {
            contents: Rc::new(contents.to_string()),
            file_path,
        };
        let context_data = ContextData {
            context: serde_json::Value::Object(context(ctx)),
            file_path: None,
        };
        render(
            &template_data,
            &context_data,
            DetectionMethod::Auto,
            TemplateExtension::Auto,
        )
        .map(|r| r.0.to_string())
    }

    #[test]
    fn magic_comment_detects_tera() {
        let rendered = render_contents(
            "<!--template tera-->Hello {{name}}",
            None,
            &[("name", serde_json::json!("World"))],
        )
        .expect("tera render");
        assert!(rendered.contains("Hello World"));
    }

    #[test]
    fn magic_comment_detects_handlebars() {
        let rendered = render_contents(
            "<!--template handlebars-->Hi {{name}}",
            None,
            &[("name", serde_json::json!("World"))],
        )
        .expect("handlebars render");
        assert!(rendered.contains("Hi World"));
    }

    #[test]
    fn magic_comment_detects_liquid() {
        let rendered = render_contents(
            "<!--template liquid-->Hi {{ name }}",
            None,
            &[("name", serde_json::json!("World"))],
        )
        .expect("liquid render");
        assert!(rendered.contains("Hi World"));
    }

    #[test]
    fn no_magic_comment_is_passed_through() {
        let rendered = render_contents("plain {{name}}", None, &[])
            .expect("no-engine render");
        assert_eq!(rendered.trim(), "plain {{name}}");
    }

    #[test]
    fn unknown_magic_comment_engine_errors() {
        let result = render_contents("<!--template pug-->x", None, &[]);
        assert!(result.is_err());
    }

    #[test]
    fn file_extension_selects_engine() {
        let dir = std::env::temp_dir().join(format!("osa_mailer_render_ext_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let p: AbsolutePath = dir.join("t.hbs").as_path().into();
        let rendered = render_contents("Hi {{name}}", Some(&p), &[("name", serde_json::json!("World"))])
            .expect("hbs render");
        assert!(rendered.contains("Hi World"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn forced_engine_none_is_passthrough() {
        let template_data = TemplateData {
            contents: Rc::new("raw {{name}}".to_string()),
            file_path: None,
        };
        let context_data = ContextData {
            context: serde_json::Value::Object(context(&[])),
            file_path: None,
        };
        let rendered = render(
            &template_data,
            &context_data,
            DetectionMethod::Force(TemplateEngine::None),
            TemplateExtension::Auto,
        )
        .expect("forced none");
        assert_eq!(rendered.0.as_str(), "raw {{name}}");
    }

    #[test]
    fn canonicalizing_a_missing_path_creates_no_file() {
        // Plan gap G1-12: the old touch-fallback could create (and possibly
        // leave behind) a file just to resolve a path.
        let dir =
            std::env::temp_dir().join(format!("osa_mailer_render_touch_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let missing = dir.join("never_created.html");
        let _abs: AbsolutePath = missing.as_path().into();

        assert!(!missing.exists(), "canonicalize must not create files");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn tera_renders_with_template_directory_on_disk() {
        // The documented layout: `templates/<name>/template.html` with a
        // magic comment selecting Tera (as in the shipped
        // `templates/ops_department/template.html`).
        let dir =
            std::env::temp_dir().join(format!("osa_mailer_render_tera_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("tpl")).unwrap();

        let p: AbsolutePath = dir.join("tpl").join("template.html").as_path().into();
        let rendered = render_contents(
            "<!--template tera--><p>{{name}}</p>",
            Some(&p),
            &[("name", serde_json::json!("World"))],
        )
        .expect("tera render with dir");
        assert!(rendered.contains("<p>World</p>"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn html_without_magic_comment_is_passed_through() {
        // Documented current contract: without a recognized engine
        // extension or magic comment the template is used as-is. (The
        // template manifest will pin the engine explicitly, plan Phase 4.)
        let dir =
            std::env::temp_dir().join(format!("osa_mailer_render_raw_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("tpl")).unwrap();

        let p: AbsolutePath = dir.join("tpl").join("template.html").as_path().into();
        let rendered = render_contents("<p>{{name}}</p>", Some(&p), &[("name", serde_json::json!("World"))])
            .expect("render");
        assert!(rendered.contains("{{name}}"), "no engine must not alter the template");

        let _ = fs::remove_dir_all(&dir);
    }
}
