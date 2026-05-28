#[cfg(all(target_arch = "wasm32", feature = "bevygap"))]
fn main() {
    let matchmaker_url = browser_matchmaker_url();
    let mut app = client::app(client::Cli::web_defaults(matchmaker_url));
    app.run();
}

#[cfg(all(target_arch = "wasm32", not(feature = "bevygap")))]
fn main() {
    panic!("lightrider-web must be built with the `bevygap` feature");
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("lightrider-web is intended for wasm32-unknown-unknown builds");
}

#[cfg(all(target_arch = "wasm32", feature = "bevygap"))]
fn browser_matchmaker_url() -> String {
    let window = web_sys::window().expect("browser window is unavailable");
    let location = window.location();
    let search = location.search().unwrap_or_default();
    let params =
        web_sys::UrlSearchParams::new_with_str(&search).expect("failed to parse query parameters");
    if let Some(url) = params.get("matchmaker_url") {
        if !url.trim().is_empty() {
            return url;
        }
    }

    let protocol = match location.protocol().as_deref() {
        Ok("https:") => "wss",
        _ => "ws",
    };
    let host = location
        .host()
        .expect("browser location host is unavailable");
    format!("{protocol}://{host}/matchmaker/ws")
}
