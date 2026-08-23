#[cfg(all(target_arch = "wasm32", feature = "lightyear-matchmaker"))]
mod wasm {
    use client::WebClientOptions;
    use leptos::prelude::*;
    use leptos_bevy_canvas::prelude::*;
    use lightyear_matchmaker_core::ProviderKind;
    use shared::network::protocol::prelude::{RoomCode, RoomId, RoomJoinMode};
    use wasm_bindgen::JsCast;

    const DEFAULT_GAME: &str = "lightrider";
    const DEFAULT_VERSION: &str = "dev";
    const CANVAS_ID: &str = "bevy_canvas";

    #[derive(Clone, Debug, PartialEq)]
    struct BrowserSettings {
        matchmaker_url: String,
        matchmaker_game: String,
        matchmaker_version: String,
        matchmaker_provider: Option<ProviderKind>,
        player_name: String,
        room: RoomJoinMode,
        headless: bool,
        auto_respawn: bool,
        turn_stress_hz: f32,
        turn_stress_seconds: f32,
        browser_rtt_probe: bool,
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
        let (name, set_name) = signal(settings.player_name.clone());
        let (room_code, set_room_code) = signal(initial_room_code);
        let (room_badge, set_room_badge) = signal(initial_room_badge);
        let (modal_open, set_modal_open) = signal(show_modal);
        let (error, set_error) = signal(String::new());
        let can_start_bevy = startup_error.is_none();
        publish_player_settings(&settings.player_name, settings.room);
        let initial_bevy_options = (can_start_bevy && !show_modal).then(|| settings.bevy_options());
        if initial_bevy_options.is_some() {
            focus_canvas_soon();
        }
        let (bevy_options, set_bevy_options) = signal(initial_bevy_options);
        let submit_settings = settings.clone();
        leptos::prelude::window_event_listener(leptos::ev::keydown, move |event| {
            if event.key() == "Escape" {
                set_modal_open.set(true);
            }
        });
        let game_stage = move || {
            if let Some(options) = bevy_options.get() {
                view! {
                    <BevyCanvas
                        canvas_id=CANVAS_ID
                        init=move || client::web_app(options.clone())
                    />
                }
                .into_any()
            } else {
                view! { <div class="game-stage-blocked"></div> }.into_any()
            }
        };

        let on_name_input = move |event| {
            set_name.set(event_target_value(&event));
        };
        let on_room_input = move |event| {
            set_room_code.set(event_target_value(&event).to_ascii_uppercase());
        };
        let on_submit = move |event: web_sys::SubmitEvent| {
            event.prevent_default();
            let player_name = sanitize_player_name(&name.get());
            if player_name.is_empty() {
                set_error.set("Enter a name.".to_string());
                return;
            }
            let room = room_code.get();
            match parse_room_input(&room) {
                Ok(room_mode) => {
                    set_error.set(String::new());
                    set_room_badge.set(room_badge_text(room_mode));
                    replace_settings_in_url(&player_name, room_mode);
                    publish_player_settings(&player_name, room_mode);
                    set_modal_open.set(false);
                    if can_start_bevy && bevy_options.get_untracked().is_none() {
                        set_bevy_options.set(Some(
                            submit_settings.bevy_options_with(player_name, room_mode),
                        ));
                    }
                    focus_canvas_soon();
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
                    </section>
                </div>
                <div class=move || if room_badge.get().is_some() { "room-code-badge" } else { "room-code-badge hidden" }>
                    {move || room_badge.get().unwrap_or_default()}
                </div>
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

    fn focus_canvas_soon() {
        focus_canvas();
        let Some(window) = web_sys::window() else {
            return;
        };
        let callback = wasm_bindgen::closure::Closure::once(focus_canvas);
        let _ = window.request_animation_frame(callback.as_ref().unchecked_ref());
        callback.forget();
    }

    fn focus_canvas() {
        let Some(canvas) =
            document_element_by_id(CANVAS_ID).and_then(|element| element.dyn_into().ok())
        else {
            return;
        };
        let canvas: web_sys::HtmlElement = canvas;
        let _ = canvas.set_attribute("tabindex", "0");
        let _ = canvas.focus();
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
                matchmaker_provider: parse_provider_param(params.get("provider"))
                    .or_else(|| parse_provider_param(params.get("matchmaker_provider"))),
                player_name: params
                    .get("name")
                    .map(|name| sanitize_player_name(&name))
                    .unwrap_or_default(),
                room,
                headless: parse_bool_param(&params, "headless")
                    || matches!(
                        params.get("render").unwrap_or_default().trim(),
                        "0" | "false" | "off"
                    ),
                auto_respawn: parse_bool_param(&params, "auto_respawn")
                    || parse_bool_param(&params, "auto-respawn"),
                turn_stress_hz: parse_positive_f32_param(&params, "turn_stress_hz")
                    .or_else(|| parse_positive_f32_param(&params, "turn-stress-hz"))
                    .unwrap_or(0.0),
                turn_stress_seconds: parse_positive_f32_param(&params, "turn_stress_seconds")
                    .or_else(|| parse_positive_f32_param(&params, "turn-stress-seconds"))
                    .unwrap_or(0.0),
                browser_rtt_probe: parse_bool_param(&params, "rtt_probe")
                    || parse_bool_param(&params, "browser_rtt_probe"),
            }
        }

        fn bevy_options(&self) -> WebClientOptions {
            self.bevy_options_with(self.player_name.clone(), self.room)
        }

        fn bevy_options_with(&self, player_name: String, room: RoomJoinMode) -> WebClientOptions {
            WebClientOptions {
                matchmaker_url: self.matchmaker_url.clone(),
                matchmaker_game: self.matchmaker_game.clone(),
                matchmaker_version: self.matchmaker_version.clone(),
                matchmaker_provider: self.matchmaker_provider,
                room,
                name: player_name,
                canvas_selector: format!("#{CANVAS_ID}"),
                headless: self.headless,
                auto_respawn: self.auto_respawn,
                turn_stress_hz: self.turn_stress_hz,
                turn_stress_seconds: self.turn_stress_seconds,
                browser_rtt_probe: self.browser_rtt_probe,
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

    fn parse_provider_param(value: Option<String>) -> Option<ProviderKind> {
        match value?.trim().to_ascii_lowercase().as_str() {
            "static" => Some(ProviderKind::Static),
            "edgegap" => Some(ProviderKind::Edgegap),
            "gameflow" => Some(ProviderKind::Gameflow),
            _ => None,
        }
    }

    fn parse_positive_f32_param(params: &web_sys::UrlSearchParams, key: &str) -> Option<f32> {
        let parsed = params.get(key)?.trim().parse::<f32>().ok()?;
        parsed.is_finite().then_some(parsed.max(0.0))
    }

    fn parse_bool_param(params: &web_sys::UrlSearchParams, key: &str) -> bool {
        matches!(
            params
                .get(key)
                .unwrap_or_default()
                .trim()
                .to_ascii_lowercase()
                .as_str(),
            "1" | "true" | "yes" | "on"
        )
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

    fn publish_player_settings(name: &str, room: RoomJoinMode) {
        let Some(window) = web_sys::window() else {
            return;
        };
        let object = js_sys::Object::new();
        let _ = js_sys::Reflect::set(
            &object,
            &wasm_bindgen::JsValue::from_str("name"),
            &wasm_bindgen::JsValue::from_str(name),
        );
        let _ = js_sys::Reflect::set(
            &object,
            &wasm_bindgen::JsValue::from_str("room"),
            &wasm_bindgen::JsValue::from_str(&room_input_value(room)),
        );
        let _ = js_sys::Reflect::set(
            window.as_ref(),
            &wasm_bindgen::JsValue::from_str("LIGHTRIDER_PLAYER_SETTINGS"),
            &object,
        );
    }

    fn replace_settings_in_url(name: &str, room: RoomJoinMode) {
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
            if let Some(history) = web_sys::window().and_then(|window| window.history().ok()) {
                let _ =
                    history.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(&href));
            }
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
