use crate::request::{EffectiveBody, EffectiveRequest};

pub fn to_shell_command(req: &EffectiveRequest) -> String {
    let q = |s: &str| format!("'{}'", s.replace('\'', r"'\''"));
    let mut parts = vec![
        "curl".to_string(),
        "-X".into(),
        format!("{:?}", req.method).to_uppercase(),
        q(&req.url),
    ];
    for (k, v) in &req.headers {
        parts.push("-H".into());
        parts.push(q(&format!("{k}: {v}")));
    }
    match &req.body {
        None => {}
        Some(EffectiveBody::Text {
            content_type,
            content,
        }) => {
            parts.push("-H".into());
            parts.push(q(&format!("content-type: {content_type}")));
            parts.push("--data".into());
            parts.push(q(content));
        }
        Some(EffectiveBody::Form { fields }) => {
            for (k, v) in fields {
                parts.push("--data-urlencode".into());
                parts.push(q(&format!("{k}={v}")));
            }
        }
        Some(EffectiveBody::Multipart { fields, files }) => {
            for (k, v) in fields {
                parts.push("-F".into());
                parts.push(q(&format!("{k}={v}")))
            }
            for (k, p) in files {
                parts.push("-F".into());
                parts.push(q(&format!("{k}=@{p}")))
            }
        }
        Some(EffectiveBody::Binary { path }) => {
            parts.push("--data-binary".into());
            parts.push(q(&format!("@{path}")));
        }
    }
    parts.join(" ")
}
