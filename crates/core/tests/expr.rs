use reqchain_core::expr;
use reqchain_core::model::Api;
use reqchain_core::secrets::Secrets;
use reqchain_core::vars::Scope;

macro_rules! eval {
    ($src:expr) => {{
        let api = Api::from_json(include_str!("../../../tests/fixtures/precedence.json")).unwrap();
        let secrets = Secrets::empty();
        let ep = api.endpoint("probe").unwrap();
        let scope = Scope::new(&api, ep, Some("test"), &secrets);
        expr::eval($src, &scope)
    }};
}

#[test]
fn hashes_and_encodes() {
    assert_eq!(eval!(r#"md5("abc")"#).unwrap(), "900150983cd24fb0d6963f7d28e17f72");
    assert_eq!(eval!(r#"sha1("abc")"#).unwrap(), "a9993e364706816aba3e25717850c26c9cd0d89d");
    assert_eq!(eval!(r#"sha256("abc")"#).unwrap(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    assert_eq!(eval!(r#"base64("abc")"#).unwrap(), "YWJj");
}

#[test]
fn concatenates_strings_and_variables() {
    assert_eq!(eval!(r#""a" + "b" + {{what}}"#).unwrap(), "abapi");
}

#[test]
fn now_formats_current_local_date() {
    let today = chrono::Local::now().format("%Y%m%d").to_string();
    assert_eq!(eval!(r#"now("YYYYMMDD")"#).unwrap(), today);
}

#[test]
fn nests_calls_over_concatenation() {
    let today = chrono::Local::now().format("%Y%m%d").to_string();
    let expected = format!("{:x}", md5_simple::compute(format!("{today}api").as_bytes()));
    assert_eq!(eval!(r#"md5(now("YYYYMMDD") + {{what}})"#).unwrap(), expected);
}

#[test]
fn unknown_function_lists_the_supported_set() {
    let err = eval!(r#"sha512("abc")"#).unwrap_err().to_string();
    assert!(err.contains("sha512"), "got: {err}");
    assert!(err.contains("sha256"), "error must list supported functions: {err}");
}

#[test]
fn unbalanced_parens_are_a_syntax_error() {
    assert!(eval!(r#"md5("abc""#).is_err());
}

#[test]
fn wrong_arity_is_reported() {
    let err = eval!(r#"md5("a", "b")"#).unwrap_err().to_string();
    assert!(err.contains("md5"), "got: {err}");
}
