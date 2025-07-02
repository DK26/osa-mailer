<div align="center">

![osa-mailer](assets/logo.png)

**Send dynamic and sophisticated E-mails using Smart Templates**

[Guidebook](https://dk26.github.io/osa-mailer/) |
[Discussions](https://github.com/DK26/osa-mailer/discussions)  

[![Build](https://github.com/DK26/osa-mailer/actions/workflows/general.yml/badge.svg?branch=main)](https://github.com/DK26/osa-mailer/actions/workflows/general.yml)
[![Security Audit & License Compatibility](https://github.com/DK26/osa-mailer/actions/workflows/security-audit.yml/badge.svg?branch=main)](https://github.com/DK26/osa-mailer/actions/workflows/security-audit.yml)
[![pages-build-deployment](https://github.com/DK26/osa-mailer/actions/workflows/pages/pages-build-deployment/badge.svg?branch=main)](https://github.com/DK26/osa-mailer/actions/workflows/pages/pages-build-deployment)  

[Download Alpha 3 (i686-pc-windows)](https://github.com/DK26/osa-mailer/releases/tag/alpha-3)

</div>

## Features

- Currently supports 3 template engines: `tera`, `handlebars` and `liquid`
- Automatically attaches all your HTML resources to the SMTP message
- Provides you the ability to aggregate multiple E-mails of the same subject and recipients, into a single E-mail with accumulated context
- Supports multiple SMTP connection types: `NOAUTH`, `TLS` and `STARTTLS`

## Supported Template Engines

| Name       | Short / File Extension | Version | Guide / Manual / Tutorial                                     |
| ---------- | ---------------------- | ------- | ------------------------------------------------------------- |
| Tera       | `tera`                 | v1.18.1 | <https://tera.netlify.app/docs/#templates>                    |
| Handlebars | `hbs`                  | v4.3.6  | <https://handlebarsjs.com/guide/>                             |
| Liquid     | `liq`                  | v0.26.1 | <https://github.com/Shopify/liquid/wiki/Liquid-for-Designers> |

## How it Works

OSA-Mailer uses asynchronous approach for sending E-mails, sacrificing realtime notifications in favour of increasing feature capabilities. By using an asynchronous approach, OSA-Mailer can merge E-mails context of the same us recipients, and craft beautiful and sophisticated E-mails, using rendering engines.

1. An external tool is creating and writing a formatted E-mail send requests into the `outbox` directory (refer to the `send_email_request` python scripts for examples)  
2. The OSA-Mailer service is pulling every X configured seconds for new E-mail send requests in the `outbox` directory  
3. OSA-Mailer calculates an E-mail ID for each request, effectively merging dynamic context values  
4. It then composes an SMTP message using the provided HTML template, rendering it with the chosen rendering engine against the merged context, allowing for a dynamic E-mail creation, and automatically loads all related resources (images) used within the HTML template  
5. It sends out the final SMTP E-mail message, using the configurations within the `osa_mailer.cmd` file  

## Quick Template Engines Guide

<details>
<summary>Tera (click to expand)</summary>

* Guide: <https://tera.netlify.app/docs/#templates>  
* Version: **v1.18.1**
* Repository: <https://github.com/Keats/tera>
* Alternatives: `Jinja2`, `Django`, `Liquid`, `Twig`
  
A highly advanced, capable and secure by default; rendering engine that follows the OWASP Top 10 guidelines.
A good alternative choice if you are used to template engines such as `Jinja2`, `Django`, `Liquid` or `Twig`. Originated in the Rust programming language.  

</details>

<details>
<summary>Handlebars (click to expand)</summary>

* Guide: <https://handlebarsjs.com/guide/>  
* Version: **v4.3.6**
* Repository: <https://github.com/sunng87/handlebars-rust>
* Alternatives: `Mustache`
  
A highly popular rendering engine that has been implemented across many programming languages. Considered to be somewhat more limited in features compared to the other engines. Originated in the Javascript programming language.

</details>

<details>
<summary>Liquid (click to expand)</summary>

* Guide: <https://github.com/Shopify/liquid/wiki/Liquid-for-Designers>  
* Version: **v0.26.1**
* Repository: <https://github.com/cobalt-org/liquid-rust>
* Alternatives: `smarty`
  
A highly advanced, capable and senior rendering engine, offering some optional security capabilities. A good alternative choice if you are used to the `smarty` template engine. Originated in the Ruby programming language.

</details>

## Template Design & Preview

Try the [`rendit` CLI tool](https://github.com/DK26/rendit)

## Prototype Note

This is still an MVP (Minimal Viable Product) and there is still work to be done and features to be added. Features may be added or removed later with no notice (but typically documented in the changelog notes of each release)

## License
MIT