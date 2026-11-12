use std::convert::Infallible;
use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::http::response::Builder;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router, ServiceExt, extract, middleware};
use chrono::Utc;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use tokio::net::TcpListener;
use tower::{Layer, Service};
use tower_http::services::ServeDir;
use tracing::{debug, info, warn};
use url::Url;

use crate::client_ip::add_client_ip;
use crate::normalize_path::normalize_path;
use crate::request_logging::log_requests;
use crate::{AppError, AppState};

trait RouterExt {
  fn serve_dir_with_fallback(self, path: &str, local_root: &Path, fallback_url: Option<Url>) -> Self;
}

impl<S> RouterExt for Router<S>
where
  S: Clone + Send + Sync + 'static,
{
  fn serve_dir_with_fallback(self, path: &str, local_root: &Path, fallback_url: Option<Url>) -> Self {
    // Serve from the local resources folder (not the URL path), falling back to the upstream mirror.
    let serve_dir = ServeDir::new(local_root);
    if let Some(url) = fallback_url {
      self.nest_service(path, serve_dir.fallback(ServeRemoteResource::new(url)))
    } else {
      self.nest_service(path, serve_dir)
    }
  }
}

pub async fn start(state: Arc<AppState>) -> io::Result<()> {
  let settings = &state.settings.static_server;
  let app = Router::new()
    .route("/", get(get_root_friendly))
    .route("/public.pem", get(get_public_key))
    .route("/versions/{version}", get(get_version))
    .route("/webview/EN/data/json/news.json", get(get_news))
    .route("/news/{file}", get(get_news_detail))
    .serve_dir_with_fallback(
      "/bundles",
      &settings.resources_root.join("bundles/"),
      settings.upstream_url.as_ref().map(|url| url.join("bundles/").unwrap()),
    )
    .serve_dir_with_fallback(
      "/banners",
      &settings.resources_root.join("banners/"),
      settings.upstream_url.as_ref().map(|url| url.join("banners/").unwrap()),
    )
    .serve_dir_with_fallback(
      "/webview",
      &settings.resources_root.join("webview/"),
      settings.upstream_url.as_ref().map(|url| url.join("webview/").unwrap()),
    )
    .layer(log_requests())
    .layer(middleware::from_fn(add_client_ip))
    .with_state(state.clone());
  let middleware = tower::util::MapRequestLayer::new(normalize_path);
  let app = middleware.layer(app);

  let listener = TcpListener::bind(settings.bind_address).await?;
  info!(
    "static server started at {:?} -> {}",
    listener.local_addr()?,
    settings.public_url
  );
  axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await
}

#[derive(Debug, Clone)]
struct ServeRemoteResource {
  remote_url: Url,
}

impl ServeRemoteResource {
  pub fn new(remote_url: Url) -> Self {
    Self { remote_url }
  }
}

impl Service<Request> for ServeRemoteResource {
  type Response = Response;
  type Error = Infallible;
  type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + Sync + 'static>>;

  fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
    Poll::Ready(Ok(()))
  }

  fn call(&mut self, request: Request) -> Self::Future {
    let path = request.uri().path()[1..].to_owned();
    let remote_url = self.remote_url.join(&path).unwrap();
    debug!("get bundle remote: {} -> {}", path, remote_url);

    Box::pin(async move {
      let client = Client::new();
      let response = client.get(remote_url).send().await.unwrap();

      let status = StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
      if !status.is_success() {
        warn!("failed to fetch remote resource: {}, status: {}", path, status);
      }

      let headers = response.headers().clone();
      let stream = response.bytes_stream();
      let body = Body::from_stream(stream);

      let mut builder = Builder::new().status(status);
      for (name, value) in headers {
        let name = match name {
          Some(name) => name,
          None => continue,
        };

        builder = builder.header(name, value);
      }

      Ok(builder.body(body).unwrap_or_else(|_| {
        Response::builder()
          .status(StatusCode::INTERNAL_SERVER_ERROR)
          .body(Body::empty())
          .unwrap()
      }))
    })
  }
}

#[derive(Serialize, Deserialize)]
pub struct NewsItem {
  pub title: String,
  pub category: NewsCategory,
  pub platform: NewsPlatform,
  pub thumbnail: Option<String>,
  pub banner: Option<String>,
  pub url: String,
  pub priority: Option<i64>,
  pub start_at: i64,
  pub end_at: i64,
  pub date: String,
}

#[derive(Debug, Clone, Copy, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum NewsCategory {
  AnnouncementReportError = 1,
  Event = 2,
  EventRecruit = 3,
  EventCampaign = 4,
  AnnouncementNotice = 5,
  Update = 6,
  UpdateGameMaintenance = 7,
}

impl NewsCategory {
  pub fn from_i32(value: i32) -> Self {
    match value {
      1 => NewsCategory::AnnouncementReportError,
      2 => NewsCategory::Event,
      3 => NewsCategory::EventRecruit,
      4 => NewsCategory::EventCampaign,
      6 => NewsCategory::Update,
      7 => NewsCategory::UpdateGameMaintenance,
      _ => NewsCategory::AnnouncementNotice,
    }
  }
}

#[derive(Debug, Clone, Copy, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum NewsPlatform {
  All = 0,
  Apple = 1,
  Android = 2,
  Pc = 3,
}

/// Announcements for "Note", from the `news` table (see migrations/20261112000002_add_news.sql).
async fn get_news(State(state): State<Arc<AppState>>) -> axum::response::Result<impl IntoResponse, AppError> {
  let news = match fetch_news(&state).await {
    Ok(news) => news,
    Err(error) => {
      // Most likely the news table does not exist yet: show an empty list instead of breaking the page
      warn!(?error, "failed to load news");
      Vec::new()
    }
  };
  info!(count = news.len(), "get news");

  Ok(Json(news))
}

async fn fetch_news(state: &AppState) -> anyhow::Result<Vec<NewsItem>> {
  let client = state.get_database_client().await?;
  #[rustfmt::skip]
  let rows = client.query(/* language=postgresql */ r#"
    select id, title, category, banner, priority, start_at, end_at
    from news
    where start_at <= now()
      and (end_at is null or end_at > now())
    order by priority desc, start_at desc
  "#, &[]).await?;

  let base = format_url(&state.settings.static_server.public_url, Some("news/")).map_err(anyhow::Error::msg)?;
  let news = rows
    .iter()
    .map(|row| {
      let id: i64 = row.get("id");
      let category: i32 = row.get("category");
      let start_at: chrono::DateTime<Utc> = row.get("start_at");
      let end_at: Option<chrono::DateTime<Utc>> = row.get("end_at");
      NewsItem {
        title: row.get("title"),
        category: NewsCategory::from_i32(category),
        platform: NewsPlatform::All,
        thumbnail: None,
        banner: row.get("banner"),
        url: format!("https://{base}{id}.html"),
        priority: Some(row.get::<_, i32>("priority") as i64),
        start_at: start_at.timestamp(),
        end_at: end_at
          .unwrap_or_else(|| Utc::now() + chrono::Duration::days(365))
          .timestamp(),
        date: start_at.format("%Y-%m-%d").to_string(),
      }
    })
    .collect();
  Ok(news)
}

/// Page opened when tapping an announcement in "Note".
async fn get_news_detail(
  State(state): State<Arc<AppState>>,
  extract::Path(file): extract::Path<String>,
) -> axum::response::Result<impl IntoResponse, AppError> {
  let not_found = || (StatusCode::NOT_FOUND, Html("<h1>Not found</h1>".to_owned())).into_response();
  let Some(id) = file.strip_suffix(".html").unwrap_or(&file).parse::<i64>().ok() else {
    return Ok(not_found());
  };

  let client = state.get_database_client().await?;
  #[rustfmt::skip]
  let row = client.query_opt(/* language=postgresql */ r#"
    select title, banner, body, start_at
    from news
    where id = $1
  "#, &[&id]).await?;
  let Some(row) = row else {
    return Ok(not_found());
  };

  let title: String = row.get("title");
  let banner: Option<String> = row.get("banner");
  let body: String = row.get("body");
  let start_at: chrono::DateTime<Utc> = row.get("start_at");
  let banner = banner
    .map(|url| format!(r#"<img src="{}" alt="">"#, html_escape(&url)))
    .unwrap_or_default();

  Ok(
    Html(format!(
      r#"<!DOCTYPE html>
<html lang="es">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<style>
  body {{ margin: 0; padding: 16px; font-family: sans-serif; background: #fdf6e3; color: #4a3b2a; }}
  h1 {{ font-size: 1.3em; margin: 0 0 4px; }}
  .date {{ color: #8a7a66; font-size: 0.85em; margin-bottom: 12px; }}
  img {{ max-width: 100%; border-radius: 6px; margin-bottom: 12px; }}
</style>
</head>
<body>
{banner}
<h1>{title}</h1>
<div class="date">{date}</div>
{body}
</body>
</html>"#,
      title = html_escape(&title),
      date = start_at.format("%Y-%m-%d"),
    ))
    .into_response(),
  )
}

fn html_escape(text: &str) -> String {
  text
    .replace('&', "&amp;")
    .replace('<', "&lt;")
    .replace('>', "&gt;")
    .replace('"', "&quot;")
}

fn format_url(url: &Url, suffix: Option<&str>) -> Result<String, &'static str> {
  if url.scheme() != "https" {
    return Err("URL scheme must be https");
  }

  // Ensure trailing slash
  let mut base = url.clone();
  if !base.path().ends_with('/') {
    base.set_path(&format!("{}/", base.path()));
  }

  let new_url = if let Some(suffix) = suffix {
    base.join(suffix).unwrap()
  } else {
    base
  };
  let without_scheme = new_url.as_str().trim_start_matches("https://").to_string();

  Ok(without_scheme)
}

async fn get_version(
  State(state): State<Arc<AppState>>,
  extract::Path(version): extract::Path<String>,
) -> axum::response::Result<impl IntoResponse, AppError> {
  info!("get version info: {}", version);

  let name = env!("CARGO_PKG_NAME");
  let version = env!("CARGO_PKG_VERSION");

  Ok(Json(GetVersion {
    app_version: format!("4.11.5/{name}-{version}"),
    asset_version: "2025012110".to_string(),
    api_url: format_url(&state.settings.api_server.public_url, None).unwrap(),
    asset_url: format_url(&state.settings.static_server.public_url, Some("bundles/4.11.6/")).unwrap(),
    webview_url: format_url(&state.settings.static_server.public_url, Some("webview/")).unwrap(),
    banner_url: format_url(&state.settings.static_server.public_url, Some("banners/")).unwrap(),
    inquiry_url: "inquiry.sesisoft.com/".to_string(),
    enable_review: "false".to_string(),
  }))
}

#[derive(Debug, Serialize)]
pub struct GetVersion {
  pub app_version: String,
  pub asset_version: String,
  pub api_url: String,
  pub asset_url: String,
  pub webview_url: String,
  pub banner_url: String,
  pub inquiry_url: String,
  pub enable_review: String,
}

async fn get_root_friendly() -> axum::response::Result<impl IntoResponse, AppError> {
  let name = env!("CARGO_PKG_NAME");
  let version = env!("CARGO_PKG_VERSION");

  Ok(Html(format!(
    "<DOCTYPE html>
    <html>
      <head>
        <title>Axel static server</title>
      </head>
      <body>
        <h1>Welcome to the Axel static server!</h1>
        <p>This server provides initial configuration and static assets for the game.</p>
        <p>Available endpoints: <code>/versions/{{version}}</code></p>
        <hr />
        <i>{name}/{version}</i>
      </body>
    </html>",
  )))
}

async fn get_public_key() -> axum::response::Result<impl IntoResponse, AppError> {
  Ok(include_str!("../pubkey.pem"))
}
