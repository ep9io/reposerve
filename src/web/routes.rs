//! The HTTP surface: one route per kind of page, the assets, the theme and
//! reading-mode controls, and the optional token gate.

use std::convert::Infallible;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Form, Path, Query, Request, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::Router;
use percent_encoding::percent_decode_str;
use serde::Deserialize;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::{Stream, StreamExt};
use tower_http::compression::CompressionLayer;

use crate::config::{MAX_READING_WIDTH, MIN_READING_WIDTH};
use crate::site::highlight;
use crate::site::html::{href, KIB};
use crate::site::index::{normalise_request_path, Extensions, Listing};
use crate::site::media;
use crate::site::notebook;
use crate::site::render;
use crate::ui::shell::View;
use crate::ui::{self, views::Rendered};
use crate::web::assets;
use crate::web::cache;
use crate::web::server::AppState;
use crate::web::theme::Theme;

/// The name of the cookie a token-carrying first visit leaves behind.
const TOKEN_COOKIE: &str = "reposerve_token";
/// The name of the cookie that carries the light or dark choice.
const THEME_COOKIE: &str = "reposerve_theme";
/// The name of the cookie that carries the file-type filter.
const FILTER_COOKIE: &str = "reposerve_ext";
/// The name of the cookie that carries the reading mode.
const READING_COOKIE: &str = "reposerve_reading";
/// The name of the cookie that carries the reading width.
const WIDTH_COOKIE: &str = "reposerve_width";

/// A year, in seconds. The longest lifetime a browser accepts.
const ONE_YEAR: &str = "31536000";
/// Thirty days, in seconds.
const THIRTY_DAYS: &str = "2592000";

/// Largest file the source view renders. A larger file is reported by size.
const SOURCE_LIMIT: usize = 512 * KIB;

/// Every page is built from this policy, which allows the page's own scripts
/// and fonts and nothing else.
const POLICY: &str = "default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; \
img-src 'self' data:; font-src 'self'; connect-src 'self'; base-uri 'none'; \
form-action 'self'; frame-ancestors 'none'";

/// One answer from a handler, before it becomes an HTTP response.
enum Body {
    /// A complete HTML page.
    Html(String),
    /// Raw bytes with their content type.
    Bytes(&'static str, Vec<u8>),
    /// A move to another address, used when a directory chain collapses.
    Redirect(String),
}

/// The reader's choices, read from the request cookies.
#[derive(Clone, Debug)]
struct Choices {
    theme: Theme,
    extensions: Extensions,
    reading: bool,
    width: u16,
}

impl Choices {
    fn from_headers(headers: &HeaderMap, config: &crate::config::Config) -> Self {
        Self {
            theme: cookie(headers, THEME_COOKIE).map_or(config.theme, Theme::from_value),
            extensions: cookie(headers, FILTER_COOKIE)
                .map_or_else(|| config.extensions.clone(), Extensions::parse),
            reading: cookie(headers, READING_COOKIE).is_some_and(|value| value == "on"),
            width: cookie(headers, WIDTH_COOKIE).map_or(config.reading_width, |value| {
                width(value, config.reading_width)
            }),
        }
    }

    fn view<'a>(&self, config: &'a crate::config::Config) -> View<'a> {
        View {
            theme: self.theme,
            extensions: self.extensions.clone(),
            reading: self.reading,
            width: self.width,
            config,
        }
    }

    fn key(&self) -> String {
        format!(
            "{}|{}|{}|{}",
            self.theme.value(),
            self.extensions.value(),
            self.reading,
            self.width
        )
    }
}

/// Builds the router.
///
/// The token gate wraps every route, and the security headers wrap the gate, so
/// a refusal is a complete response as well.
pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(home))
        .route("/search", get(search))
        .route("/__reload", get(reload))
        .route("/__index.json", get(index_json))
        .route("/__token", post(submit_token))
        .route("/filter", get(filter_moved))
        .route("/filter/apply", get(apply_filter))
        .route("/theme/{mode}", get(switch_theme))
        .route("/reading/{state}", get(switch_reading))
        .route("/width/{rem}", get(switch_width))
        .route("/highlight.css", get(highlight_css))
        .route("/manifest.webmanifest", get(manifest))
        .route("/assets/{hash}/{*name}", get(asset))
        .route("/favicon.ico", get(favicon))
        .route("/{*path}", get(entry))
        .layer(middleware::from_fn_with_state(
            Arc::clone(&state),
            require_token,
        ))
        .layer(middleware::from_fn(security_headers))
        .layer(CompressionLayer::new())
        .with_state(state)
}

/// Serves the landing page: the root README when it has one, else the root list.
async fn home(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    let choices = Choices::from_headers(&headers, &state.config);
    let key = format!("home|{}", choices.key());
    run(state, Some(key), headers, move |state| {
        let view = choices.view(&state.config);
        build_home(&state, &view)
    })
    .await
}

fn build_home(state: &AppState, view: &View<'_>) -> Result<(StatusCode, Body), crate::Error> {
    let index = state.index();
    let listing = index.listing(&view.extensions);
    if listing.is_empty() {
        return Ok((StatusCode::OK, Body::Html(empty_page(&listing, view))));
    }
    if let Some(path) = listing.front_page("") {
        return document_answer(&listing, &path, view);
    }
    let html = ui::views::directory(
        &context(&listing, view, "", &state.config.title),
        listing.tree(),
    );
    Ok((StatusCode::OK, Body::Html(html)))
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
    let choices = Choices::from_headers(&headers, &state.config);
    let key = format!(
        "search|{}|{}",
        query.q.clone().unwrap_or_default(),
        choices.key()
    );
    run(state, Some(key), headers, move |state| {
        let view = choices.view(&state.config);
        let index = state.index();
        let listing = index.listing(&view.extensions);
        let needle = query.q.unwrap_or_default();
        let matches = listing.search(&needle);
        let title = format!("Results for {needle}");
        let html = ui::views::search(&context(&listing, &view, "", &title), &needle, &matches);
        Ok((StatusCode::OK, Body::Html(html)))
    })
    .await
}

/// Streams a message whenever a served file changes.
///
/// The payload names the changed paths, so a page can ignore a change elsewhere
/// and keep the reader's place.
async fn reload(
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = BroadcastStream::new(state.reload.subscribe())
        .filter_map(|message| message.ok().map(|text| Ok(Event::default().data(text))));
    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Serves the path list the quick-open palette filters.
async fn index_json(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    let choices = Choices::from_headers(&headers, &state.config);
    let key = format!("index|{}", choices.key());
    run(state, Some(key), headers, move |state| {
        let index = state.index();
        let listing = index.listing(&choices.extensions);
        let paths: Vec<&str> = listing.paths().collect();
        let body = serde_json::to_string(&paths).unwrap_or_else(|_| "[]".to_string());
        Ok((
            StatusCode::OK,
            Body::Bytes("application/json; charset=utf-8", body.into_bytes()),
        ))
    })
    .await
}

/// Serves one embedded asset.
async fn asset(Path((_hash, name)): Path<(String, String)>) -> Response {
    let Some(asset) = assets::find(&name) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let mut response = (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, assets::content_type(&name)),
            (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
        ],
        Bytes::from_static(asset.bytes),
    )
        .into_response();
    response.headers_mut().insert(
        header::ETAG,
        HeaderValue::from_str(&format!("\"{}\"", assets::hash_of(asset.bytes)))
            .unwrap_or_else(|_| HeaderValue::from_static("\"asset\"")),
    );
    response
}

/// Redirects the browser's automatic favicon request to the real icon.
async fn favicon() -> Response {
    Redirect::permanent(&assets::url(assets::ICON)).into_response()
}

/// Serves the generated syntax palette.
async fn highlight_css() -> Response {
    let css = highlight::global().stylesheet();
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/css; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
        ],
        css,
    )
        .into_response()
}

/// Serves the web app manifest, so a tablet can pin the site.
async fn manifest(State(state): State<Arc<AppState>>) -> Response {
    let body = serde_json::json!({
        "name": state.config.title,
        "short_name": state.config.title,
        "start_url": "/",
        "display": "standalone",
        "background_color": "#ffffff",
        "theme_color": "#0f1115",
        "icons": [{
            "src": assets::url(assets::ICON),
            "sizes": "any",
            "type": "image/svg+xml",
            "purpose": "any",
        }],
    });
    let mut response = (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/manifest+json")],
        body.to_string(),
    )
        .into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    response
}

#[derive(Debug, Deserialize)]
struct NextQuery {
    next: Option<String>,
}

/// Stores the chosen theme and returns the reader to the page they were on.
async fn switch_theme(Path(mode): Path<String>, Query(query): Query<NextQuery>) -> Response {
    let theme = Theme::from_value(&mode);
    redirect_with_cookie(
        &safe_target(query.next.as_deref()),
        THEME_COOKIE,
        theme.value(),
        ONE_YEAR,
    )
}

/// Stores the reading mode and returns the reader to the page they were on.
async fn switch_reading(Path(state): Path<String>, Query(query): Query<NextQuery>) -> Response {
    let value = if state == "on" { "on" } else { "off" };
    redirect_with_cookie(
        &safe_target(query.next.as_deref()),
        READING_COOKIE,
        value,
        ONE_YEAR,
    )
}

/// Stores the chosen reading width and returns the reader to the page they were on.
async fn switch_width(
    State(state): State<Arc<AppState>>,
    Path(rem): Path<String>,
    Query(query): Query<NextQuery>,
) -> Response {
    let width = width(&rem, state.config.reading_width);
    redirect_with_cookie(
        &safe_target(query.next.as_deref()),
        WIDTH_COOKIE,
        &width.to_string(),
        ONE_YEAR,
    )
}

/// A width from a cookie or a link, when it is inside the allowed range.
fn width(value: &str, fallback: u16) -> u16 {
    value
        .trim()
        .parse::<u16>()
        .ok()
        .filter(|rem| (MIN_READING_WIDTH..=MAX_READING_WIDTH).contains(rem))
        .unwrap_or(fallback)
}

/// The filter form lives in the sidebar now. An old link lands on the root.
async fn filter_moved() -> Response {
    Redirect::permanent("/").into_response()
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
    let target = safe_target(Some(&next));
    // A reader who hides the type of the page they are reading goes to the root
    // instead of a dead end.
    let target = match extension_of(&target) {
        Some(extension) if !extensions.contains(extension) => "/".to_string(),
        _ => target,
    };
    redirect_with_cookie(&target, FILTER_COOKIE, &extensions.value(), ONE_YEAR)
}

/// The file type of a request path, from its last segment.
fn extension_of(path: &str) -> Option<&str> {
    let name = path.rsplit('/').next()?;
    let (stem, extension) = name.rsplit_once('.')?;
    (!stem.is_empty() && !extension.is_empty()).then_some(extension)
}

#[derive(Debug, Deserialize)]
struct TokenForm {
    token: Option<String>,
}

/// Accepts the token from a form, so it never appears in the address bar.
async fn submit_token(State(state): State<Arc<AppState>>, Form(form): Form<TokenForm>) -> Response {
    let Some(expected) = state.config.token.as_deref() else {
        return Redirect::to("/").into_response();
    };
    match form.token {
        Some(supplied) if tokens_match(&supplied, expected) => {
            redirect_with_cookie("/", TOKEN_COOKIE, expected, THIRTY_DAYS)
        }
        _ => Redirect::to("/?locked=1").into_response(),
    }
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

fn redirect_with_cookie(target: &str, name: &str, value: &str, max_age: &str) -> Response {
    let mut response = Redirect::to(target).into_response();
    if let Ok(cookie) = HeaderValue::from_str(&format!(
        "{name}={value}; Path=/; SameSite=Lax; Max-Age={max_age}"
    )) {
        response.headers_mut().insert(header::SET_COOKIE, cookie);
    }
    response
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
    let choices = Choices::from_headers(&headers, &state.config);
    let wants_raw = query.raw.as_deref().is_some_and(|value| value != "0");
    let key = format!("entry|{raw_path}|{wants_raw}|{}", choices.key());
    run(state, Some(key), headers, move |state| {
        let view = choices.view(&state.config);
        build_entry(&state, &view, &raw_path, wants_raw)
    })
    .await
}

fn build_entry(
    state: &AppState,
    view: &View<'_>,
    raw_path: &str,
    wants_raw: bool,
) -> Result<(StatusCode, Body), crate::Error> {
    let index = state.index();
    let listing = index.listing(&view.extensions);
    let Some(path) = normalise_request_path(raw_path) else {
        return Ok(not_found(&listing, view, raw_path));
    };
    if wants_raw {
        return raw_answer(&listing, &path);
    }
    if listing.has_file(&path) {
        return file_answer(&listing, &path, view);
    }
    if listing.has_directory(&path) {
        // A chain of directories that holds one branch is one entry, so a
        // reader who asks for its top lands on its deepest directory.
        if !path.is_empty() {
            if let Some(target) = listing.collapsed_directory(&path) {
                if target != path {
                    return Ok((
                        StatusCode::OK,
                        Body::Redirect(format!("/{}", href(&target))),
                    ));
                }
            }
        }
        if let Some(front) = listing.front_page(&path) {
            return document_answer(&listing, &front, view);
        }
        let title = directory_title(&path);
        let html = ui::views::directory(
            &context(&listing, view, &path, &title),
            find(listing.tree(), &path).unwrap_or_else(|| listing.tree()),
        );
        return Ok((StatusCode::OK, Body::Html(html)));
    }
    Ok(not_found(&listing, view, &path))
}

fn not_found(listing: &Listing<'_>, view: &View<'_>, path: &str) -> (StatusCode, Body) {
    let mut context = context(listing, view, path, "Not found");
    context.actions = String::new();
    (
        StatusCode::NOT_FOUND,
        Body::Html(ui::views::not_found(&context, path)),
    )
}

/// The bytes of one indexed file, unrendered.
fn raw_answer(listing: &Listing<'_>, path: &str) -> Result<(StatusCode, Body), crate::Error> {
    let bytes = listing.read(path)?;
    let content_type = media::content_type(path).unwrap_or("text/plain; charset=utf-8");
    Ok((StatusCode::OK, Body::Bytes(content_type, bytes)))
}

/// One indexed file, rendered according to what it is.
fn file_answer(
    listing: &Listing<'_>,
    path: &str,
    view: &View<'_>,
) -> Result<(StatusCode, Body), crate::Error> {
    if media::is_notebook(path) {
        let bytes = listing.read(path)?;
        let source = String::from_utf8_lossy(&bytes);
        let options = render::Options {
            math: view.config.math,
        };
        return match notebook::render(&source, options) {
            Ok(rendered) => Ok((
                StatusCode::OK,
                Body::Html(ui::views::notebook(
                    &context(listing, view, path, &file_title(path)),
                    &rendered,
                )),
            )),
            // Not valid notebook JSON: show it as source rather than failing.
            Err(_) => source_answer(listing, path, view),
        };
    }
    if media::is_markdown(path) {
        return document_answer(listing, path, view);
    }
    if let Some(content_type) = media::content_type(path) {
        let bytes = listing.read(path)?;
        return Ok((StatusCode::OK, Body::Bytes(content_type, bytes)));
    }
    source_answer(listing, path, view)
}

/// A Markdown document, rendered.
fn document_answer(
    listing: &Listing<'_>,
    path: &str,
    view: &View<'_>,
) -> Result<(StatusCode, Body), crate::Error> {
    let bytes = listing.read(path)?;
    let source = String::from_utf8_lossy(&bytes);
    let document = render::markdown(
        &source,
        render::Options {
            math: view.config.math,
        },
    );
    let title = document.title.clone().unwrap_or_else(|| file_title(path));
    let context = Rendered {
        listing,
        view: view.clone(),
        current: path,
        title,
        actions: ui::shell::source_action(path),
    };
    Ok((
        StatusCode::OK,
        Body::Html(ui::views::document(&context, &document)),
    ))
}

/// A file shown as numbered text, highlighted when its type is known.
fn source_answer(
    listing: &Listing<'_>,
    path: &str,
    view: &View<'_>,
) -> Result<(StatusCode, Body), crate::Error> {
    let bytes = listing.read(path)?;
    let size = bytes.len();
    if size > SOURCE_LIMIT || is_binary(&bytes) {
        return Ok((
            StatusCode::OK,
            Body::Html(ui::views::binary(
                &context(listing, view, path, &file_title(path)),
                size,
            )),
        ));
    }
    let source = String::from_utf8_lossy(&bytes);
    let highlighted = highlight::global().for_path(path, &source);
    Ok((
        StatusCode::OK,
        Body::Html(ui::views::source(
            &context(listing, view, path, &file_title(path)),
            &source,
            highlighted.as_deref(),
            size,
        )),
    ))
}

/// The parts every view shares.
fn context<'a>(
    listing: &'a Listing<'a>,
    view: &View<'a>,
    current: &'a str,
    title: &str,
) -> Rendered<'a> {
    Rendered {
        listing,
        view: view.clone(),
        current,
        title: title.to_string(),
        actions: if current.is_empty() {
            String::new()
        } else {
            ui::shell::source_action(current)
        },
    }
}

fn empty_page(listing: &Listing<'_>, view: &View<'_>) -> String {
    ui::views::empty_filter(&context(listing, view, "", "No file type is shown"))
}

fn directory_title(path: &str) -> String {
    match path.rsplit('/').next() {
        Some(name) if !name.is_empty() => name.to_string(),
        _ => path.to_string(),
    }
}

fn file_title(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

fn find<'a>(
    node: &'a crate::site::index::TreeNode,
    directory: &str,
) -> Option<&'a crate::site::index::TreeNode> {
    if node.path == directory {
        return Some(node);
    }
    node.dirs.iter().find_map(|child| find(child, directory))
}

/// A NUL byte in the first block means the file is not text.
fn is_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8192).any(|byte| *byte == 0)
}

/// Runs a blocking page build off the async worker.
///
/// The answer carries an entity tag, so a repeat visit costs a `304` and no
/// body. A redirect carries no tag, because it must always be followed.
async fn run<F>(state: Arc<AppState>, key: Option<String>, headers: HeaderMap, work: F) -> Response
where
    F: FnOnce(Arc<AppState>) -> Result<(StatusCode, Body), crate::Error> + Send + 'static,
{
    let if_none_match = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);

    if let Some(key) = key.as_deref() {
        if let Some(page) = state.pages.get(key) {
            return answer(
                StatusCode::OK,
                Body::Html(page.html.clone()),
                if_none_match.as_deref(),
            );
        }
    }

    let worker = Arc::clone(&state);
    let built = tokio::task::spawn_blocking(move || work(worker)).await;
    let (status, body) = match built {
        Ok(Ok(answer)) => answer,
        Ok(Err(error)) => {
            eprintln!("error: {error}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
                format!("{error}\n"),
            )
                .into_response();
        }
        Err(error) => {
            eprintln!("error: the page build panicked: {error}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response();
        }
    };

    if let (Some(key), Body::Html(html)) = (key, &body) {
        state.pages.insert(key, html.clone());
    }
    answer(status, body, if_none_match.as_deref())
}

fn answer(status: StatusCode, body: Body, if_none_match: Option<&str>) -> Response {
    match body {
        Body::Html(html) => tagged(
            status,
            "text/html; charset=utf-8",
            html.into_bytes(),
            if_none_match,
        ),
        Body::Bytes(content_type, bytes) => tagged(status, content_type, bytes, if_none_match),
        Body::Redirect(target) => Redirect::to(&target).into_response(),
    }
}

fn tagged(
    status: StatusCode,
    content_type: &'static str,
    bytes: Vec<u8>,
    if_none_match: Option<&str>,
) -> Response {
    let etag = cache::etag_bytes(&bytes);
    if if_none_match.is_some_and(|value| value.split(',').any(|item| item.trim() == etag)) {
        return (
            StatusCode::NOT_MODIFIED,
            [
                (header::ETAG, etag),
                (header::CACHE_CONTROL, "private, no-cache".to_string()),
            ],
        )
            .into_response();
    }
    let mut response = (status, [(header::CONTENT_TYPE, content_type)], bytes).into_response();
    let headers = response.headers_mut();
    if let Ok(value) = HeaderValue::from_str(&etag) {
        headers.insert(header::ETAG, value);
    }
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-cache"),
    );
    response
}

/// Refuses a request that carries neither the token cookie nor a matching token.
async fn require_token(
    State(state): State<Arc<AppState>>,
    request: Request,
    next: Next,
) -> Response {
    let Some(expected) = state.config.token.as_deref() else {
        return next.run(request).await;
    };
    if request.uri().path() == "/__token" {
        return next.run(request).await;
    }

    let query = request.uri().query().unwrap_or_default();
    let pairs: Vec<&str> = query.split('&').filter(|pair| !pair.is_empty()).collect();
    let supplied = pairs
        .iter()
        .find_map(|pair| pair.strip_prefix("token="))
        .map(|value| percent_decode_str(value).decode_utf8_lossy().into_owned());

    if let Some(supplied) = supplied {
        if tokens_match(&supplied, expected) {
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
            return redirect_with_cookie(&target, TOKEN_COOKIE, expected, THIRTY_DAYS);
        }
    }

    if cookie(request.headers(), TOKEN_COOKIE).is_some_and(|value| tokens_match(value, expected)) {
        return next.run(request).await;
    }

    let index = state.index();
    let listing = index.listing(&state.config.extensions);
    let view = View {
        theme: Choices::from_headers(request.headers(), &state.config).theme,
        extensions: state.config.extensions.clone(),
        reading: false,
        width: state.config.reading_width,
        config: &state.config,
    };
    let html = ui::views::token(&context(&listing, &view, "", "Token required"));
    (
        StatusCode::UNAUTHORIZED,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        html,
    )
        .into_response()
}

/// Compares two tokens without leaking their length through timing.
fn tokens_match(supplied: &str, expected: &str) -> bool {
    let supplied = supplied.as_bytes();
    let expected = expected.as_bytes();
    if supplied.len() != expected.len() {
        return false;
    }
    let mut difference = 0u8;
    for (left, right) in supplied.iter().zip(expected) {
        difference |= left ^ right;
    }
    difference == 0
}

/// Adds the headers that keep a page from reaching anything but this server.
async fn security_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(POLICY),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        header::HeaderName::from_static("cross-origin-opener-policy"),
        HeaderValue::from_static("same-origin"),
    );
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
    fn a_width_must_be_inside_the_allowed_range() {
        assert_eq!(width("56", 46), 56);
        assert_eq!(width(" 68 ", 46), 68);
        assert_eq!(width("12", 46), 46);
        assert_eq!(width("91", 46), 46);
        assert_eq!(width("wide", 46), 46);
    }

    #[test]
    fn the_return_address_must_stay_on_this_server() {
        assert_eq!(safe_target(Some("/docs/README.md")), "/docs/README.md");
        assert_eq!(safe_target(Some("//evil.example/x")), "/");
        assert_eq!(safe_target(Some("https://evil.example/")), "/");
        assert_eq!(safe_target(None), "/");
    }

    #[test]
    fn a_null_byte_marks_a_file_as_binary() {
        assert!(is_binary(&[0x89, b'P', 0, b'N']));
        assert!(!is_binary(b"plain text"));
    }

    fn config() -> crate::config::Config {
        crate::config::Config::default()
    }

    #[test]
    fn the_theme_cookie_is_read_by_name() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("other=1; reposerve_theme=dark"),
        );
        let choices = Choices::from_headers(&headers, &config());
        assert_eq!(choices.theme, Theme::Dark);
        assert_eq!(Theme::from_value("LIGHT"), Theme::Light);
        assert_eq!(Theme::from_value("purple"), Theme::Auto);
    }

    #[test]
    fn a_first_visit_uses_the_configured_theme() {
        let headers = HeaderMap::new();
        let configured = crate::config::Config {
            theme: Theme::Dark,
            ..config()
        };
        assert_eq!(
            Choices::from_headers(&headers, &configured).theme,
            Theme::Dark
        );
        assert_eq!(
            Choices::from_headers(&headers, &config()).theme,
            Theme::Auto,
            "the default is the system"
        );
    }

    #[test]
    fn the_filter_cookie_is_read_by_name() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("reposerve_ext=md,rs; other=1"),
        );
        let default = Extensions::parse("md");
        let choices = Choices::from_headers(&headers, &config());
        assert_eq!(choices.extensions.value(), "md,rs");

        let none = HeaderMap::new();
        assert_eq!(Choices::from_headers(&none, &config()).extensions, default);
    }

    #[test]
    fn the_reading_cookie_is_read_by_name() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("reposerve_reading=on"),
        );
        assert!(Choices::from_headers(&headers, &config()).reading);
    }

    #[test]
    fn tokens_are_compared_by_value_and_length() {
        assert!(tokens_match("secret", "secret"));
        assert!(!tokens_match("secrey", "secret"));
        assert!(!tokens_match("secret ", "secret"));
        assert!(!tokens_match("", "secret"));
    }

    #[test]
    fn the_policy_forbids_a_remote_script() {
        assert!(POLICY.contains("script-src 'self'"));
        assert!(!POLICY.contains("script-src 'self' 'unsafe-inline'"));
        assert!(POLICY.contains("frame-ancestors 'none'"));
    }
}
