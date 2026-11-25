//! End-to-end tests over the router, without a socket.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt as _;
use reposerve::config::Config;
use reposerve::web::server::AppState;
use tower::ServiceExt as _;

fn site() -> (tempfile::TempDir, Arc<AppState>) {
    let directory = tempfile::tempdir().expect("a temporary directory");
    std::fs::create_dir_all(directory.path().join("docs")).expect("a directory");
    std::fs::write(
        directory.path().join("README.md"),
        "---\ntitle: The handbook\n---\n\n# Welcome\n\nSee [the guide](docs/guide.md).\n",
    )
    .expect("a document");
    std::fs::write(
        directory.path().join("docs/guide.md"),
        "# Guide\n\n| A | B |\n| --- | --- |\n| 1 | 2 |\n\n```rust\nfn main() {}\n```\n",
    )
    .expect("a document");
    std::fs::write(directory.path().join("docs/note.txt"), "plain words\n").expect("a file");
    std::fs::write(
        directory.path().join("plot.ipynb"),
        r#"{"nbformat":4,"nbformat_minor":5,"metadata":{"language_info":{"name":"python"}},"cells":[{"cell_type":"code","source":"x","outputs":[{"output_type":"display_data","data":{"text/html":["<b onclick=\"x()\">bold</b><script>bad()</script>"]}}]}]}"#,
    )
    .expect("a notebook");
    std::fs::write(directory.path().join("logo.png"), [0x89, b'P', b'N', b'G']).expect("an image");

    let config = Config {
        root: directory.path().to_path_buf(),
        extensions: reposerve::site::index::Extensions::parse("md,ipynb"),
        ..Config::default()
    };
    let state = Arc::new(AppState::new(config).expect("the state is built"));
    (directory, state)
}

async fn get(state: &Arc<AppState>, uri: &str) -> (StatusCode, axum::http::HeaderMap, String) {
    let request = Request::builder()
        .uri(uri)
        .body(Body::empty())
        .expect("a request");
    let response = reposerve::web::routes::router(Arc::clone(state))
        .oneshot(request)
        .await
        .expect("a response");
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("a body")
        .to_bytes();
    (
        status,
        headers,
        String::from_utf8_lossy(&bytes).into_owned(),
    )
}

#[tokio::test]
async fn the_root_shows_its_readme() {
    let (_directory, state) = site();
    let (status, headers, body) = get(&state, "/").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get(header::CONTENT_TYPE).unwrap(),
        "text/html; charset=utf-8"
    );
    assert!(body.contains("<h1 id=\"welcome\">"), "{body}");
    assert!(
        body.contains("The handbook ·"),
        "the front matter names the page: {body}"
    );
    assert!(body.contains("class=\"tree\""), "{body}");
}

#[tokio::test]
async fn an_unknown_path_is_a_page_not_a_crash() {
    let (_directory, state) = site();
    let (status, _headers, body) = get(&state, "/missing.md").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body.contains("Not found"), "{body}");
}

#[tokio::test]
async fn a_traversal_attempt_is_refused() {
    let (_directory, state) = site();
    let (status, _headers, _body) = get(&state, "/../Cargo.toml").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_document_is_revalidated_and_then_answers_304() {
    let (_directory, state) = site();
    let (status, headers, _body) = get(&state, "/docs/guide.md").await;
    assert_eq!(status, StatusCode::OK);
    let etag = headers
        .get(header::ETAG)
        .expect("an entity tag")
        .to_str()
        .expect("ascii")
        .to_string();
    assert_eq!(
        headers.get(header::CACHE_CONTROL).unwrap(),
        "private, no-cache"
    );

    let request = Request::builder()
        .uri("/docs/guide.md")
        .header(header::IF_NONE_MATCH, etag)
        .body(Body::empty())
        .expect("a request");
    let response = reposerve::web::routes::router(Arc::clone(&state))
        .oneshot(request)
        .await
        .expect("a response");
    assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
}

#[tokio::test]
async fn every_answer_carries_the_security_headers() {
    let (_directory, state) = site();
    let (_status, headers, _body) = get(&state, "/docs/guide.md").await;
    let policy = headers
        .get(header::CONTENT_SECURITY_POLICY)
        .expect("a policy")
        .to_str()
        .expect("ascii");
    assert!(policy.contains("script-src 'self'"), "{policy}");
    assert_eq!(
        headers.get(header::X_CONTENT_TYPE_OPTIONS).unwrap(),
        "nosniff"
    );
    assert_eq!(headers.get(header::REFERRER_POLICY).unwrap(), "no-referrer");
}

#[tokio::test]
async fn an_asset_is_served_from_a_fingerprinted_url() {
    let (_directory, state) = site();
    let url = reposerve::web::assets::url(reposerve::web::assets::APP_CSS);
    let (status, headers, body) = get(&state, &url).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get(header::CONTENT_TYPE).unwrap(),
        "text/css; charset=utf-8"
    );
    assert!(headers
        .get(header::CACHE_CONTROL)
        .unwrap()
        .to_str()
        .unwrap()
        .contains("immutable"));
    assert!(body.contains("--accent"), "the stylesheet arrived");

    let (missing, _headers, _body) = get(&state, "/assets/abc/nothing.css").await;
    assert_eq!(missing, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_notebook_output_is_sanitised_before_it_reaches_the_page() {
    let (_directory, state) = site();
    let (status, _headers, body) = get(&state, "/plot.ipynb").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("<b>bold</b>"), "{body}");
    assert!(!body.contains("onclick"), "{body}");
    assert!(!body.contains("bad()"), "the stored script is gone: {body}");
}

#[tokio::test]
async fn the_filter_cookie_narrows_the_tree() {
    let (_directory, state) = site();
    let request = Request::builder()
        .uri("/")
        .header(header::COOKIE, "reposerve_ext=txt")
        .body(Body::empty())
        .expect("a request");
    let response = reposerve::web::routes::router(Arc::clone(&state))
        .oneshot(request)
        .await
        .expect("a response");
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("a body")
        .to_bytes();
    let body = String::from_utf8_lossy(&bytes);
    assert!(body.contains("note.txt"), "{body}");
    assert!(!body.contains("guide.md"), "{body}");
}

#[tokio::test]
async fn a_filter_that_hides_the_current_page_returns_to_the_root() {
    let (_directory, state) = site();
    let (status, headers, _body) = get(&state, "/filter/apply?ext=ipynb&next=/docs/guide.md").await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        headers.get(header::LOCATION).expect("a location"),
        "/",
        "a hidden page is not a dead end"
    );
}

#[tokio::test]
async fn a_filter_that_keeps_the_current_page_stays_on_it() {
    let (_directory, state) = site();
    let (status, headers, _body) = get(&state, "/filter/apply?ext=md&next=/docs/guide.md").await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        headers.get(header::LOCATION).expect("a location"),
        "/docs/guide.md"
    );
}

#[tokio::test]
async fn a_token_is_asked_for_once_and_then_kept_in_a_cookie() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(directory.path().join("README.md"), "# Hi\n").expect("a document");
    let config = Config {
        root: directory.path().to_path_buf(),
        token: Some("s3cret".to_string()),
        ..Config::default()
    };
    let state = Arc::new(AppState::new(config).expect("the state is built"));

    let (status, _headers, body) = get(&state, "/").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(body.contains("Token required"), "{body}");

    let request = Request::builder()
        .uri("/?token=s3cret")
        .body(Body::empty())
        .expect("a request");
    let response = reposerve::web::routes::router(Arc::clone(&state))
        .oneshot(request)
        .await
        .expect("a response");
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert!(response
        .headers()
        .get(header::SET_COOKIE)
        .expect("a cookie")
        .to_str()
        .expect("ascii")
        .contains("reposerve_token=s3cret"));

    let request = Request::builder()
        .uri("/")
        .header(header::COOKIE, "reposerve_token=s3cret")
        .body(Body::empty())
        .expect("a request");
    let response = reposerve::web::routes::router(Arc::clone(&state))
        .oneshot(request)
        .await
        .expect("a response");
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn a_page_is_compressed_when_the_browser_asks() {
    let (_directory, state) = site();
    let request = Request::builder()
        .uri("/docs/guide.md")
        .header(header::ACCEPT_ENCODING, "gzip")
        .body(Body::empty())
        .expect("a request");
    let response = reposerve::web::routes::router(Arc::clone(&state))
        .oneshot(request)
        .await
        .expect("a response");
    assert_eq!(
        response.headers().get(header::CONTENT_ENCODING).unwrap(),
        "gzip"
    );
}

#[tokio::test]
async fn the_palette_index_lists_only_shown_files() {
    let (_directory, state) = site();
    let (status, headers, body) = get(&state, "/__index.json").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get(header::CONTENT_TYPE).unwrap(),
        "application/json; charset=utf-8"
    );
    assert!(body.contains("docs/guide.md"), "{body}");
    assert!(!body.contains("note.txt"), "the filter hides it: {body}");
}

#[tokio::test]
async fn the_manifest_names_the_site() {
    let (_directory, state) = site();
    let (status, _headers, body) = get(&state, "/manifest.webmanifest").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("\"start_url\":\"/\""), "{body}");
    assert!(body.contains("icon"), "{body}");
}

#[tokio::test]
async fn a_large_placeholder_file_reports_its_size() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(directory.path().join("blob.bin"), vec![0u8; 1024]).expect("a file");
    let config = Config {
        root: directory.path().to_path_buf(),
        extensions: reposerve::site::index::Extensions::parse("bin"),
        ..Config::default()
    };
    let state = Arc::new(AppState::new(config).expect("the state is built"));
    let (status, _headers, body) = get(&state, "/blob.bin").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("1.0 KiB"), "{body}");
    assert!(body.contains("not text"), "{body}");
}
