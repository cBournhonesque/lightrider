#[cfg(all(target_arch = "wasm32", feature = "lightyear-matchmaker"))]
mod wasm {
    use client::WebClientOptions;
    use leptos::prelude::*;
    use leptos_bevy_canvas::prelude::*;
    use shared::network::protocol::prelude::{RoomCode, RoomId, RoomJoinMode};
    use wasm_bindgen::JsCast;

    const DEFAULT_GAME: &str = "lightrider";
    const DEFAULT_VERSION: &str = "dev";
    const CANVAS_ID: &str = "bevy_canvas";

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct BrowserSettings {
        matchmaker_url: String,
        matchmaker_game: String,
        matchmaker_version: String,
        player_name: String,
        room: RoomJoinMode,
    }

    pub fn run() {
        console_error_panic_hook::set_once();
        if let Some(root) = document_element_by_id("lightrider-root") {
            root.set_inner_html("");
            leptos::mount::mount_to(root.unchecked_into(), || view! { <LightriderWebApp /> })
                .forget();
        } else {
            leptos::mount::mount_to_body(|| view! { <LightriderWebApp /> });
        }
    }

    #[component]
    fn LightriderWebApp() -> impl IntoView {
        let settings = BrowserSettings::from_location();
        let initial_room_code = room_input_value(settings.room);
        let initial_room_badge = room_badge_text(settings.room);
        let show_modal = settings.player_name.trim().is_empty();
        let startup_error = browser_startup_error(&settings);
        if let Some(message) = startup_error.as_deref() {
            set_status_element(Some(message));
        }
        let can_start_bevy = startup_error.is_none();
        let game_stage = if can_start_bevy {
            let bevy_options = settings.bevy_options();
            view! {
                <BevyCanvas
                    canvas_id=CANVAS_ID
                    init=move || client::web_app(bevy_options.clone())
                />
            }
            .into_any()
        } else {
            view! { <div class="game-stage-blocked"></div> }.into_any()
        };

        let (name, set_name) = signal(settings.player_name.clone());
        let (room_code, set_room_code) = signal(initial_room_code);
        let (room_badge, set_room_badge) = signal(initial_room_badge);
        let (modal_open, set_modal_open) = signal(show_modal);
        let (error, set_error) = signal(String::new());

        let on_name_input = move |event| {
            set_name.set(event_target_value(&event));
        };
        let on_room_input = move |event| {
            set_room_code.set(event_target_value(&event).to_ascii_uppercase());
        };
        let on_submit = move |event: web_sys::SubmitEvent| {
            event.prevent_default();
            let player_name = sanitize_player_name(&name.get());
            let room = room_code.get();
            match parse_room_input(&room) {
                Ok(room_mode) => {
                    set_error.set(String::new());
                    set_room_badge.set(room_badge_text(room_mode));
                    apply_settings_to_url(&player_name, room_mode);
                    set_modal_open.set(false);
                }
                Err(message) => set_error.set(message),
            }
        };

        view! {
            <main class="lightrider-web-shell">
                <div class="game-stage">
                    {game_stage}
                </div>
                <div class=move || if modal_open.get() { "menu-backdrop" } else { "menu-backdrop hidden" }>
                    <section class="join-modal" aria-label="Lightrider menu">
                        <button
                            class="modal-close"
                            type="button"
                            aria-label="Close menu"
                            on:click=move |_| set_modal_open.set(false)
                        >
                            "X"
                        </button>
                        <h1>"LIGHTRIDER"</h1>
                        <form on:submit=on_submit>
                            <input
                                class="name-input"
                                autocomplete="nickname"
                                maxlength="18"
                                placeholder="Name"
                                prop:value=name
                                on:input=on_name_input
                            />
                            <div class="room-row">
                                <input
                                    class="room-input"
                                    autocomplete="off"
                                    maxlength="4"
                                    placeholder="ROOM"
                                    prop:value=room_code
                                    on:input=on_room_input
                                />
                                <button class="play-button" type="submit">"PLAY"</button>
                            </div>
                            <p class=move || if error.get().is_empty() { "form-error hidden" } else { "form-error" }>
                                {move || error.get()}
                            </p>
                        </form>
                        <div class="modal-links">
                            <button type="button" on:click=move |_| {
                                set_room_code.set(String::new());
                            }>"PUBLIC"</button>
                            <a href="https://github.com/cBournhonesque/lightrider" target="_blank" rel="noreferrer">"GitHub"</a>
                        </div>
                    </section>
                </div>
                <div class=move || if room_badge.get().is_some() { "room-code-badge" } else { "room-code-badge hidden" }>
                    {move || room_badge.get().unwrap_or_default()}
                </div>
                <button
                    class=move || if modal_open.get() { "menu-button hidden" } else { "menu-button" }
                    type="button"
                    aria-label="Open menu"
                    on:click=move |_| set_modal_open.set(true)
                >
                    "MENU"
                </button>
            </main>
        }
    }

    fn browser_startup_error(settings: &BrowserSettings) -> Option<String> {
        let window = web_sys::window()?;
        if !window.is_secure_context() {
            return Some(
                "WebTransport requires a secure browser context. Use https://, or open through http://localhost while testing."
                    .to_string(),
            );
        }

        if !global_exists("WebTransport") {
            return Some(
                "This browser does not expose WebTransport. Use a current Chromium-based browser."
                    .to_string(),
            );
        }

        if window_location().protocol().ok().as_deref() == Some("https:")
            && settings.matchmaker_url.starts_with("ws://")
        {
            return Some("HTTPS pages must use a wss:// matchmaker URL.".to_string());
        }

        None
    }

    fn global_exists(name: &str) -> bool {
        let Some(window) = web_sys::window() else {
            return false;
        };
        js_sys::Reflect::has(window.as_ref(), &wasm_bindgen::JsValue::from_str(name))
            .unwrap_or(false)
    }

    fn set_status_element(status: Option<&str>) {
        let Some(element) = document_element_by_id("lightrider-status") else {
            return;
        };
        match status {
            Some(status) => {
                element.set_inner_html(status);
                element.set_class_name("loading-status");
            }
            None => {
                element.set_inner_html("");
                element.set_class_name("loading-status hidden");
            }
        }
    }

    fn document_element_by_id(id: &str) -> Option<web_sys::Element> {
        web_sys::window()?.document()?.get_element_by_id(id)
    }

    fn room_badge_text(room: RoomJoinMode) -> Option<String> {
        match room {
            RoomJoinMode::Private(code) => Some(format!("ROOM {code}")),
            RoomJoinMode::Specific(id) => Some(format!("ROOM {}", id.0)),
            RoomJoinMode::New => Some("NEW ROOM".to_string()),
            RoomJoinMode::Auto => None,
        }
    }

    impl BrowserSettings {
        fn from_location() -> Self {
            let location = window_location();
            let search = location.search().unwrap_or_default();
            let params = web_sys::UrlSearchParams::new_with_str(&search)
                .expect("failed to parse query parameters");
            let room = parse_room_param(params.get("room"));
            let bootstrap = BrowserBootstrap::from_window();
            Self {
                matchmaker_url: params
                    .get("matchmaker_url")
                    .filter(|value| !value.trim().is_empty())
                    .or_else(|| {
                        params
                            .get("matchmaker")
                            .and_then(|route| routed_matchmaker_url(&route))
                    })
                    .or_else(|| bootstrap.matchmaker_url.clone())
                    .unwrap_or_else(default_matchmaker_url),
                matchmaker_game: params
                    .get("matchmaker_game")
                    .filter(|value| !value.trim().is_empty())
                    .or_else(|| bootstrap.matchmaker_game.clone())
                    .unwrap_or_else(|| DEFAULT_GAME.to_string()),
                matchmaker_version: params
                    .get("matchmaker_version")
                    .filter(|value| !value.trim().is_empty())
                    .or_else(|| bootstrap.matchmaker_version.clone())
                    .unwrap_or_else(|| DEFAULT_VERSION.to_string()),
                player_name: params
                    .get("name")
                    .map(|name| sanitize_player_name(&name))
                    .unwrap_or_default(),
                room,
            }
        }

        fn bevy_options(&self) -> WebClientOptions {
            WebClientOptions {
                matchmaker_url: self.matchmaker_url.clone(),
                matchmaker_game: self.matchmaker_game.clone(),
                matchmaker_version: self.matchmaker_version.clone(),
                room: self.room,
                name: self.player_name.clone(),
                canvas_selector: format!("#{CANVAS_ID}"),
            }
        }
    }

    #[derive(Default)]
    struct BrowserBootstrap {
        matchmaker_url: Option<String>,
        matchmaker_game: Option<String>,
        matchmaker_version: Option<String>,
    }

    impl BrowserBootstrap {
        fn from_window() -> Self {
            Self {
                matchmaker_url: bootstrap_string("matchmaker_url"),
                matchmaker_game: bootstrap_string("matchmaker_game"),
                matchmaker_version: bootstrap_string("matchmaker_version"),
            }
        }
    }

    fn bootstrap_string(key: &str) -> Option<String> {
        let window = web_sys::window()?;
        let bootstrap = js_sys::Reflect::get(
            window.as_ref(),
            &wasm_bindgen::JsValue::from_str("LIGHTRIDER_BOOTSTRAP"),
        )
        .ok()?;
        if bootstrap.is_null() || bootstrap.is_undefined() {
            return None;
        }
        js_sys::Reflect::get(&bootstrap, &wasm_bindgen::JsValue::from_str(key))
            .ok()
            .and_then(|value| value.as_string())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    }

    fn window_location() -> web_sys::Location {
        web_sys::window()
            .expect("browser window is unavailable")
            .location()
    }

    fn default_matchmaker_url() -> String {
        format!("{}/matchmaker/ws", same_origin_matchmaker_base_url())
    }

    fn routed_matchmaker_url(route: &str) -> Option<String> {
        let route = route.trim().trim_matches('/').to_ascii_lowercase();
        if route.is_empty() {
            return None;
        }
        if route == "default" || route == "root" {
            return Some(default_matchmaker_url());
        }
        if !route.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        }) {
            return None;
        }
        Some(format!(
            "{}/matchmaker/{route}/ws",
            same_origin_matchmaker_base_url()
        ))
    }

    fn same_origin_matchmaker_base_url() -> String {
        let location = window_location();
        let protocol = match location.protocol().as_deref() {
            Ok("https:") => "wss",
            _ => "ws",
        };
        let host = location
            .host()
            .expect("browser location host is unavailable");
        format!("{protocol}://{host}")
    }

    fn parse_room_param(value: Option<String>) -> RoomJoinMode {
        let Some(value) = value else {
            return RoomJoinMode::Auto;
        };
        parse_room_input(&value).unwrap_or(RoomJoinMode::Auto)
    }

    fn parse_room_input(value: &str) -> Result<RoomJoinMode, String> {
        let trimmed = value.trim();
        if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("auto") {
            return Ok(RoomJoinMode::Auto);
        }
        if trimmed.eq_ignore_ascii_case("new") {
            return Ok(RoomJoinMode::New);
        }
        RoomCode::parse(trimmed)
            .map(RoomJoinMode::Private)
            .or_else(|_| {
                trimmed
                    .parse::<u64>()
                    .map(|id| RoomJoinMode::Specific(RoomId(id)))
            })
            .map_err(|_| "Use a four-letter room code.".to_string())
    }

    fn room_input_value(room: RoomJoinMode) -> String {
        match room {
            RoomJoinMode::Auto => String::new(),
            RoomJoinMode::New => "NEW".to_string(),
            RoomJoinMode::Specific(id) => id.0.to_string(),
            RoomJoinMode::Private(code) => code.to_string(),
        }
    }

    fn sanitize_player_name(name: &str) -> String {
        name.trim().chars().take(18).collect()
    }

    fn apply_settings_to_url(name: &str, room: RoomJoinMode) {
        let location = window_location();
        let search = location.search().unwrap_or_default();
        let params = web_sys::UrlSearchParams::new_with_str(&search)
            .expect("failed to parse query parameters");
        if name.is_empty() {
            params.delete("name");
        } else {
            params.set("name", name);
        }
        match room {
            RoomJoinMode::Auto => params.delete("room"),
            _ => params.set("room", &room_input_value(room)),
        }
        let pathname = location.pathname().unwrap_or_else(|_| "/".to_string());
        let query = params.to_string().as_string().unwrap_or_default();
        let href = if query.is_empty() {
            pathname
        } else {
            format!("{pathname}?{query}")
        };
        if location.href().ok().as_deref() != Some(&href) {
            let _ = location.set_href(&href);
        }
    }
}

#[cfg(all(target_arch = "wasm32", feature = "lightyear-matchmaker"))]
fn main() {
    wasm::run();
}

#[cfg(all(target_arch = "wasm32", not(feature = "lightyear-matchmaker")))]
fn main() {
    panic!("lightrider-web must be built with the `lightyear-matchmaker` feature");
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("lightrider-web is intended for wasm32-unknown-unknown builds");
}
