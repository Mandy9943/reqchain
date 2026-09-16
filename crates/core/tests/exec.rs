use reqchain_core::exec::Runner;
use reqchain_core::model::Api;
use reqchain_core::request::{self, EffectiveBody};
use reqchain_core::secrets::Secrets;
use reqchain_core::shell;
use reqchain_core::vars::Scope;
use reqchain_core::auth;
use wiremock::matchers::{body_string_contains, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn api_with_base(base: &str) -> Api {
    let text = include_str!("../../../tests/fixtures/exec.json").replace("BASE_URL", base);
    Api::from_json(&text).unwrap()
}

/// Builds a minimal single-endpoint Api with a JSON body, where the endpoint-level variable
/// `v` is set to `var_value` and the body content is `body_content` (typically referencing
/// `{{v}}` somewhere inside it).
fn api_with_json_body(var_value: &str, body_content: serde_json::Value) -> Api {
    let doc = serde_json::json!({
        "schemaVersion": 1,
        "id": "json-escaping",
        "name": "Json Escaping",
        "baseUrl": "https://example.test",
        "variables": {},
        "environments": [],
        "auth": { "type": "none" },
        "endpoints": [{
            "id": "ep",
            "name": "Ep",
            "method": "POST",
            "path": "/ep",
            "headers": {},
            "query": {},
            "variables": { "v": var_value },
            "auth": { "type": "none" },
            "body": { "type": "json", "content": body_content }
        }]
    });
    Api::from_json(&doc.to_string()).unwrap()
}

#[tokio::test]
async fn sends_interpolated_json_body_and_reports_metrics() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/echo/1.0"))
        .and(header("x-doc", "12345678"))
        .and(body_string_contains("12345678"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
        .mount(&server)
        .await;

    let api = api_with_base(&server.uri());
    let ep = api.endpoint("echo").unwrap();
    let secrets = Secrets::empty();
    let scope = Scope::new(&api, ep, Some("test"), &secrets);
    let req = request::build(&api, ep, &scope).unwrap();
    assert_eq!(req.url, format!("{}/echo/1.0?trace=on", server.uri()));

    let res = Runner::new().send(&req).await.unwrap();
    assert_eq!(res.status, 200);
    assert!(res.size_bytes > 0);
    assert!(res.headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("content-type")));
}

#[tokio::test]
async fn computed_header_is_evaluated() {
    let today = chrono::Local::now().format("%Y%m%d");
    let expected = format!("{:x}", md5_simple::compute(format!("{today}12345678").as_bytes()));

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/odilo/user"))
        .and(header("user", "12345678"))
        .and(header("hash", expected.as_str()))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let api = api_with_base(&server.uri());
    let ep = api.endpoint("odilo").unwrap();
    let secrets = Secrets::from_map([("API_TOKEN".into(), "t0k".into())]);
    let scope = Scope::new(&api, ep, Some("test"), &secrets);
    let mut req = request::build(&api, ep, &scope).unwrap();
    auth::apply_static(&api, ep, &scope, &mut req).unwrap();

    let res = Runner::new().send(&req).await.unwrap();
    assert_eq!(res.status, 200);
}

#[test]
fn shell_export_masks_secret_values() {
    let text = include_str!("../../../tests/fixtures/exec.json").replace("BASE_URL", "https://example.test");
    let api = Api::from_json(&text).unwrap();
    let ep = api.endpoint("odilo").unwrap();
    let secrets = Secrets::from_map([("API_TOKEN".into(), "sup3rsecret".into())]);
    let scope = Scope::new(&api, ep, Some("test"), &secrets);
    let mut req = request::build(&api, ep, &scope).unwrap();
    auth::apply_static(&api, ep, &scope, &mut req).unwrap();

    let command = shell::to_shell_command(&req.masked(&scope.secret_values()));
    assert!(command.starts_with("curl "));
    assert!(!command.contains("sup3rsecret"));
    assert!(command.contains("***"));
}

fn effective_json_body(req: &request::EffectiveRequest) -> &str {
    match req.body.as_ref().unwrap() {
        EffectiveBody::Text { content, .. } => content.as_str(),
        other => panic!("expected a Text body, got {other:?}"),
    }
}

#[test]
fn json_body_escapes_quotes_and_backslashes_in_interpolated_values() {
    let value = "a\"b\\c";
    let api = api_with_json_body(value, serde_json::json!({ "field": "{{v}}" }));
    let ep = api.endpoint("ep").unwrap();
    let secrets = Secrets::empty();
    let scope = Scope::new(&api, ep, None, &secrets);
    let req = request::build(&api, ep, &scope).unwrap();

    let content = effective_json_body(&req);
    let parsed: serde_json::Value = serde_json::from_str(content)
        .unwrap_or_else(|e| panic!("interpolated body must still be valid JSON: {e}\nbody: {content}"));
    assert_eq!(parsed["field"], serde_json::Value::String(value.to_string()));
}

#[test]
fn json_body_does_not_let_an_interpolated_value_inject_a_sibling_key() {
    let value = "\",\"injected\":\"x";
    let api = api_with_json_body(value, serde_json::json!({ "field": "{{v}}" }));
    let ep = api.endpoint("ep").unwrap();
    let secrets = Secrets::empty();
    let scope = Scope::new(&api, ep, None, &secrets);
    let req = request::build(&api, ep, &scope).unwrap();

    let content = effective_json_body(&req);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    assert_eq!(parsed["field"], serde_json::Value::String(value.to_string()));
    assert!(parsed.get("injected").is_none(), "value must not inject a sibling key: {content}");
    assert_eq!(parsed.as_object().unwrap().len(), 1);
}

#[test]
fn json_body_preserves_number_boolean_and_null_literals() {
    let api = api_with_json_body(
        "12345678",
        serde_json::json!({ "s": "{{v}}", "n": 5, "b": true, "nil": null }),
    );
    let ep = api.endpoint("ep").unwrap();
    let secrets = Secrets::empty();
    let scope = Scope::new(&api, ep, None, &secrets);
    let req = request::build(&api, ep, &scope).unwrap();

    let content = effective_json_body(&req);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    assert_eq!(parsed["s"], serde_json::Value::String("12345678".to_string()));
    assert_eq!(parsed["n"], serde_json::json!(5));
    assert_eq!(parsed["b"], serde_json::json!(true));
    assert_eq!(parsed["nil"], serde_json::Value::Null);
}

#[test]
fn masked_redacts_secret_values_in_multipart_fields_but_not_file_paths() {
    let doc = serde_json::json!({
        "schemaVersion": 1,
        "id": "multipart-mask",
        "name": "Multipart Mask",
        "baseUrl": "https://example.test",
        "variables": {},
        "environments": [],
        "auth": { "type": "none" },
        "endpoints": [{
            "id": "ep",
            "name": "Ep",
            "method": "POST",
            "path": "/ep",
            "headers": {},
            "query": {},
            "variables": {},
            "auth": { "type": "none" },
            "body": {
                "type": "multipart",
                "fields": { "apiKey": "{{secret:API_KEY}}" },
                "files": { "attachment": "/tmp/sup3rsecret-report.pdf" }
            }
        }]
    });
    let api = Api::from_json(&doc.to_string()).unwrap();
    let ep = api.endpoint("ep").unwrap();
    let secrets = Secrets::from_map([("API_KEY".into(), "sup3rsecret".into())]);
    let scope = Scope::new(&api, ep, None, &secrets);
    let req = request::build(&api, ep, &scope).unwrap();

    match req.body.as_ref().unwrap() {
        EffectiveBody::Multipart { fields, .. } => {
            assert_eq!(fields, &vec![("apiKey".to_string(), "sup3rsecret".to_string())]);
        }
        other => panic!("expected a Multipart body, got {other:?}"),
    }

    let masked = req.masked(&scope.secret_values());
    match masked.body.as_ref().unwrap() {
        EffectiveBody::Multipart { fields, files } => {
            assert_eq!(fields, &vec![("apiKey".to_string(), "***".to_string())]);
            // File paths must never be treated as secret-bearing content.
            assert_eq!(files, &vec![("attachment".to_string(), "/tmp/sup3rsecret-report.pdf".to_string())]);
        }
        other => panic!("expected a Multipart body, got {other:?}"),
    }
}
