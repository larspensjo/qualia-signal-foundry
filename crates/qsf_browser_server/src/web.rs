use axum::{Router, response::Html, routing::get};

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(root))
}

async fn root() -> Html<&'static str> {
    Html(
        r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8">
    <title>QSF Memory Browser API</title>
    <style>
      body { background: #050812; color: #eaf6ff; font-family: system-ui, sans-serif; margin: 0; padding: 32px; }
      a { color: #7de3ff; }
      code { background: #121a2a; border-radius: 4px; padding: 2px 5px; }
    </style>
  </head>
  <body>
    <h1>QSF Memory Browser API</h1>
    <p>The Rust backend is running on this port.</p>
    <p>Open the Vite workbench UI at <a href="http://localhost:5173/">http://localhost:5173/</a>.</p>
    <p>API health is available at <a href="/api/health"><code>/api/health</code></a>.</p>
  </body>
</html>"#,
    )
}
