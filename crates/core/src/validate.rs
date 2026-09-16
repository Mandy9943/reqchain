use crate::expr::SUPPORTED;
use crate::model::{Api, Auth, AuthExtract, Body, Endpoint};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity { Error, Warning }

#[derive(Debug, Clone)]
pub struct Diagnostic { pub severity: Severity, pub path: String, pub message: String }

impl Diagnostic {
    fn error(path: impl Into<String>, message: impl Into<String>) -> Diagnostic {
        Diagnostic { severity: Severity::Error, path: path.into(), message: message.into() }
    }
    fn warning(path: impl Into<String>, message: impl Into<String>) -> Diagnostic {
        Diagnostic { severity: Severity::Warning, path: path.into(), message: message.into() }
    }
}

pub fn validate_text(text: &str) -> Vec<Diagnostic> {
    match Api::from_json(text) {
        Err(e) => vec![Diagnostic::error("$", e.to_string())],
        Ok(api) => validate_api(&api),
    }
}

pub fn validate_api(api: &Api) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for (i, ep) in api.endpoints.iter().enumerate() {
        let base = format!("$.endpoints[{i}]");
        if !seen.insert(ep.id.clone()) {
            out.push(Diagnostic::error(format!("{base}.id"), format!("duplicate endpoint id `{}`", ep.id)));
        }
        check_variables(api, ep, &base, &mut out);
        check_auth(api, ep, &base, &mut out);
    }
    check_cycles(api, &mut out);
    out
}

fn referenced_vars(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else { break };
        out.push(after[..end].trim().to_string());
        rest = &after[end + 2..];
    }
    out
}

fn known_var(api: &Api, ep: &Endpoint, name: &str) -> bool {
    if name == "value" { return true } // injection templates
    if let Some(secret) = name.strip_prefix("secret:") {
        // Whether the secret exists is a runtime concern, not a file concern.
        return !secret.trim().is_empty();
    }
    ep.variables.contains_key(name)
        || api.environments.iter().any(|e| e.variables.contains_key(name))
        || api.variables.contains_key(name)
}

fn endpoint_texts(api: &Api, ep: &Endpoint) -> Vec<String> {
    let mut texts = vec![api.base_url.clone(), ep.path.clone()];
    texts.extend(ep.headers.values().cloned());
    texts.extend(ep.query.values().cloned());
    match &ep.body {
        Some(Body::Json { content }) => texts.push(content.to_string()),
        Some(Body::Text { content }) | Some(Body::Xml { content }) => texts.push(content.clone()),
        Some(Body::Form { fields }) => texts.extend(fields.values().cloned()),
        Some(Body::Multipart { fields, files }) => {
            texts.extend(fields.values().cloned());
            texts.extend(files.values().cloned());
        }
        Some(Body::Binary { path }) => texts.push(path.clone()),
        None => {}
    }
    texts
}

fn check_variables(api: &Api, ep: &Endpoint, base: &str, out: &mut Vec<Diagnostic>) {
    for text in endpoint_texts(api, ep) {
        for name in referenced_vars(&text) {
            if !known_var(api, ep, &name) {
                out.push(Diagnostic::error(
                    base,
                    format!(
                        "unknown variable `{name}` in endpoint `{}` — define it on the endpoint, an environment, or the API",
                        ep.id
                    ),
                ));
            }
        }
    }
}

fn check_auth(api: &Api, ep: &Endpoint, base: &str, out: &mut Vec<Diagnostic>) {
    match crate::auth::resolve(api, ep) {
        Auth::Computed { expression, .. } => {
            for name in function_names(expression) {
                if !SUPPORTED.contains(&name.as_str()) {
                    out.push(Diagnostic::error(
                        format!("{base}.auth.expression"),
                        format!(
                            "unknown function `{name}` in endpoint `{}` — supported: {}",
                            ep.id,
                            SUPPORTED.join(", ")
                        ),
                    ));
                }
            }
        }
        Auth::Chained { source, extract, ttl, .. } => {
            if api.endpoint(&source.endpoint).is_none() {
                out.push(Diagnostic::error(
                    format!("{base}.auth.source.endpoint"),
                    format!(
                        "chained auth of `{}` points at endpoint `{}`, which does not exist in this API",
                        ep.id, source.endpoint
                    ),
                ));
            }
            if extract.is_none() {
                out.push(Diagnostic::warning(
                    format!("{base}.auth"),
                    format!(
                        "endpoint `{}` relies on the default extract $.access_token — set auth.extract explicitly if this API returns a different field",
                        ep.id
                    ),
                ));
            }
            if ttl.is_none() {
                out.push(Diagnostic::warning(
                    format!("{base}.auth"),
                    format!(
                        "endpoint `{}` relies on the default ttl $.expires_in — set auth.ttl explicitly if this API reports expiry differently",
                        ep.id
                    ),
                ));
            }
            if let Some(AuthExtract::Body { json_path: Some(p), .. }) = extract {
                if serde_json_path::JsonPath::parse(p).is_err() {
                    out.push(Diagnostic::error(
                        format!("{base}.auth.extract.jsonPath"),
                        format!("invalid jsonPath `{p}`"),
                    ));
                }
            }
            if let Some(AuthExtract::Body { xpath: Some(_), .. }) = extract {
                out.push(Diagnostic::error(
                    format!("{base}.auth.extract.xpath"),
                    "xpath extraction is not implemented yet — use jsonPath or regex".to_string(),
                ));
            }
        }
        _ => {}
    }
}

/// Every identifier immediately followed by `(` is a function call.
fn function_names(expression: &str) -> Vec<String> {
    let bytes = expression.as_bytes();
    let mut out = Vec::new();
    for end in 0..bytes.len() {
        if bytes[end] != b'(' { continue }
        let mut start = end;
        while start > 0 && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_') {
            start -= 1;
        }
        if start < end {
            out.push(String::from_utf8_lossy(&bytes[start..end]).into_owned());
        }
    }
    out
}

fn check_cycles(api: &Api, out: &mut Vec<Diagnostic>) {
    for ep in &api.endpoints {
        let mut path = vec![ep.id.clone()];
        let mut current = ep.clone();
        while let Auth::Chained { source, .. } = crate::auth::resolve(api, &current).clone() {
            if path.contains(&source.endpoint) {
                path.push(source.endpoint.clone());
                out.push(Diagnostic::error(
                    "$.endpoints",
                    format!("auth cycle detected: {}", path.join(" -> ")),
                ));
                break;
            }
            path.push(source.endpoint.clone());
            let Some(next) = api.endpoint(&source.endpoint) else { break };
            current = next.clone();
            if path.len() > crate::chain::MAX_DEPTH + 1 {
                out.push(Diagnostic::error(
                    "$.endpoints",
                    format!(
                        "auth chain deeper than {} levels: {}",
                        crate::chain::MAX_DEPTH,
                        path.join(" -> ")
                    ),
                ));
                break;
            }
        }
    }
}
