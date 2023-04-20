use lazy_static::lazy_static;

use anyhow::{anyhow, Context, Result};
use lettre::address::AddressError;
use lettre::message::Message as LettreMessage;
use lettre::message::{header, Attachment, Body, MultiPart, SinglePart};
use lettre::{SmtpTransport, Transport};

use lettre::transport::smtp::authentication::Credentials;
use regex::Regex;
use relative_path::RelativePath;

use std::fs;
use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

type LettreMessageBuilder = lettre::message::MessageBuilder;

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
    fn attachments(attachments: &str) -> Result<Option<MultiPart>>;
}

impl MultiPartAttachments for MultiPart {
    /// Build a MultiPart loaded with attachments from the given multiple paths (separated by `;` or `,`).
    fn attachments(paths: &str) -> Result<Option<MultiPart>> {
        // let mut file_data;
        let mut file_contents_body;
        let mut file_content_type;

        let mut multi_part: Option<MultiPart> = None;

        for attachment in split(paths) {
            let attachment_path = Path::new(attachment);

            match fs::read(attachment_path) {
                Ok(fd) => {
                    // file_data = fs::read(attachment_path).expect("File not found");
                    file_contents_body = Body::new(fd);
                    file_content_type = get_mime(attachment_path);

                    let attachment_part = Attachment::new(owned_filename_string(attachment_path))
                        .body(
                            file_contents_body,
                            file_content_type
                                .parse()
                                .context("Unable to parse attached file content type")?,
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
                .body(html_image_embedded),
        );

        for (cid, mime, full_file_path) in images {
            let image_data = fs::read(full_file_path).context("Error reading image")?;
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

// #[derive(Debug)]
// pub struct SecUtf8Credentials {
//     username: SecUtf8,
//     password: SecUtf8,
// }

// impl SecUtf8Credentials {
//     pub fn new(username: String, password: String) -> Self {
//         Self {
//             username: SecUtf8::from(username),
//             password: SecUtf8::from(password),
//         }
//     }
// }

// impl From<SecUtf8Credentials> for lettre::transport::smtp::authentication::Credentials {
//     fn from(credentials: SecUtf8Credentials) -> Self {
//         lettre::transport::smtp::authentication::Credentials::new(
//             credentials.username.into_unsecure(),
//             credentials.password.into_unsecure(),
//         )
//     }
// }

/// Defines how to connect
#[derive(Debug)]
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

#[derive(thiserror::Error, Debug)]
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
        self.content = Some(content);
        self
    }

    pub fn attachments(&mut self, attachments: &'a str) -> &mut Self {
        self.attachments = Some(attachments);
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
            new_message = new_message.attachments(attachments)?;
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

    pub fn attachments(mut self, attachments: &str) -> Result<Self> {
        // self.attachments = Some(MultiPart::attachments(attachments));
        self.attachments = MultiPart::attachments(attachments)?;
        Ok(self)
    }
}

// impl std::convert::From<Message> for LettreMessage {
//     fn from(message: Message) -> Self {
//         let mut multipart: Option<MultiPart> = None;

//         if let Some(alternative_content) = message.alternative_content {
//             multipart = Some(MultiPart::alternative().singlepart(alternative_content));
//         }

//         if let Some(content) = message.content {
//             multipart = if let Some(parts) = multipart {
//                 Some(parts.multipart(content))
//             } else {
//                 Some(content)
//             };
//         }

//         if let Some(attachments) = message.attachments {
//             multipart = if let Some(parts) = multipart {
//                 Some(MultiPart::mixed().multipart(parts).multipart(attachments))
//             } else {
//                 Some(attachments)
//             };
//         }

//         message
//             .message_builder
//             .multipart(multipart.unwrap_or_else(|| {
//                 MultiPart::mixed().singlepart(
//                     SinglePart::builder()
//                         .header(header::ContentType::TEXT_PLAIN)
//                         .body(String::new()), // Empty E-mail if no contents were given
//                 )
//             }))
//             .expect("Unable to create a message multi-part")
//     }
// }

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

#[derive(Debug)]
pub enum ConnectionMode {
    Once,
    Service,
}
// struct Content<'a>(&'a str);
// struct AlternativeContent<'a>(&'a str);
// struct Attachments<'a>(&'a str);
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
