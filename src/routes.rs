//! The HTTP surface: one route per kind of page, plus the theme control, the
//! embedded Mermaid script and the optional token gate.

use std::convert::Infallible;
use std::io::Write as _;
use std::sync::{Arc, OnceLock};

use axum::body::Bytes;
use axum::extract::{Path, Query, Request, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Router;
use flate2::write::GzEncoder;
use flate2::Compression;
use percent_encoding::percent_decode_str;
use serde::Deserialize;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::{Stream, StreamExt};

use crate::error::Error;
use crate::highlight;
use crate::index::{normalise_request_path, Extensions, RepoIndex};
use crate::notebook;
use crate::pages::{self, Theme, View};
use crate::render::markdown_to_html;
use crate::server::AppState;

/// The name of the cookie a token-carrying first visit leaves behind.
const TOKEN_COOKIE: &str = "reposerve_token";

/// The name of the cookie that carries the light or dark choice.
const THEME_COOKIE: &str = "reposerve_theme";

/// The name of the cookie that carries the file-type filter.
const FILTER_COOKIE: &str = "reposerve_ext";

/// Largest file the source view renders. Anything larger is reported by size.
const SOURCE_LIMIT: usize = 512 * 1024;

/// The diagram script, embedded so the server needs no network.
const MERMAID_JS: &[u8] = include_bytes!("../assets/mermaid.min.js");

/// One answer from a handler, before it becomes an HTTP response.
enum Body {
    /// A complete HTML page.
    Html(String),
    /// Raw bytes with their content type.
    Bytes(&'static str, Vec<u8>),
    /// A move to another address, used when a directory chain collapses.
    Redirect(String),
}

impl Body {
    fn into_response(self, status: StatusCode) -> Response {
        match self {
            Self::Html(page) => (
                status,
                [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
                page,
            )
                .into_response(),
            Self::Bytes(content_type, bytes) => {
                (status, [(header::CONTENT_TYPE, content_type)], bytes).into_response()
            }
            Self::Redirect(target) => Redirect::to(&target).into_response(),
        }
    }
}

/// Builds the router. The token gate wraps every route when one is configured.
pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", axum::routing::get(home))
        .route("/search", axum::routing::get(search))
        .route("/__reload", axum::routing::get(reload))
        .route("/filter", axum::routing::get(filter_page))
        .route("/filter/apply", axum::routing::get(apply_filter))
        .route("/theme/{mode}", axum::routing::get(switch_theme))
        .route("/assets/mermaid.min.js", axum::routing::get(mermaid_script))
        .route("/assets/highlight.css", axum::routing::get(highlight_css))
        .route("/favicon.ico", axum::routing::get(favicon))
        .route("/{*path}", axum::routing::get(entry))
        .layer(axum::middleware::from_fn_with_state(
            Arc::clone(&state),
            require_token,
        ))
        .with_state(state)
}

/// Serves the landing page.
async fn home(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    let theme = theme_from(&headers);
    let extensions = extensions_from(&headers, &state.config.extensions);
    run(state, move |state| {
        let index = RepoIndex::load(&state.config)?;
        let view = View {
            theme,
            extensions,
            config: &state.config,
        };
        Ok((
            StatusCode::OK,
            Body::Html(pages::directory_page(&index, "", view)),
        ))
    })
    .await
}

#[derive(Debug, Deserialize)]
struct SearchQuery {
    q: Option<String>,
}

/// Serves the search results for a path fragment.
async fn search(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<SearchQuery>,
) -> Response {
    let theme = theme_from(&headers);
    let extensions = extensions_from(&headers, &state.config.extensions);
    let needle = query.q.unwrap_or_default();
    run(state, move |state| {
        let index = RepoIndex::load(&state.config)?;
        let view = View {
            theme,
            extensions,
            config: &state.config,
        };
        let matches = index.search(&needle);
        Ok((
            StatusCode::OK,
            Body::Html(pages::search_page(&index, &needle, &matches, view)),
        ))
    })
    .await
}

/// Streams a message whenever a served file changes.
///
/// The page opens one `EventSource` and reloads on each message. A keep-alive
/// comment holds the connection open through a proxy that would drop an idle one.
async fn reload(
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = BroadcastStream::new(state.reload.subscribe())
        .filter_map(|message| message.ok().map(|()| Ok(Event::default().data("reload"))));
    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Answers the browser's automatic favicon request without a file.
async fn favicon() -> StatusCode {
    StatusCode::NO_CONTENT
}

#[derive(Debug, Deserialize)]
struct NextQuery {
    next: Option<String>,
}

/// Stores the chosen theme and returns the reader to the page they were on.
///
/// The theme is a cookie rather than a script, so the server writes it into the
/// markup and the first paint already has the right colours.
async fn switch_theme(Path(mode): Path<String>, Query(query): Query<NextQuery>) -> Response {
    let theme = Theme::from_value(&mode);
    let mut response = Redirect::to(&safe_target(query.next.as_deref())).into_response();
    if let Ok(cookie) = HeaderValue::from_str(&format!(
        "{THEME_COOKIE}={}; Path=/; SameSite=Lax; Max-Age=31536000",
        theme.value()
    )) {
        response.headers_mut().insert(header::SET_COOKIE, cookie);
    }
    response
}

/// Only a path on this server is a valid return address.
fn safe_target(next: Option<&str>) -> String {
    match next {
        Some(value)
            if value.starts_with('/')
                && !value.starts_with("//")
                && !value.contains(['\r', '\n']) =>
        {
            value.to_string()
        }
        _ => "/".to_string(),
    }
}

/// Serves the embedded diagram script.
async fn mermaid_script(headers: HeaderMap) -> Response {
    asset(
        &headers,
        "text/javascript; charset=utf-8",
        MERMAID_JS,
        mermaid_gzip(),
    )
}

/// Serves the syntax palette for both colour schemes.
async fn highlight_css(headers: HeaderMap) -> Response {
    asset(
        &headers,
        "text/css; charset=utf-8",
        highlight_css_bytes(),
        highlight_css_gzip(),
    )
}

/// Serves one embedded asset, gzipped when the browser accepts it and cached for a year.
fn asset(
    headers: &HeaderMap,
    content_type: &'static str,
    raw: &'static [u8],
    compressed: &'static [u8],
) -> Response {
    let use_gzip = headers
        .get(header::ACCEPT_ENCODING)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.contains("gzip"))
        && !compressed.is_empty();

    let body = if use_gzip {
        Bytes::from_static(compressed)
    } else {
        Bytes::from_static(raw)
    };
    let mut response =
        (StatusCode::OK, [(header::CONTENT_TYPE, content_type)], body).into_response();
    let headers = response.headers_mut();
    if use_gzip {
        headers.insert(header::CONTENT_ENCODING, HeaderValue::from_static("gzip"));
    }
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=31536000, immutable"),
    );
    headers.insert(header::VARY, HeaderValue::from_static("Accept-Encoding"));
    response
}

fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::best());
    match encoder.write_all(bytes).and_then(|()| encoder.finish()) {
        Ok(compressed) => compressed,
        Err(error) => {
            eprintln!("error: cannot compress an asset: {error}");
            Vec::new()
        }
    }
}

fn mermaid_gzip() -> &'static [u8] {
    static COMPRESSED: OnceLock<Vec<u8>> = OnceLock::new();
    COMPRESSED.get_or_init(|| gzip(MERMAID_JS))
}

fn highlight_css_bytes() -> &'static [u8] {
    static CSS: OnceLock<String> = OnceLock::new();
    CSS.get_or_init(|| highlight::global().css()).as_bytes()
}

fn highlight_css_gzip() -> &'static [u8] {
    static COMPRESSED: OnceLock<Vec<u8>> = OnceLock::new();
    COMPRESSED.get_or_init(|| gzip(highlight_css_bytes()))
}

#[derive(Debug, Deserialize)]
struct EntryQuery {
    raw: Option<String>,
}

/// Serves one path: a directory listing, a rendered document, or a source view.
async fn entry(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(raw_path): Path<String>,
    Query(query): Query<EntryQuery>,
) -> Response {
    let theme = theme_from(&headers);
    let extensions = extensions_from(&headers, &state.config.extensions);
    let wants_raw = query.raw.is_some_and(|value| value != "0");
    run(state, move |state| {
        let index = RepoIndex::load(&state.config)?;
        let view = View {
            theme,
            extensions,
            config: &state.config,
        };
        let Some(path) = normalise_request_path(&raw_path) else {
            return Ok(not_found(&index, &raw_path, view));
        };
        if wants_raw {
            return raw_answer(&index, &path);
        }
        if index.has_file(&path) {
            return file_answer(&index, &path, view);
        }
        if index.has_directory(&path) {
            // A chain of directories that holds one branch is one entry, so a
            // reader who asks for its top lands on its deepest directory.
            if !path.is_empty() {
                if let Some(target) = index.collapsed_directory(&path) {
                    if target != path {
                        return Ok((
                            StatusCode::OK,
                            Body::Redirect(format!("/{}", pages::href(&target))),
                        ));
                    }
                }
            }
            return Ok((
                StatusCode::OK,
                Body::Html(pages::directory_page(&index, &path, view)),
            ));
        }
        Ok(not_found(&index, &path, view))
    })
    .await
}

fn not_found(index: &RepoIndex, path: &str, view: View) -> (StatusCode, Body) {
    (
        StatusCode::NOT_FOUND,
        Body::Html(pages::not_found_page(index, path, view)),
    )
}

/// The bytes of one indexed file, unrendered.
fn raw_answer(index: &RepoIndex, path: &str) -> Result<(StatusCode, Body), Error> {
    let bytes = index.read(path)?;
    Ok((
        StatusCode::OK,
        Body::Bytes("text/plain; charset=utf-8", bytes),
    ))
}

/// One indexed file, rendered according to what it is.
fn file_answer(index: &RepoIndex, path: &str, view: View) -> Result<(StatusCode, Body), Error> {
    if notebook::is_notebook(path) {
        let bytes = index.read(path)?;
        let source = String::from_utf8_lossy(&bytes);
        return match notebook::render(&source) {
            Ok(cells) => Ok((
                StatusCode::OK,
                Body::Html(pages::notebook_page(index, path, &cells, view)),
            )),
            // Not valid notebook JSON: show it as source rather than failing.
            Err(_) => source_answer(index, path, view),
        };
    }
    if is_markdown(path) {
        let bytes = index.read(path)?;
        let source = String::from_utf8_lossy(&bytes);
        let html = markdown_to_html(&source);
        return Ok((
            StatusCode::OK,
            Body::Html(pages::document_page(index, path, &html, view)),
        ));
    }
    if let Some(content_type) = media_type(path) {
        let bytes = index.read(path)?;
        return Ok((StatusCode::OK, Body::Bytes(content_type, bytes)));
    }

    source_answer(index, path, view)
}

/// A file shown as numbered text, highlighted when its type is known.
fn source_answer(index: &RepoIndex, path: &str, view: View) -> Result<(StatusCode, Body), Error> {
    let bytes = index.read(path)?;
    let size = bytes.len();
    if size > SOURCE_LIMIT || is_binary(&bytes) {
        return Ok((
            StatusCode::OK,
            Body::Html(pages::binary_page(index, path, size, view)),
        ));
    }
    let source = String::from_utf8_lossy(&bytes);
    let highlighted = highlight::global().for_path(path, &source);
    Ok((
        StatusCode::OK,
        Body::Html(pages::source_page(
            index,
            path,
            &source,
            size,
            view,
            highlighted.as_deref(),
        )),
    ))
}

fn is_markdown(path: &str) -> bool {
    extension(path).is_some_and(|extension| {
        extension.eq_ignore_ascii_case("md") || extension.eq_ignore_ascii_case("markdown")
    })
}

/// A NUL byte in the first block means the file is not text.
fn is_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8192).any(|byte| *byte == 0)
}

fn extension(path: &str) -> Option<&str> {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.rsplit_once('.').map(|(_, extension)| extension)
}

/// Content types served as bytes rather than as a page.
fn media_type(path: &str) -> Option<&'static str> {
    match path
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_lowercase())
    {
        Some(extension) => match extension.as_str() {
            "png" => Some("image/png"),
            "jpg" | "jpeg" => Some("image/jpeg"),
            "gif" => Some("image/gif"),
            "webp" => Some("image/webp"),
            "avif" => Some("image/avif"),
            "ico" => Some("image/x-icon"),
            "svg" => Some("image/svg+xml"),
            "pdf" => Some("application/pdf"),
            "woff" => Some("font/woff"),
            "woff2" => Some("font/woff2"),
            "ttf" => Some("font/ttf"),
            _ => None,
        },
        None => None,
    }
}

/// Runs a blocking page build off the async worker and maps its failure to 500.
async fn run<F>(state: Arc<AppState>, work: F) -> Response
where
    F: FnOnce(Arc<AppState>) -> Result<(StatusCode, Body), Error> + Send + 'static,
{
    let built = tokio::task::spawn_blocking(move || work(Arc::clone(&state))).await;
    let mut response = match built {
        Ok(Ok((status, body))) => body.into_response(status),
        Ok(Err(error)) => {
            eprintln!("error: {error}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                [("content-type", "text/plain; charset=utf-8")],
                format!("{error}\n"),
            )
                .into_response()
        }
        Err(error) => {
            eprintln!("error: the page build panicked: {error}");
            (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response()
        }
    };
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// Refuses a request that carries neither the token cookie nor a matching token.
async fn require_token(
    State(state): State<Arc<AppState>>,
    request: Request,
    next: Next,
) -> Response {
    let Some(expected) = state.config.token.as_ref() else {
        return next.run(request).await;
    };

    let query = request.uri().query().unwrap_or_default();
    let pairs: Vec<&str> = query.split('&').filter(|pair| !pair.is_empty()).collect();
    let supplied = pairs
        .iter()
        .find_map(|pair| pair.strip_prefix("token="))
        .map(|value| percent_decode_str(value).decode_utf8_lossy().into_owned());

    if let Some(supplied) = supplied {
        if supplied == *expected {
            let remaining: Vec<&str> = pairs
                .iter()
                .copied()
                .filter(|pair| !pair.starts_with("token="))
                .collect();
            let path = request.uri().path();
            let target = if remaining.is_empty() {
                path.to_string()
            } else {
                format!("{path}?{}", remaining.join("&"))
            };
            let mut response = Redirect::to(&target).into_response();
            if let Ok(cookie) = HeaderValue::from_str(&format!(
                "{TOKEN_COOKIE}={expected}; Path=/; HttpOnly; SameSite=Lax; Max-Age=2592000"
            )) {
                response.headers_mut().insert(header::SET_COOKIE, cookie);
            }
            return response;
        }
    }

    if cookie(request.headers(), TOKEN_COOKIE).is_some_and(|value| value == *expected) {
        return next.run(request).await;
    }

    match RepoIndex::load(&state.config) {
        Ok(index) => (
            StatusCode::UNAUTHORIZED,
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            pages::token_page(
                &index,
                View {
                    theme: theme_from(request.headers()),
                    extensions: state.config.extensions.clone(),
                    config: &state.config,
                },
            ),
        )
            .into_response(),
        Err(_) => StatusCode::UNAUTHORIZED.into_response(),
    }
}

/// Reads the light or dark choice from the request cookies.
fn theme_from(headers: &HeaderMap) -> Theme {
    cookie(headers, THEME_COOKIE).map_or(Theme::Auto, Theme::from_value)
}

/// Reads the file-type filter from the request cookies, falling back to the
/// extensions the configuration names.
fn extensions_from(headers: &HeaderMap, default: &Extensions) -> Extensions {
    cookie(headers, FILTER_COOKIE).map_or_else(|| default.clone(), Extensions::parse)
}

/// The form that chooses which file types are shown.
async fn filter_page(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    let theme = theme_from(&headers);
    let extensions = extensions_from(&headers, &state.config.extensions);
    run(state, move |state| {
        let index = RepoIndex::load(&state.config)?;
        let view = View {
            theme,
            extensions,
            config: &state.config,
        };
        Ok((
            StatusCode::OK,
            Body::Html(pages::filter_page(&index, "", view)),
        ))
    })
    .await
}

/// Stores the chosen file types and returns the reader to the page they were on.
async fn apply_filter(Query(params): Query<Vec<(String, String)>>) -> Response {
    let mut extensions = String::new();
    let mut extra = String::new();
    let mut next = "/".to_string();
    for (key, value) in params {
        match key.as_str() {
            "ext" => {
                extensions.push(',');
                extensions.push_str(&value);
            }
            "extra" => {
                extra.push(',');
                extra.push_str(&value);
            }
            "next" => next = value,
            _ => {}
        }
    }
    extensions.push_str(&extra);
    let extensions = Extensions::parse(&extensions);
    let mut response = Redirect::to(&safe_target(Some(&next))).into_response();
    if let Ok(cookie) = HeaderValue::from_str(&format!(
        "{FILTER_COOKIE}={}; Path=/; SameSite=Lax; Max-Age=31536000",
        extensions.value()
    )) {
        response.headers_mut().insert(header::SET_COOKIE, cookie);
    }
    response
}

fn cookie<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    let cookies = headers.get(header::COOKIE)?.to_str().ok()?;
    cookies.split(';').find_map(|pair| {
        pair.trim()
            .strip_prefix(name)
            .and_then(|rest| rest.strip_prefix('='))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_files_are_recognised_by_either_extension() {
        assert!(is_markdown("docs/README.md"));
        assert!(is_markdown("work/notes.markdown"));
        assert!(!is_markdown("crates/main.rs"));
    }

    #[test]
    fn images_and_documents_pass_through_by_type() {
        assert_eq!(media_type("logo.png"), Some("image/png"));
        assert_eq!(media_type("docs/manual.PDF"), Some("application/pdf"));
        assert_eq!(media_type("src/main.rs"), None);
    }

    #[test]
    fn a_null_byte_marks_a_file_as_binary() {
        assert!(is_binary(&[0x89, b'P', 0, b'N']));
        assert!(!is_binary(b"plain text"));
    }

    #[test]
    fn the_return_address_must_stay_on_this_server() {
        assert_eq!(safe_target(Some("/docs/README.md")), "/docs/README.md");
        assert_eq!(safe_target(Some("//evil.example/x")), "/");
        assert_eq!(safe_target(Some("https://evil.example/")), "/");
        assert_eq!(safe_target(None), "/");
    }

    #[test]
    fn the_theme_cookie_is_read_by_name() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("other=1; reposerve_theme=dark"),
        );
        assert_eq!(theme_from(&headers), Theme::Dark);
        assert_eq!(Theme::from_value("LIGHT"), Theme::Light);
        assert_eq!(Theme::from_value("purple"), Theme::Auto);
    }

    #[test]
    fn the_filter_cookie_is_read_by_name() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("reposerve_ext=md,rs; other=1"),
        );
        let default = Extensions::parse("md");
        assert_eq!(extensions_from(&headers, &default).value(), "md,rs");

        let none = HeaderMap::new();
        assert_eq!(extensions_from(&none, &default), default);
        assert_eq!(
            extensions_from(&none, &Extensions::parse("txt")).value(),
            "txt",
            "the configured default applies when there is no cookie"
        );
    }

    #[test]
    fn the_diagram_script_compresses_to_a_browser_sized_payload() {
        assert!(MERMAID_JS.len() > 1_000_000);
        let compressed = mermaid_gzip();
        assert!(compressed.len() < MERMAID_JS.len() / 2);
        assert_eq!(&compressed[..2], &[0x1f, 0x8b]);
    }
}
