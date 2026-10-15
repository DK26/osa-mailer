use lazy_static::lazy_static;

use anyhow::{anyhow, Context, Result};
use lettre::address::AddressError;
use lettre::message::Message as LettreMessage;
use lettre::message::MessageBuilder as LettreMessageBuilder;
use lettre::message::{header, Attachment, Body, MultiPart, SinglePart};
use lettre::{SmtpTransport, Transport};

use lettre::transport::smtp::authentication::Credentials;
use regex::Regex;

use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

lazy_static! {
    static ref HTML_SRC_PATTERN: Regex =
        Regex::new(r#".*?<.*?src=["']?([^;>=]+?)["']?(?:>|\s\w+=)"#).unwrap();
    static ref CSS_URL_PATTERN: Regex =
        Regex::new(r#".*?<.*?url\(["']?([^;>=]+?)["']?\)"#).unwrap();
}

/// Restricts a user-provided resource path (attachment, image, template) to stay
/// within a trusted root directory. Absolute paths, `..` escapes and external
/// URL schemes are rejected, so untrusted entry files can neither read nor
/// exfiltrate arbitrary local files (plan gap G5-4).
#[inline]
pub fn resolve_within_root(raw: &str, root: &Path) -> Result<PathBuf> {
    let candidate = Path::new(raw);

    // Reject remote resources (`http://...`, `file://...`, etc.)
    if candidate
        .components()
        .any(|c| matches!(c, std::path::Component::Prefix(_)))
    {
        return Err(anyhow!(
            "External resource paths are not allowed: `{raw}`"
        ));
    }

    if candidate.is_absolute() {
        return Err(anyhow!(
            "Absolute resource paths are not allowed: `{raw}`"
        ));
    }

    let root = root
        .canonicalize()
        .with_context(|| format!("Resource root directory not found: {}", root.display()))?;

    let mut resolved = root.clone();
    for component in candidate.components() {
        match component {
            std::path::Component::ParentDir => {
                resolved = resolved
                    .parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or(resolved);
            }
            std::path::Component::CurDir => {}
            other => resolved = resolved.join(other),
        }
    }

    let resolved = resolved
        .canonicalize()
        .with_context(|| format!("Resource path not found: `{raw}`"))?;

    if !resolved.starts_with(&root) {
        return Err(anyhow!(
            "Resource path escapes its root directory: `{raw}`"
        ));
    }

    Ok(resolved)
}

#[inline]
fn split(input: &str) -> impl Iterator<Item = &str> {
    input
        .split([',', ';'].as_ref())
        .map(|part| part.trim())
        .filter(|&part| !part.is_empty())
}

#[inline]
fn owned_filename_string(path: &Path) -> Result<String> {
    let string_filename = path
        .file_name()
        .with_context(|| {
            format!(
                "Unable to get filename from path `{}`",
                path.to_string_lossy()
            )
        })?
        .to_str()
        .with_context(|| {
            format!(
                "Unable to cheaply convert filepath `{}` from OS string to valid UTF-8 string",
                path.to_string_lossy()
            )
        })?
        .to_owned();
    Ok(string_filename)
}

#[inline]
/// Infers the MIME-Type of a given filepath.
/// Unknown MIME-Types are currently set to `application/octet-stream`.
/// ## Error
/// Fails if unable to load file path.
fn get_mime(filepath: impl AsRef<Path>) -> std::io::Result<&'static str> {
    // mime_guess::from_path(filepath)
    //     .first()
    //     .unwrap()
    //     .as_ref()
    //     .to_owned();

    let inferred_mime_type = infer::get_from_path(&filepath)?;

    let mime_type = if let Some(known_type) = inferred_mime_type {
        known_type.mime_type()
    } else {
        // Either fallback to MIME Guess or `application/octet-stream`
        "application/octet-stream"
    };
    Ok(mime_type)
}

pub trait MultiPartAttachments {
    // TODO: Attach content from within the code, contained an owned Vec[u8] + Case for Base64
    fn attachments(attachments: &str, root: &Path) -> Result<Option<MultiPart>>;
}

impl MultiPartAttachments for MultiPart {
    /// Build a MultiPart loaded with attachments from the given multiple paths
    /// (separated by `;` or `,`), each interpreted relative to `root`.
    ///
    /// Attachment paths are required to stay within `root` (no absolute
    /// paths, no `..` escapes, no remote URLs); violators are skipped with a
    /// warning instead of being attached (plan gap G5-4).
    fn attachments(paths: &str, root: &Path) -> Result<Option<MultiPart>> {
        let mut file_contents_body;
        let mut file_content_type;

        let mut multi_part: Option<MultiPart> = None;

        for attachment in split(paths) {
            let attachment_path = match resolve_within_root(attachment, root) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("Failed to attach file: \"{attachment}\". {e}");
                    continue;
                }
            };

            match fs::read(&attachment_path) {
                Ok(fd) => {
                    // file_data = fs::read(attachment_path).expect("File not found");
                    file_contents_body = Body::new(fd);
                    file_content_type = match get_mime(&attachment_path) {
                        Ok(mime_type) => mime_type,
                        Err(e) => {
                            // Unable to determine the MIME type? Skip attachment file and report the error
                            eprintln!("{e:?}");
                            continue;
                        }
                    };

                    let attachment_filename = match owned_filename_string(&attachment_path) {
                        Ok(v) => v,
                        Err(e) => {
                            // Unable to get filename? Skip attachment file and report the error
                            eprintln!("{e:?}");
                            continue;
                        }
                    };

                    let attachment_part = Attachment::new(attachment_filename).body(
                        file_contents_body,
                        file_content_type
                            .parse()
                            .context("Unable to parse attached file content type")?, // FIXME: Skip iteration instead of return
                    );

                    multi_part = Some(match multi_part {
                        None => MultiPart::mixed().singlepart(attachment_part),
                        Some(part) => part.singlepart(attachment_part),
                    });
                }
                Err(e) => {
                    eprintln!(
                        "Failed to attach file: \"{}\". {e}",
                        attachment_path.display()
                    );
                    continue;
                }
            }
        }
        Ok(multi_part)
    }
}

pub trait MultiPartHtmlWithImages {
    fn html_with_images(html_contents: &str, resources_path: Option<&Path>) -> Result<MultiPart>;
}
impl MultiPartHtmlWithImages for MultiPart {
    fn html_with_images(html_contents: &str, resources_path: Option<&Path>) -> Result<MultiPart> {
        // TODO: then, remove all comments from the final HTML + Optimize HTML size

        // Resources resolve against the template's root directory, or the
        // current working directory when no template root is given.
        let resources_root = resources_path.unwrap_or_else(|| Path::new("."));

        let mut images: Vec<(String, &'static str, PathBuf)> = Vec::new();
        let mut cid_by_filename: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        // (start, end, cid) spans of the matched `src`/`url()` values, to be
        // rewritten in the final HTML.
        let mut spans: Vec<(usize, usize, String)> = Vec::new();

        let caps = HTML_SRC_PATTERN
            .captures_iter(html_contents)
            .chain(CSS_URL_PATTERN.captures_iter(html_contents));

        for (i, cap) in caps.enumerate() {
            let Some(filename_match) = cap.get(1) else {
                continue;
            };
            let filename = filename_match.as_str();

            // Resolve against the resources root (containment enforced);
            // broken references are skipped instead of failing the e-mail.
            let full_file_path = match resolve_within_root(filename, resources_root) {
                Ok(p) => p,
                Err(e) => {
                    log::debug!("Skipping image resource `{filename}`: {e}");
                    continue;
                }
            };

            // Infer the MIME type from the resolved path, not the raw
            // CWD-relative HTML reference.
            let mime = match get_mime(&full_file_path) {
                Ok(mime_type) => mime_type,
                Err(e) => {
                    log::debug!("Skipping image resource `{filename}`: {e}");
                    continue;
                }
            };

            let cid = match cid_by_filename.get(filename) {
                Some(existing_cid) => existing_cid.clone(),
                None => {
                    let cid = format!("image_{i}");
                    cid_by_filename.insert(filename.to_string(), cid.clone());
                    images.push((cid.clone(), mime, full_file_path));
                    cid
                }
            };

            spans.push((filename_match.start(), filename_match.end(), cid));
        }

        // Rewrite only the exact matched spans (never other occurrences of a
        // similar file name), deduplicated per resource.
        spans.sort_by_key(|(start, _, _)| *start);
        let mut html_image_embedded = String::with_capacity(html_contents.len());
        let mut last_end = 0usize;
        for (start, end, cid) in spans {
            if start < last_end {
                continue; // overlapping match from the two regex passes
            }
            html_image_embedded.push_str(&html_contents[last_end..start]);
            html_image_embedded.push_str(&format!("cid:{cid}"));
            last_end = end;
        }
        html_image_embedded.push_str(&html_contents[last_end..]);

        let mut multi_part = MultiPart::related().singlepart(
            SinglePart::builder()
                .header(header::ContentType::TEXT_HTML)
                .header(header::ContentTransferEncoding::Base64)
                .body(html_image_embedded),
        );

        for (cid, mime, full_file_path) in images {
            let image_data = match fs::read(&full_file_path) {
                Ok(data) => data,
                Err(e) => {
                    log::debug!("Skipping unreadable inline image `{cid}`: {e}");
                    continue;
                }
            };
            let image_body = Body::new(image_data);
            multi_part = multi_part.singlepart(
                Attachment::new_inline(cid).body(
                    image_body,
                    mime.parse()
                        .context("Unable to parse attached image content type")?,
                ),
            )
        }
        Ok(multi_part)
    }
}

pub trait MultipleAddressParser {
    fn to_addresses(self, addresses: &str) -> Result<LettreMessageBuilder, AddressError>;
    fn cc_addresses(self, addresses: &str) -> Result<LettreMessageBuilder, AddressError>;
    fn bcc_addresses(self, addresses: &str) -> Result<LettreMessageBuilder, AddressError>;
    fn reply_to_addresses(self, addresses: &str) -> Result<LettreMessageBuilder, AddressError>;
}

impl MultipleAddressParser for LettreMessageBuilder {
    fn to_addresses(mut self, addresses: &str) -> Result<Self, AddressError> {
        for address in split(addresses) {
            self = self.to(address.parse()?);
        }
        Ok(self)
    }

    fn cc_addresses(mut self, addresses: &str) -> Result<Self, AddressError> {
        for address in split(addresses) {
            self = self.cc(address.parse()?);
        }
        Ok(self)
    }

    fn bcc_addresses(mut self, addresses: &str) -> Result<Self, AddressError> {
        for address in split(addresses) {
            self = self.bcc(address.parse()?);
        }
        Ok(self)
    }

    fn reply_to_addresses(mut self, addresses: &str) -> Result<LettreMessageBuilder, AddressError> {
        for address in split(addresses) {
            self = self.reply_to(address.parse()?);
        }
        Ok(self)
    }
}

/// Defines how to connect
#[derive(Debug, Clone, PartialEq)]
pub enum Authentication {
    NoAuth,
    Tls,
    Starttls,
}

impl std::fmt::Display for Authentication {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Authentication::NoAuth => write!(f, "noauth"),
            Authentication::Tls => write!(f, "tls"),
            Authentication::Starttls => write!(f, "starttls"),
        }
    }
}

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum RelayError {
    #[error("Unknown SMTP authentication method \"{0}\"")]
    UnknownAuthenticationMethod(String),
}

impl FromStr for Authentication {
    type Err = RelayError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let res = match s.trim().to_lowercase().as_str() {
            "noauth" => Authentication::NoAuth,
            "tls" => Authentication::Tls,
            "starttls" => Authentication::Starttls,
            _ => return Err(RelayError::UnknownAuthenticationMethod(s.to_string())),
        };

        Ok(res)
    }
}

#[derive(Debug, Default, Clone)]
pub struct MessageBuilder<'a> {
    from: Option<&'a str>,
    reply_to_addresses: Option<&'a str>,
    in_reply_to: Option<String>,
    to_addresses: Option<&'a str>,
    cc_addresses: Option<&'a str>,
    bcc_addresses: Option<&'a str>,
    subject: Option<&'a str>,
    content: Option<&'a str>,
    resources_path: Option<&'a Path>,
    alternative_content: Option<&'a str>,
    attachments: Option<&'a str>,
    attachments_root: Option<&'a Path>,
}

impl<'a> MessageBuilder<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from(&mut self, address: &'a str) -> &mut Self {
        self.from = Some(address);
        self
    }

    pub fn reply_to_addresses(&mut self, addresses: &'a str) -> &mut Self {
        self.reply_to_addresses = Some(addresses);
        self
    }

    pub fn in_reply_to(&mut self, id: String) -> &mut Self {
        self.in_reply_to = Some(id);
        self
    }

    pub fn to_addresses(&mut self, addresses: &'a str) -> &mut Self {
        self.to_addresses = Some(addresses);
        self
    }

    pub fn cc_addresses(&mut self, addresses: &'a str) -> &mut Self {
        self.cc_addresses = Some(addresses);
        self
    }

    pub fn bcc_addresses(&mut self, addresses: &'a str) -> &mut Self {
        self.bcc_addresses = Some(addresses);
        self
    }

    pub fn subject(&mut self, subject: &'a str) -> &mut Self {
        self.subject = Some(subject);
        self
    }

    pub fn content(&mut self, content: &'a str, resources_path: Option<&'a Path>) -> &mut Self {
        self.content = Some(content);
        self.resources_path = resources_path;
        self
    }

    pub fn alternative_content(&mut self, content: &'a str) -> &mut Self {
        self.alternative_content = Some(content);
        self
    }

    pub fn attachments(&mut self, attachments: &'a str, root: Option<&'a Path>) -> &mut Self {
        self.attachments = Some(attachments);
        self.attachments_root = root;
        self
    }

    pub fn build(&self) -> Result<Message> {
        let mut new_message = Message::new();

        if let Some(address) = self.from {
            new_message = new_message.from(address)?;
        }

        if let Some(addresses) = self.reply_to_addresses {
            new_message = new_message.reply_to_addresses(addresses)?;
        }

        if let Some(ref id) = self.in_reply_to {
            new_message = new_message.in_reply_to(id.clone());
        }

        if let Some(addresses) = self.to_addresses {
            new_message = new_message.to_addresses(addresses)?;
        }

        if let Some(addresses) = self.cc_addresses {
            new_message = new_message.cc_addresses(addresses)?;
        }

        if let Some(addresses) = self.bcc_addresses {
            new_message = new_message.bcc_addresses(addresses)?;
        }

        if let Some(subject) = self.subject {
            new_message = new_message.subject(subject);
        }

        if let Some(content) = self.content {
            new_message = new_message.content(content, self.resources_path)?;
        }

        if let Some(content) = self.alternative_content {
            new_message = new_message.alternative_content(content);
        }

        if let Some(attachments) = self.attachments {
            let root = self
                .attachments_root
                .unwrap_or_else(|| Path::new("."));
            new_message = new_message.attachments(attachments, root)?;
        }

        Ok(new_message)
    }
}

/// Contains all contents of an E-Mail to be sent later.
#[derive(Debug, Default, Clone)]
pub struct Message {
    message_builder: LettreMessageBuilder,
    content: Option<MultiPart>,
    alternative_content: Option<SinglePart>,
    attachments: Option<MultiPart>,
}

impl Message {
    fn new() -> Self {
        Self::default()
    }

    pub fn from(mut self, address: &str) -> Result<Self> {
        self.message_builder = self.message_builder.from(
            address
                .parse()
                .context("Unable to parse `from` address(es)")?,
        );
        Ok(self)
    }

    pub fn reply_to_addresses(mut self, addresses: &str) -> Result<Self> {
        self.message_builder = self
            .message_builder
            .reply_to_addresses(addresses)
            .context("Unable to parse `reply_to` address(es)")?;
        Ok(self)
    }

    pub fn in_reply_to(mut self, id: String) -> Self {
        self.message_builder = self.message_builder.in_reply_to(id);
        self
    }

    pub fn to_addresses(mut self, addresses: &str) -> Result<Self> {
        self.message_builder = self
            .message_builder
            .to_addresses(addresses)
            .context("Unable to parse `to` address(es)")?;
        Ok(self)
    }

    pub fn cc_addresses(mut self, addresses: &str) -> Result<Self> {
        self.message_builder = self
            .message_builder
            .cc_addresses(addresses)
            .context("Unable to parse `cc` address(es)")?;
        Ok(self)
    }

    pub fn bcc_addresses(mut self, addresses: &str) -> Result<Self> {
        self.message_builder = self
            .message_builder
            .bcc_addresses(addresses)
            .context("Unable to parse `bcc` address(es)")?;
        Ok(self)
    }

    pub fn subject(mut self, subject: &str) -> Self {
        self.message_builder = self.message_builder.subject(subject);
        self
    }

    pub fn content(mut self, content: &str, resources_path: Option<&Path>) -> Result<Self> {
        self.content = Some(MultiPart::html_with_images(content, resources_path)?);
        Ok(self)
    }

    pub fn alternative_content(mut self, content: &str) -> Self {
        self.alternative_content = Some(
            SinglePart::builder()
                .header(header::ContentType::TEXT_PLAIN)
                .header(header::ContentTransferEncoding::Base64)
                .body(content.to_owned()),
        );
        self
    }

    pub fn attachments(mut self, attachments: &str, root: &Path) -> Result<Self> {
        self.attachments = MultiPart::attachments(attachments, root)?;
        Ok(self)
    }
}

impl std::convert::TryFrom<Message> for LettreMessage {
    type Error = anyhow::Error;

    fn try_from(message: Message) -> std::result::Result<Self, Self::Error> {
        let mut multipart: Option<MultiPart> = None;

        if let Some(alternative_content) = message.alternative_content {
            multipart = Some(MultiPart::alternative().singlepart(alternative_content));
        }

        if let Some(content) = message.content {
            multipart = if let Some(parts) = multipart {
                Some(parts.multipart(content))
            } else {
                Some(content)
            };
        }

        if let Some(attachments) = message.attachments {
            multipart = if let Some(parts) = multipart {
                Some(MultiPart::mixed().multipart(parts).multipart(attachments))
            } else {
                Some(attachments)
            };
        }

        let built_message = message
            .message_builder
            .multipart(multipart.unwrap_or_else(|| {
                MultiPart::mixed().singlepart(
                    SinglePart::builder()
                        .header(header::ContentType::TEXT_PLAIN)
                        .body(String::new()), // Empty E-mail if no contents were given
                )
            }))
            .context("Unable to create a message multi-part")?;
        Ok(built_message)
    }
}

/// Establishes a connection and sends SMTP messages from its own thread (actor).
/// Receiving Messages from a Messages Channel and sends them downstream to the connection.
// #[derive(Debug)]
pub struct Connection<'a> {
    // Username/Password Method: TLS/Starttls/NoAuth
    relay_server: &'a str,
    port: u16,
    // channel: (Sender<LettreMessage>, Receiver<LettreMessage>),
    // tx: Option<Sender<LettreMessage>>,
    // mode: ConnectionMode,
    connection: Option<SmtpTransport>,
    auth: Authentication,
}

impl<'a> Connection<'a> {
    pub fn new(relay_server: &'a str, port: u16, auth: Authentication) -> Self {
        Self {
            // credentials: Credentials::new(username, password), // TODO: Improve security:
            relay_server,
            port,
            auth,
            connection: None,
        }
    }

    // fn job(&self) {
    //     let rx = &self.rx;
    //     println!("test");
    // }

    /// Establish the connection
    // pub fn establish(&mut self, username: SecUtf8, password: SecUtf8) {
    //     let connection = SmtpTransport::relay(self.relay_server)
    //         .unwrap()
    //         .credentials(Credentials::new(
    //             username.into_unsecure(),
    //             password.into_unsecure(),
    //         ))
    //         .port(self.port) // TODO: Set all configurations: https://docs.rs/lettre/latest/lettre/transport/smtp/struct.SmtpTransportBuilder.html#method.port
    //         .build();
    // }

    pub fn establish(&mut self, credentials: Option<Credentials>) -> Result<()> {
        let connection = match self.auth {
            Authentication::NoAuth => SmtpTransport::builder_dangerous(self.relay_server)
                .port(self.port)
                .build(),
            Authentication::Tls => {
                let mut smtp_builder = SmtpTransport::relay(self.relay_server)
                    .context("Failed to establish `TLS` connection with the provided mail relay")?;

                if let Some(passed_credentials) = credentials {
                    smtp_builder = smtp_builder.credentials(passed_credentials);
                };

                smtp_builder
                    .port(self.port) // TODO: Set all configurations: https://docs.rs/lettre/0.10.0-rc.4/lettre/transport/smtp/struct.SmtpTransportBuilder.html#method.port
                    .build()
            }
            Authentication::Starttls => {
                let mut smtp_builder = SmtpTransport::starttls_relay(self.relay_server).context(
                    "Failed to establish `STARTTLS` connection with the provided mail relay",
                )?;

                if let Some(passed_credentials) = credentials {
                    smtp_builder = smtp_builder.credentials(passed_credentials);
                };

                smtp_builder
                    .port(self.port) // TODO: Set all configurations: https://docs.rs/lettre/0.10.0-rc.4/lettre/transport/smtp/struct.SmtpTransportBuilder.html#method.port
                    .build()
            }
        };

        // .unwrap()
        // .credentials(Credentials::new(
        //     username.into_unsecure(),
        //     password.into_unsecure(),
        // ))
        // .port(self.port) // TODO: Set all configurations: https://docs.rs/lettre/latest/lettre/transport/smtp/struct.SmtpTransportBuilder.html#method.port
        // .build();

        self.connection = Some(connection);
        Ok(())
    }

    /// Send a lettre Message object downstream
    pub fn send(&self, msg: LettreMessage) -> Result<()> {
        let connection = self
            .connection
            .as_ref()
            .ok_or_else(|| anyhow!("No connection was established."));

        connection?.send(&msg)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal 1x1 transparent PNG; the `infer` crate recognizes it by its
    /// magic bytes and reports `image/png`.
    const PNG_1X1: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x62, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("osa_mailer_test_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn split_addresses_trims_and_filters_empty() {
        let parts: Vec<&str> = split(" a@x.io , b@x.io ; c@x.io ,  ").collect();
        assert_eq!(parts, vec!["a@x.io", "b@x.io", "c@x.io"]);
        assert_eq!(split("").count(), 0);
    }

    #[test]
    fn authentication_parses_known_methods() {
        assert_eq!("noauth".parse::<Authentication>(), Ok(Authentication::NoAuth));
        assert_eq!("TLS".parse::<Authentication>(), Ok(Authentication::Tls));
        assert_eq!(" starttls ".parse::<Authentication>(), Ok(Authentication::Starttls));
        assert!("bogus".parse::<Authentication>().is_err());
    }

    #[test]
    fn owned_filename_string_extracts_name() {
        assert_eq!(
            owned_filename_string(Path::new("a/b/c.txt")).unwrap(),
            "c.txt"
        );
    }

    #[test]
    fn resolve_within_root_accepts_paths_inside_root() {
        let dir = temp_dir("root_ok");
        fs::write(dir.join("a.txt"), "x").unwrap();
        fs::create_dir_all(dir.join("sub")).unwrap();
        fs::write(dir.join("sub/b.txt"), "x").unwrap();

        let expected_root = dir.canonicalize().unwrap();
        assert_eq!(
            resolve_within_root("a.txt", &dir).unwrap(),
            expected_root.join("a.txt")
        );
        assert!(resolve_within_root("sub/b.txt", &dir).is_ok());
    }

    #[test]
    fn resolve_within_root_rejects_dotdot_escape() {
        let dir = temp_dir("root_escape");
        let outside = dir
            .parent()
            .unwrap()
            .join(format!("osa_mailer_outside_{}", std::process::id()));
        fs::write(&outside, "outside").unwrap();

        let rel = format!("../{}", outside.file_name().unwrap().to_string_lossy());
        assert!(resolve_within_root(&rel, &dir).is_err());

        let _ = fs::remove_file(&outside);
    }

    #[test]
    fn resolve_within_root_rejects_absolute_paths() {
        let dir = temp_dir("root_abs");
        assert!(resolve_within_root("C:\\Windows\\win.ini", &dir).is_err());
        assert!(resolve_within_root("/etc/passwd", &dir).is_err());
    }

    #[test]
    fn resolve_within_root_rejects_external_urls() {
        let dir = temp_dir("root_url");
        assert!(resolve_within_root("http://example.com/x.png", &dir).is_err());
        assert!(resolve_within_root("https://example.com/x.png", &dir).is_err());
    }

    #[test]
    fn resolve_within_root_rejects_missing_files() {
        let dir = temp_dir("root_missing");
        assert!(resolve_within_root("nope.txt", &dir).is_err());
    }

    /// Render a full message (headers + MIME body) as a string for inspection.
    fn render(msg: LettreMessage) -> String {
        String::from_utf8(msg.formatted()).expect("valid utf-8")
    }

    /// Decode the Base64-encoded body of the `text/html` part of a rendered
    /// message (the HTML part carries a Base64 content-transfer-encoding).
    fn decode_html_part(msg: &str) -> String {
        use base64::{engine::general_purpose::STANDARD, Engine};

        let lines: Vec<&str> = msg.lines().collect();
        let start = lines
            .iter()
            .position(|l| l.trim().to_ascii_lowercase().starts_with("content-type: text/html"))
            .expect("message must contain a text/html part");

        // Skip the remaining headers of the part (up to the blank line)
        let mut i = start + 1;
        while i < lines.len() && !lines[i].is_empty() && !lines[i].starts_with("--") {
            i += 1;
        }

        // Collect the (wrapped) base64 body lines up to the next boundary
        let mut body = String::new();
        while i < lines.len() && !lines[i].trim_start().starts_with("--") {
            body.push_str(lines[i].trim());
            i += 1;
        }

        let decoded = STANDARD.decode(body).expect("valid base64");
        String::from_utf8(decoded).expect("utf-8 html")
    }

    /// Build and render a message whose HTML content is processed by
    /// `html_with_images` against the given resources root.
    fn message_with_content(content: &str, root: Option<&Path>) -> String {
        let msg = Message::default()
            .from("from@example.com")
            .expect("from")
            .to_addresses("to@example.com")
            .expect("to")
            .subject("t")
            .content(content, root)
            .expect("content")
            .try_into()
            .expect("try_into");
        render(msg)
    }

    fn message_with_attachments(paths: &str, root: &Path) -> String {
        let msg = Message::default()
            .from("from@example.com")
            .expect("from")
            .to_addresses("to@example.com")
            .expect("to")
            .subject("t")
            .attachments(paths, root)
            .expect("attachments")
            .try_into()
            .expect("try_into");
        render(msg)
    }

    #[test]
    fn attachments_resolves_within_root() {
        let dir = temp_dir("attach_ok");
        fs::write(dir.join("a.txt"), "alpha").unwrap();
        fs::write(dir.join("b.txt"), "bravo").unwrap();

        let rendered = message_with_attachments("a.txt; b.txt", &dir);
        assert!(rendered.contains(r#"filename="a.txt""#));
        assert!(rendered.contains(r#"filename="b.txt""#));
    }

    #[test]
    fn attachments_skips_paths_that_escape_root() {
        let dir = temp_dir("attach_escape");
        fs::write(dir.join("inside.txt"), "in").unwrap();
        let outside = dir
            .parent()
            .unwrap()
            .join(format!("osa_mailer_attach_outside_{}", std::process::id()));
        fs::write(&outside, "out").unwrap();

        let rendered = message_with_attachments("inside.txt, ../x", &dir);
        assert!(rendered.contains(r#"filename="inside.txt""#));

        // An absolute path must not be attached
        let multi_part = MultiPart::attachments("C:\\Windows\\win.ini", &dir).unwrap();
        assert!(multi_part.is_none());

        let _ = fs::remove_file(&outside);
    }

    #[test]
    fn inline_image_is_inlined_from_resolved_template_path() {
        // Plan gap G1-9: the image lives under the template root, not the CWD,
        // so it must be found via the resolved path.
        let dir = temp_dir("images_ok");
        fs::create_dir_all(dir.join("images")).unwrap();
        fs::write(dir.join("images/logo.png"), PNG_1X1).unwrap();

        let html = "<html><body><img src=\"images/logo.png\" alt=\"l\"> the file logo.png exists</body></html>";
        let rendered = message_with_content(html, Some(&dir));
        let decoded = decode_html_part(&rendered);

        assert!(decoded.contains("cid:image_0"), "src must be replaced by a cid");
        assert!(!decoded.contains("src=\"images/logo.png\""));
        // Only the matched src span is rewritten, never other occurrences
        assert!(decoded.contains("the file logo.png exists"));
        assert!(rendered.contains("image/png"));
    }

    #[test]
    fn duplicate_image_references_share_one_cid_and_attachment() {
        let dir = temp_dir("images_dupe");
        fs::create_dir_all(dir.join("images")).unwrap();
        fs::write(dir.join("images/logo.png"), PNG_1X1).unwrap();

        let html = "<img src=\"images/logo.png\"><img src=\"images/logo.png\">";
        let rendered = message_with_content(html, Some(&dir));
        let decoded = decode_html_part(&rendered);

        assert_eq!(decoded.matches("cid:image_0").count(), 2);
        // The image is attached exactly once despite two references
        assert_eq!(rendered.matches("image/png").count(), 1);
    }

    #[test]
    fn missing_inline_image_is_skipped_not_fatal() {
        let dir = temp_dir("images_missing");
        fs::create_dir_all(dir.join("images")).unwrap();

        let html = "<img src=\"images/gone.png\">";
        let rendered = message_with_content(html, Some(&dir));
        let decoded = decode_html_part(&rendered);
        assert!(!decoded.contains("cid:"));
        assert!(decoded.contains("images/gone.png"));
    }

    #[test]
    fn external_url_image_is_skipped_not_fatal() {
        let dir = temp_dir("images_url");
        let html = "<img src=\"http://example.com/x.png\">";
        let rendered = message_with_content(html, Some(&dir));
        let decoded = decode_html_part(&rendered);
        assert!(!decoded.contains("cid:"));
        assert!(decoded.contains("http://example.com/x.png"));
    }

    #[test]
    fn escaping_inline_image_is_skipped_not_fatal() {
        // `secret.png` exists, but outside the template root (`subdir/`):
        // it must be skipped, not attached.
        let dir = temp_dir("images_escape");
        fs::create_dir_all(dir.join("subdir")).unwrap();
        fs::write(dir.join("secret.png"), PNG_1X1).unwrap();

        let html = "<img src=\"../secret.png\">";
        let rendered = message_with_content(html, Some(&dir.join("subdir")));
        let decoded = decode_html_part(&rendered);
        assert!(!decoded.contains("cid:"));
        assert!(decoded.contains("../secret.png"));
    }

    #[test]
    fn message_builds_alternative_plain_and_html_parts() {
        // Plan gap G1-1: alternative_content must produce a text/plain part
        // alongside the HTML part (previously it silently replaced the
        // HTML content).
        let raw = send_message("from@example.com", "to@example.com", "<b>Hi</b>", Some("Plain text body"));

        assert!(raw.contains("text/plain"), "plain alternative part missing");
        assert!(raw.contains("text/html"), "html part missing");
        // base64("Plain text body")
        assert!(raw.contains("UGxhaW4gdGV4dCBib2R5"), "alt body must be present");
        assert!(raw.contains("multipart/alternative"));
    }

    #[test]
    fn message_without_alternative_has_no_plain_part() {
        let raw = send_message("from@example.com", "to@example.com", "<b>Hi</b>", None);
        assert!(raw.contains("text/html"));
        assert!(!raw.contains("text/plain"));
    }

    fn send_message(
        from: &str,
        to: &str,
        content: &str,
        alternative: Option<&str>,
    ) -> String {
        let dir = temp_dir("msg_parts");
        let mut message = Message::default()
            .from(from)
            .expect("from")
            .to_addresses(to)
            .expect("to")
            .subject("t")
            .content(content, Some(&dir))
            .expect("content");
        if let Some(alt) = alternative {
            message = message.alternative_content(alt);
        }
        render(message.try_into().expect("try_into"))
    }
}
