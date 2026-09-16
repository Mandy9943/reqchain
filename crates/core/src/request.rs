use crate::model::{Api, Body, Endpoint, Method};
use crate::vars::{Scope, VarError};

#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error(transparent)]
    Var(#[from] VarError),
}

/// Shortest value the global substring mask will act on. A shorter one matches so
/// much unrelated text that redacting it destroys the display without protecting
/// anything a reader could not already see.
pub const MIN_MASKABLE_LEN: usize = 6;

/// The sentinel a redacted value is replaced with everywhere in this crate.
pub const MASK_SENTINEL: &str = "***";

/// Replaces every occurrence of each `value` at least [`MIN_MASKABLE_LEN`] long
/// with [`MASK_SENTINEL`]. Shared by [`EffectiveRequest::masked_with`] and
/// `history::entry_from`, so there is exactly one place that decides what
/// "redacted" means.
pub fn redact(text: &str, values: &[String]) -> String {
    let mut out = text.to_string();
    for v in values
        .iter()
        .filter(|s| s.chars().count() >= MIN_MASKABLE_LEN)
    {
        out = out.replace(v.as_str(), MASK_SENTINEL);
    }
    out
}

#[derive(Debug, Clone, PartialEq)]
pub enum EffectiveBody {
    Text {
        content_type: String,
        content: String,
    },
    Form {
        fields: Vec<(String, String)>,
    },
    Multipart {
        fields: Vec<(String, String)>,
        files: Vec<(String, String)>,
    },
    Binary {
        path: String,
    },
}

#[derive(Debug, Clone)]
pub struct EffectiveRequest {
    pub method: Method,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<EffectiveBody>,
}

impl EffectiveRequest {
    /// Replaces every secret value with `***`, for display and shell export.
    pub fn masked(&self, secrets: &[String]) -> EffectiveRequest {
        self.masked_with(secrets, &[])
    }

    /// As [`EffectiveRequest::masked`], and additionally redacts the value of every
    /// header in `auth_headers` — the names [`crate::auth::apply_static`] reported.
    ///
    /// The two mechanisms are deliberately different:
    ///
    /// - `secrets` are masked GLOBALLY, by substring, because a secret-store literal
    ///   or a chain-derived token can turn up anywhere: the URL, a query parameter,
    ///   a header or the body. A value that reaches the URL is stored
    ///   percent-encoded, so a literal match alone never finds it — the secret
    ///   `p@ss/w+rd=` is in the URL as `p%40ss%2Fw%2Brd%3D` — and each value is
    ///   therefore redacted in both forms, using the same encoder `build` used.
    /// - `auth_headers` are redacted BY NAME, never by substring. A static auth
    ///   value is not a secret that could appear elsewhere, and feeding an ordinary
    ///   one such as `X-Api-Version: 1` to the global mask would replace every `1`
    ///   in the URL and in unrelated headers.
    ///
    /// Values shorter than [`MIN_MASKABLE_LEN`] are skipped in the global list as a
    /// belt-and-braces guard: a pathologically short value there would do the same
    /// damage. It is a backstop, not the mechanism — auth values are kept out of
    /// that list in the first place.
    pub fn masked_with(&self, secrets: &[String], auth_headers: &[String]) -> EffectiveRequest {
        let forms: Vec<String> = secrets
            .iter()
            .filter(|s| s.chars().count() >= MIN_MASKABLE_LEN)
            .flat_map(|s| {
                let encoded = urlencode(s);
                if encoded == *s {
                    vec![s.clone()]
                } else {
                    vec![s.clone(), encoded]
                }
            })
            .collect();
        let mask = |s: &String| redact(s, &forms);
        let is_auth_header = |name: &str| auth_headers.iter().any(|h| h.eq_ignore_ascii_case(name));
        EffectiveRequest {
            method: self.method,
            url: mask(&self.url),
            headers: self
                .headers
                .iter()
                .map(|(k, v)| {
                    let masked = mask(v);
                    if k.eq_ignore_ascii_case("authorization") {
                        // Keep the scheme, hide the credential, whatever set it.
                        (k.clone(), redact_credential(&masked))
                    } else if is_auth_header(k) {
                        (k.clone(), "***".to_string())
                    } else {
                        (k.clone(), masked)
                    }
                })
                .collect(),
            body: self.body.as_ref().map(|b| match b {
                EffectiveBody::Text {
                    content_type,
                    content,
                } => EffectiveBody::Text {
                    content_type: content_type.clone(),
                    content: mask(content),
                },
                EffectiveBody::Form { fields } => EffectiveBody::Form {
                    fields: fields.iter().map(|(k, v)| (k.clone(), mask(v))).collect(),
                },
                EffectiveBody::Multipart { fields, files } => EffectiveBody::Multipart {
                    fields: fields.iter().map(|(k, v)| (k.clone(), mask(v))).collect(),
                    files: files.clone(),
                },
                other => other.clone(),
            }),
        }
    }
}

pub fn build(
    api: &Api,
    endpoint: &Endpoint,
    scope: &Scope,
) -> Result<EffectiveRequest, BuildError> {
    let base = scope.interpolate(&api.base_url)?;
    let path = scope.interpolate(&endpoint.path)?;
    let mut url = format!("{}{}", base.trim_end_matches('/'), path);

    if !endpoint.query.is_empty() {
        let mut pairs = Vec::new();
        for (k, v) in &endpoint.query {
            pairs.push(format!(
                "{}={}",
                urlencode(k),
                urlencode(&scope.interpolate(v)?)
            ));
        }
        url.push('?');
        url.push_str(&pairs.join("&"));
    }

    let mut headers = Vec::new();
    for (k, v) in &endpoint.headers {
        headers.push((k.clone(), scope.interpolate(v)?));
    }

    let body = match &endpoint.body {
        None => None,
        Some(Body::Json { content }) => Some(EffectiveBody::Text {
            content_type: "application/json".into(),
            content: serde_json::to_string(&interpolate_json(content, scope)?)
                .expect("serde_json::Value always serializes"),
        }),
        Some(Body::Text { content }) => Some(EffectiveBody::Text {
            content_type: "text/plain".into(),
            content: scope.interpolate(content)?,
        }),
        Some(Body::Xml { content }) => Some(EffectiveBody::Text {
            content_type: "application/xml".into(),
            content: scope.interpolate(content)?,
        }),
        Some(Body::Form { fields }) => {
            let mut out = Vec::new();
            for (k, v) in fields {
                out.push((k.clone(), scope.interpolate(v)?))
            }
            Some(EffectiveBody::Form { fields: out })
        }
        Some(Body::Multipart { fields, files }) => {
            let mut f = Vec::new();
            for (k, v) in fields {
                f.push((k.clone(), scope.interpolate(v)?))
            }
            let mut fl = Vec::new();
            for (k, v) in files {
                fl.push((k.clone(), scope.interpolate(v)?))
            }
            Some(EffectiveBody::Multipart {
                fields: f,
                files: fl,
            })
        }
        Some(Body::Binary { path }) => Some(EffectiveBody::Binary {
            path: scope.interpolate(path)?,
        }),
    };

    Ok(EffectiveRequest {
        method: endpoint.method,
        url,
        headers,
        body,
    })
}

/// Interpolates `{{var}}` templates inside a JSON value tree, walking into string leaves
/// (and object keys) only — numbers, booleans and null pass through untouched. This lets
/// `serde_json` do the escaping, so an interpolated value can never break out of its string
/// literal or inject sibling keys.
fn interpolate_json(
    value: &serde_json::Value,
    scope: &Scope,
) -> Result<serde_json::Value, VarError> {
    Ok(match value {
        serde_json::Value::String(s) => serde_json::Value::String(scope.interpolate(s)?),
        serde_json::Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(interpolate_json(item, scope)?)
            }
            serde_json::Value::Array(out)
        }
        serde_json::Value::Object(map) => {
            let mut out = serde_json::Map::with_capacity(map.len());
            for (k, v) in map {
                out.insert(scope.interpolate(k)?, interpolate_json(v, scope)?);
            }
            serde_json::Value::Object(out)
        }
        other => other.clone(),
    })
}

/// Keeps the auth scheme of an `Authorization` header and hides the credential:
/// `Basic <base64>` becomes `Basic ***`. A value with no scheme is hidden whole.
fn redact_credential(value: &str) -> String {
    match value.split_once(' ') {
        Some((scheme, _)) if !scheme.is_empty() => format!("{scheme} ***"),
        _ => "***".to_string(),
    }
}

pub(crate) fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
