use reqchain_core::exec::Runner;
use reqchain_core::model::Api;
use reqchain_core::request;
use reqchain_core::secrets::Secrets;
use reqchain_core::vars::Scope;
use wiremock::matchers::{body_string_contains, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn api_with_base(base: &str) -> Api {
    let text = include_str!("../../../tests/fixtures/exec.json").replace("BASE_URL", base);
    Api::from_json(&text).unwrap()
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
