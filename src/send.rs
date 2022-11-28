use lazy_static::lazy_static;

use lettre::address::AddressError;
use lettre::message::{header, Attachment, Body, MessageBuilder, MultiPart, SinglePart};

use regex::Regex;
use relative_path::RelativePath;
use secstr::SecUtf8;

use std::fs;
use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

lazy_static! {
    static ref HTML_SRC_PATTERN: Regex =
        Regex::new(r#".*?<.*?src=["']?([^;>=]+?)["']?(?:>|\s\w+=)"#).unwrap();
    static ref CSS_URL_PATTERN: Regex =
        Regex::new(r#".*?<.*?url\(["']?([^;>=]+?)["']?\)"#).unwrap();
}

#[inline]
fn split(input: &str) -> impl Iterator<Item = &str> {
    input
        .split([',', ';'].as_ref())
        .map(|part| part.trim())
        .filter(|&part| !part.is_empty())
}

#[inline]
fn owned_filename_string(path: &Path) -> String {
    path.file_name().unwrap().to_str().unwrap().to_owned()
}

#[inline]
fn get_mime(filepath: impl AsRef<Path>) -> String {
    // TODO: Considering using the `infer` crate instead and `mime_guess` as fallback.
    mime_guess::from_path(filepath)
        .first()
        .unwrap()
        .as_ref()
        .to_owned()
}

#[inline]
fn get_path(path: impl AsRef<Path>, root_dir: Option<&Path>) -> RelativePath {
    let mut relative_path = RelativePath::new(path);

    if let Some(root_path) = root_dir {
        relative_path = relative_path.cwd(root_path);
    }

    relative_path
}

pub trait MultiPartAttachments {
    // TODO: Attach content from within the code, contained an owned Vec[u8] + Case for Base64
    // TODO: Replace return value with Result<MultiPart>
    fn attachments(attachments: &str) -> MultiPart;
}

impl MultiPartAttachments for MultiPart {
    /// Build a MultiPart loaded with attachments from the given multiple paths (separated by `;` or `,`).
    fn attachments(paths: &str) -> MultiPart {
        let mut file_data;
        let mut file_contents_body;
        let mut file_content_type;

        let mut multi_part: Option<MultiPart> = None;

        for attachment in split(paths) {
            let attachment_path = Path::new(attachment);

            file_data = fs::read(attachment_path).expect("File not found");
            file_contents_body = Body::new(file_data);
            file_content_type = get_mime(attachment_path);

            let attachment_part = Attachment::new(owned_filename_string(attachment_path))
                .body(file_contents_body, file_content_type.parse().unwrap());

            multi_part = Some(match multi_part {
                None => MultiPart::mixed().singlepart(attachment_part),
                Some(part) => part.singlepart(attachment_part),
            });
        }

        multi_part.unwrap()
    }
}

pub trait MultiPartHtmlWithImages {
    fn html_with_images(html_contents: &str, resources_path: Option<&Path>) -> MultiPart;
}
impl MultiPartHtmlWithImages for MultiPart {
    fn html_with_images(html_contents: &str, resources_path: Option<&Path>) -> MultiPart {
        // TODO: Detect render engine and pick accordingly
        // TODO: then, remove all comments from the final HTML + Optimize HTML size

        let mut html_image_embedded = html_contents.to_owned();

        let caps = HTML_SRC_PATTERN
            .captures_iter(html_contents)
            .chain(CSS_URL_PATTERN.captures_iter(html_contents));

        let mut images = Vec::new();

        for (i, cap) in caps.enumerate() {
            let filename = cap.get(1).unwrap().as_str();

            let full_file_path = get_path(filename, resources_path);

            let mime = get_mime(filename);

            let cid = format!("image_{i}");

            // println!("[{cid}][{mime}][{filename}][{full_file_path:?}]");

            html_image_embedded = html_image_embedded.replace(filename, &format!("cid:{cid}"));

            images.push((cid, mime, full_file_path));
        }

        // let mut multi_part = MultiPart::related().singlepart(SinglePart::html(html_image_embedded));
        let mut multi_part = MultiPart::related().singlepart(
            SinglePart::builder()
                .header(header::ContentType::TEXT_HTML)
                .header(header::ContentTransferEncoding::Base64)
                .body(String::from(html_image_embedded)),
        );

        for (cid, mime, full_file_path) in images {
            let image_data = fs::read(full_file_path).expect("Error reading image.");
            let image_body = Body::new(image_data);
            multi_part = multi_part
                .singlepart(Attachment::new_inline(cid).body(image_body, mime.parse().unwrap()))
        }
        multi_part
    }
}

pub trait MultipleAddressParser {
    fn to_addresses(self, addresses: &str) -> Result<MessageBuilder, AddressError>;
    fn cc_addresses(self, addresses: &str) -> Result<MessageBuilder, AddressError>;
    fn bcc_addresses(self, addresses: &str) -> Result<MessageBuilder, AddressError>;
    fn reply_to_addresses(self, addresses: &str) -> Result<MessageBuilder, AddressError>;
}

impl MultipleAddressParser for MessageBuilder {
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

    fn reply_to_addresses(mut self, addresses: &str) -> Result<MessageBuilder, AddressError> {
        for address in split(addresses) {
            self = self.reply_to(address.parse()?);
        }
        Ok(self)
    }
}

#[derive(Debug)]
pub struct SecUtf8Credentials {
    username: SecUtf8,
    password: SecUtf8,
}

impl SecUtf8Credentials {
    pub fn new(username: String, password: String) -> Self {
        Self {
            username: SecUtf8::from(username),
            password: SecUtf8::from(password),
        }
    }
}

impl From<SecUtf8Credentials> for lettre::transport::smtp::authentication::Credentials {
    fn from(credentials: SecUtf8Credentials) -> Self {
        lettre::transport::smtp::authentication::Credentials::new(
            credentials.username.into_unsecure(),
            credentials.password.into_unsecure(),
        )
    }
}

/// Defines how to connect
#[derive(Debug)]
pub enum Authentication {
    NoAuth,
    Tls,
    Starttls,
}

impl FromStr for Authentication {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let res = match s.trim().to_lowercase().as_str() {
            "noauth" => Authentication::NoAuth,
            "tls" => Authentication::Tls,
            "starttls" => Authentication::Starttls,
            _ => return Err(()),
        };

        Ok(res)
    }
}

/// Concrete description of the required SMTP connection
#[derive(Debug)]
pub struct SmtpConnectionInfo<'relay> {
    relay: &'relay str,
    port: u16,
    auth: Authentication,
    timeout: Duration,
}

impl<'relay> SmtpConnectionInfo<'relay> {
    #[inline]
    pub fn new(relay: &'relay str, port: u16, auth: Authentication, timeout: Duration) -> Self {
        Self {
            auth,
            port,
            relay,
            timeout,
        }
    }

    #[inline]
    pub fn auth(&self) -> &Authentication {
        &self.auth
    }

    #[inline]
    pub fn port(&self) -> &u16 {
        &self.port
    }

    #[inline]
    pub fn relay(&self) -> &str {
        self.relay
    }

    #[inline]
    pub fn timeout(&self) -> &Duration {
        &self.timeout
    }
}

#[derive(Debug)]
pub struct SmtpConnectionBuilder<'relay> {
    relay: &'relay str,
    port: Option<u16>,
    auth: Authentication,
    timeout: Option<Duration>,
}

impl<'relay> SmtpConnectionBuilder<'relay> {
    #[inline]
    pub fn new() -> Self {
        Self {
            relay: "localhost",
            port: None,
            auth: Authentication::NoAuth,
            timeout: None,
        }
    }

    #[inline]
    pub fn auth(mut self, auth: Authentication) -> Self {
        self.auth = auth;
        self
    }

    #[inline]
    pub fn port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    #[inline]
    pub fn relay(mut self, relay: &'relay str) -> Self {
        self.relay = relay;
        self
    }

    #[inline]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    #[inline]
    pub fn build(self) -> SmtpConnectionInfo<'relay> {
        SmtpConnectionInfo {
            port: match self.port {
                Some(port) => port,
                None => match self.auth {
                    Authentication::NoAuth => 25,
                    Authentication::Tls => 465,
                    Authentication::Starttls => 587,
                },
            },
            auth: self.auth,
            relay: self.relay,
            timeout: match self.timeout {
                Some(duration) => duration,
                None => Duration::from_secs(60),
            },
        }
    }
}
