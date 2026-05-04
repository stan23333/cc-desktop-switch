use include_dir::{Dir, include_dir};
use tauri::http::{Response, StatusCode, header};

static FRONTEND: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../frontend");
static INDEX_HTML: &str = include_str!("../../frontend/index.html");

pub fn serve(path: &str) -> Response<Vec<u8>> {
    let trimmed = path.trim_start_matches('/');
    let lookup_path = if trimmed.is_empty() {
        "index.html"
    } else {
        trimmed
    };

    if lookup_path == "index.html" {
        return text_response("text/html; charset=utf-8", INDEX_HTML.as_bytes().to_vec());
    }

    if let Some(file) = FRONTEND.get_file(lookup_path) {
        return bytes_response(lookup_path, file.contents().to_vec());
    }

    if !lookup_path.starts_with("api/") {
        return serve("index.html");
    }

    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(format!("404: {path}").into_bytes())
        .expect("static 404 response")
}

fn bytes_response(path: &str, bytes: Vec<u8>) -> Response<Vec<u8>> {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    text_response(mime.essence_str(), bytes)
}

fn text_response(content_type: &str, bytes: Vec<u8>) -> Response<Vec<u8>> {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "no-cache")
        .body(bytes)
        .expect("static frontend response")
}
