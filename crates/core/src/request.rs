use crate::model::{Api, Body, Endpoint, Method};
use crate::vars::{Scope, VarError};

#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error(transparent)]
    Var(#[from] VarError),
}

#[derive(Debug, Clone, PartialEq)]
pub enum EffectiveBody {
    Text { content_type: String, content: String },
    Form { fields: Vec<(String, String)> },
    Multipart { fields: Vec<(String, String)>, files: Vec<(String, String)> },
    Binary { path: String },
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
        let mask = |s: &String| {
            let mut out = s.clone();
            for secret in secrets {
                if !secret.is_empty() { out = out.replace(secret.as_str(), "***") }
            }
            out
        };
        EffectiveRequest {
            method: self.method,
            url: mask(&self.url),
            headers: self.headers.iter().map(|(k, v)| (k.clone(), mask(v))).collect(),
            body: self.body.as_ref().map(|b| match b {
                EffectiveBody::Text { content_type, content } =>
                    EffectiveBody::Text { content_type: content_type.clone(), content: mask(content) },
                EffectiveBody::Form { fields } =>
                    EffectiveBody::Form { fields: fields.iter().map(|(k, v)| (k.clone(), mask(v))).collect() },
                other => other.clone(),
            }),
        }
    }
}

pub fn build(api: &Api, endpoint: &Endpoint, scope: &Scope) -> Result<EffectiveRequest, BuildError> {
    let base = scope.interpolate(&api.base_url)?;
    let path = scope.interpolate(&endpoint.path)?;
    let mut url = format!("{}{}", base.trim_end_matches('/'), path);

    if !endpoint.query.is_empty() {
        let mut pairs = Vec::new();
        for (k, v) in &endpoint.query {
            pairs.push(format!("{}={}", urlencode(k), urlencode(&scope.interpolate(v)?)));
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
            content: scope.interpolate(&content.to_string())?,
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
            for (k, v) in fields { out.push((k.clone(), scope.interpolate(v)?)) }
            Some(EffectiveBody::Form { fields: out })
        }
        Some(Body::Multipart { fields, files }) => {
            let mut f = Vec::new();
            for (k, v) in fields { f.push((k.clone(), scope.interpolate(v)?)) }
            let mut fl = Vec::new();
            for (k, v) in files { fl.push((k.clone(), scope.interpolate(v)?)) }
            Some(EffectiveBody::Multipart { fields: f, files: fl })
        }
        Some(Body::Binary { path }) => Some(EffectiveBody::Binary { path: scope.interpolate(path)? }),
    };

    Ok(EffectiveRequest { method: endpoint.method, url, headers, body })
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
