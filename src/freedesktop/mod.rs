#[cfg(feature = "color-scheme")]
use crate::ColorScheme;
#[cfg(feature = "contrast")]
use crate::Contrast;
#[cfg(feature = "double-click-interval")]
use crate::DoubleClickInterval;
#[cfg(feature = "reduced-motion")]
use crate::ReducedMotion;
#[cfg(feature = "scrollbar-visibility")]
use crate::ScrollbarVisibility;
#[cfg(feature = "accent-color")]
use crate::{AccentColor, Srgba};

use crate::async_rt::block_on;
use crate::stream_utils::{Left, Right, Scan};
use crate::{AvailablePreferences, Interest};
use cfg_if::cfg_if;
use futures_lite::{stream, FutureExt as _, Stream, StreamExt as _};
#[cfg(feature = "accent-color")]
use std::env;
use std::time::Duration;
use zbus::{
    proxy::SignalStream,
    zvariant::{OwnedValue, Value},
    Connection, Message, Proxy,
};

#[cfg(feature = "log")]
fn log_dbus_connection_error(err: &zbus::Error) {
    log::warn!("failed to connect to dbus: {err:?}");
}

#[cfg(not(feature = "log"))]
fn log_dbus_connection_error(_err: &zbus::Error) {}

#[cfg(feature = "log")]
fn log_initial_settings_retrieval_error(err: &zbus::Error) {
    log::warn!("error retrieving the initial setting values: {err:?}");
}

#[cfg(not(feature = "log"))]
fn log_initial_settings_retrieval_error(_err: &zbus::Error) {}

#[cfg(feature = "log")]
fn log_message_error(err: &zbus::Error) {
    log::debug!("failed to process incoming dbus message: {err:?}");
}

#[cfg(not(feature = "log"))]
fn log_message_error(_err: &zbus::Error) {}

const APPEARANCE: &str = "org.freedesktop.appearance";
#[cfg(any(feature = "reduced-motion", feature = "accent-color", feature = "scrollbar-visibility"))]
const GNOME_INTERFACE: &str = "org.gnome.desktop.interface";
#[cfg(feature = "double-click-interval")]
const GNOME_PERIPHERALS_MOUSE: &str = "org.gnome.desktop.peripherals.mouse";
#[cfg(feature = "double-click-interval")]
const DOUBLE_CLICK: &str = "double-click";
#[cfg(feature = "color-scheme")]
const COLOR_SCHEME: &str = "color-scheme";
#[cfg(feature = "contrast")]
const CONTRAST: &str = "contrast";
#[cfg(feature = "accent-color")]
const ACCENT_COLOR: &str = "accent-color";
#[cfg(feature = "reduced-motion")]
const ENABLE_ANIMATIONS: &str = "enable-animations";
#[cfg(feature = "reduced-motion")]
const REDUCED_MOTION: &str = "reduced-motion";
#[cfg(feature = "accent-color")]
const GTK_THEME: &str = "gtk-theme";
#[cfg(feature = "scrollbar-visibility")]
const OVERLAY_SCROLLING: &str = "overlay-scrolling";

pub(crate) type PreferencesStream = stream::Boxed<AvailablePreferences>;

pub(crate) fn stream(interest: Interest) -> PreferencesStream {
    preferences_stream(interest).boxed()
}

pub(crate) fn default_stream() -> PreferencesStream {
    stream::once(AvailablePreferences::default()).boxed()
}

pub(crate) fn once_blocking(interest: Interest, timeout: Duration) -> Option<AvailablePreferences> {
    block_on(stream(interest).next().or(timer(timeout)))
}

cfg_if! {
    if #[cfg(feature = "tokio")] {
        async fn timer<T>(duration: Duration) -> Option<T> {
            tokio::time::sleep(duration).await;
            None
        }
    } else if #[cfg(feature = "async-io")] {
        async fn timer<T>(duration: Duration) -> Option<T> {
            async_io::Timer::after(duration).await;
            None
        }
    }
}

fn preferences_stream(interest: Interest) -> impl Stream<Item = AvailablePreferences> {
    stream::once_future(subscribe(interest)).flat_map(move |(state, preferences, stream)| {
        let initial_value = stream::once(preferences);
        let stream = stream.map(Left).unwrap_or_else(|| Right(stream::empty()));
        initial_value.chain(changes(interest, state, preferences, stream))
    })
}

fn changes(
    interest: Interest,
    state: State,
    preferences: AvailablePreferences,
    stream: impl Stream<Item = Message>,
) -> impl Stream<Item = AvailablePreferences> {
    Scan::new(
        stream,
        (state, preferences),
        move |(mut state, mut preferences), message| async move {
            if let Err(err) = apply_message(interest, &mut state, &mut preferences, message).await {
                log_message_error(&err);
            }
            Some(((state, preferences), preferences))
        },
    )
}

async fn subscribe(
    interest: Interest,
) -> (State, AvailablePreferences, Option<SignalStream<'static>>) {
    match connect().await {
        Ok(proxy) => {
            let stream = setting_changed(&proxy, interest)
                .await
                .inspect_err(log_dbus_connection_error)
                .ok();
            let (state, preferences) = initial_preferences(&proxy, interest)
                .await
                .inspect_err(log_initial_settings_retrieval_error)
                .unwrap_or_default();
            (state, preferences, stream)
        }
        Err(err) => {
            log_dbus_connection_error(&err);
            Default::default()
        }
    }
}

async fn connect() -> zbus::Result<Proxy<'static>> {
    let connection = Connection::session().await?;
    settings_proxy(&connection).await
}

async fn apply_message(
    interest: Interest,
    _state: &mut State,
    preferences: &mut AvailablePreferences,
    message: Message,
) -> Result<(), zbus::Error> {
    let body = message.body();
    let (namespace, key, value): (&str, &str, Value) = body.deserialize()?;
    match (namespace, key) {
        #[cfg(feature = "color-scheme")]
        (APPEARANCE, COLOR_SCHEME) if interest.is(Interest::ColorScheme) => {
            preferences.color_scheme = parse_color_scheme(value);
        }
        #[cfg(feature = "contrast")]
        (APPEARANCE, CONTRAST) if interest.is(Interest::Contrast) => {
            preferences.contrast = parse_contrast(value);
        }
        #[cfg(feature = "reduced-motion")]
        (APPEARANCE, REDUCED_MOTION) if interest.is(Interest::ReducedMotion) => {
            _state.has_reduced_motion_setting = true;
            preferences.reduced_motion = parse_reduced_motion(value);
        }
        #[cfg(feature = "reduced-motion")]
        (GNOME_INTERFACE, ENABLE_ANIMATIONS)
            if interest.is(Interest::ReducedMotion) && !_state.has_reduced_motion_setting =>
        {
            preferences.reduced_motion = parse_enable_animation(value);
        }
        #[cfg(feature = "accent-color")]
        (APPEARANCE, ACCENT_COLOR) if interest.is(Interest::AccentColor) => {
            _state.has_accent_color = true;
            preferences.accent_color = parse_accent_color(value);
        }
        #[cfg(feature = "accent-color")]
        (GNOME_INTERFACE, GTK_THEME)
            if interest.is(Interest::AccentColor) && !_state.has_accent_color && is_ubuntu() =>
        {
            preferences.accent_color = parse_accent_color_from_yaru_theme(value);
        }
        #[cfg(feature = "double-click-interval")]
        (GNOME_PERIPHERALS_MOUSE, DOUBLE_CLICK) if interest.is(Interest::DoubleClickInterval) => {
            preferences.double_click_interval = parse_double_click(value);
        }
        #[cfg(feature = "scrollbar-visibility")]
        (GNOME_INTERFACE, OVERLAY_SCROLLING) if interest.is(Interest::ScrollbarVisibility) => {
            preferences.scrollbar_visibility = parse_overlay_scrolling(value);
        }
        _ => {}
    }
    Ok(())
}

async fn initial_preferences(
    proxy: &Proxy<'_>,
    interest: Interest,
) -> zbus::Result<(State, AvailablePreferences)> {
    let mut preferences = AvailablePreferences::default();
    let mut _state = State::default();
    #[cfg(feature = "color-scheme")]
    if interest.is(Interest::ColorScheme) {
        preferences.color_scheme = read_setting(proxy, APPEARANCE, COLOR_SCHEME)
            .await
            .map(parse_color_scheme)
            .unwrap_or_default();
    }
    #[cfg(feature = "contrast")]
    if interest.is(Interest::Contrast) {
        preferences.contrast = read_setting(proxy, APPEARANCE, CONTRAST)
            .await
            .map(parse_contrast)
            .unwrap_or_default();
    }
    #[cfg(feature = "reduced-motion")]
    if interest.is(Interest::ReducedMotion) {
        let reduced_motion_value = read_setting(proxy, APPEARANCE, REDUCED_MOTION).await;
        if let Some(reduced_motion_value) = reduced_motion_value {
            _state.has_reduced_motion_setting = true;
            preferences.reduced_motion = parse_reduced_motion(reduced_motion_value);
        } else {
            // Fallback until the portal preference is widely supported.
            preferences.reduced_motion = read_setting(proxy, GNOME_INTERFACE, ENABLE_ANIMATIONS)
                .await
                .map(parse_enable_animation)
                .unwrap_or_default();
        }
    }
    #[cfg(feature = "accent-color")]
    if interest.is(Interest::AccentColor) {
        let accent_color_value = read_setting(proxy, APPEARANCE, ACCENT_COLOR).await;
        if let Some(accent_color_value) = accent_color_value {
            _state.has_accent_color = true;
            preferences.accent_color = parse_accent_color(accent_color_value);
        } else if is_ubuntu() {
            preferences.accent_color = read_setting(proxy, GNOME_INTERFACE, GTK_THEME)
                .await
                .map(parse_accent_color_from_yaru_theme)
                .unwrap_or_default()
        }
    }
    #[cfg(feature = "double-click-interval")]
    if interest.is(Interest::DoubleClickInterval) {
        preferences.double_click_interval =
            read_setting(proxy, GNOME_PERIPHERALS_MOUSE, DOUBLE_CLICK)
                .await
                .map(parse_double_click)
                .unwrap_or_default();
    }
    #[cfg(feature = "scrollbar-visibility")]
    if interest.is(Interest::ScrollbarVisibility) {
        preferences.scrollbar_visibility = read_setting(proxy, GNOME_INTERFACE, OVERLAY_SCROLLING)
            .await
            .map(parse_overlay_scrolling)
            .unwrap_or_default();
    }
    Ok((_state, preferences))
}

async fn settings_proxy<'a>(connection: &Connection) -> zbus::Result<Proxy<'a>> {
    Proxy::new(
        connection,
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.Settings",
    )
    .await
}

async fn read_setting(proxy: &Proxy<'_>, namespace: &str, key: &str) -> Option<Value<'static>> {
    proxy
        .call::<_, _, OwnedValue>("Read", &(namespace, key))
        .await
        .ok()
        .map(Value::from)
        .map(flatten_value)
}

// `Read` returns a variant inside a variant *sigh*.
// In theory there's `ReadOne` which fixes this but I
// haven't checked how ubiquitously it is available.
fn flatten_value(value: Value<'_>) -> Value<'_> {
    if let Value::Value(inner) = value {
        *inner
    } else {
        value
    }
}

async fn setting_changed(
    proxy: &Proxy<'_>,
    interest: Interest,
) -> zbus::Result<SignalStream<'static>> {
    proxy
        .receive_signal_with_args("SettingChanged", signal_filter(interest))
        .await
}

fn signal_filter(
    #[cfg_attr(
        not(any(feature = "_gnome_only", feature = "accent-color")),
        expect(unused_variables)
    )]
    interest: Interest,
) -> &'static [(u8, &'static str)] {
    #[cfg(feature = "_gnome_only")]
    if interest.is(Interest::GnomeOnly) {
        return &[];
    }
    #[cfg(feature = "accent-color")]
    if interest.is(Interest::AccentColor) && is_ubuntu() {
        return &[];
    }
    &[(0, APPEARANCE)]
}

/// See <https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Settings.html>.
#[cfg(feature = "color-scheme")]
fn parse_color_scheme(value: Value) -> ColorScheme {
    match u32::try_from(value) {
        // > `1`: Prefer dark appearance
        Ok(1) => ColorScheme::Dark,
        // > `2`: Prefer light appearance
        Ok(2) => ColorScheme::Light,
        // > `0`: No preference
        // > Unknown values should be treated as `0` (no preference).
        Ok(0) | Ok(_) | Err(_) => ColorScheme::NoPreference,
    }
}

#[cfg(feature = "contrast")]
fn parse_contrast(value: Value) -> Contrast {
    match u32::try_from(value) {
        // > `1`: Higher contrast
        Ok(1) => Contrast::More,
        // > `0`: No preference (normal contrast)
        // > Unknown values should be treated as `0` (no preference).
        Ok(0) | Ok(_) | Err(_) => Contrast::NoPreference,
    }
}

// > Indicates the system’s preferred accent color as a tuple of RGB values in the sRGB color space,
// > in the range [0,1]. Out-of-range RGB values should be treated as an unset accent color.
#[cfg(feature = "accent-color")]
fn parse_accent_color(value: Value) -> AccentColor {
    if let Ok((red, green, blue)) = value.downcast() {
        AccentColor(Some(Srgba {
            red,
            green,
            blue,
            alpha: 1.0,
        }))
    } else {
        AccentColor(None)
    }
}

// Ubuntu 24.04 LTS supports accent colors but not yet the standardized `accent-color` key.
// This workaround can be removed once it goes EOL which is after May 2029 (technically longer with Ubuntu Pro,
// but I don't particularly care about those users).
#[cfg(feature = "accent-color")]
fn parse_accent_color_from_yaru_theme(value: Value) -> AccentColor {
    let Ok(theme): Result<&str, _> = value.downcast_ref() else {
        return AccentColor(None);
    };
    let theme = theme.strip_suffix("-dark").unwrap_or(theme);
    // Theme names copied from:
    // <https://github.com/ubuntu/yaru.dart/blob/c6bea50559e603e50fe42916b547df26fbfc7f15/lib/src/settings/inherited_theme.dart#L204-L214>
    // Color values copied from:
    // <https://github.com/ubuntu/yaru/blob/f01c3e9a257296242806f8e0c5d4a660516f2181/common/accent-colors.scss.in>
    let color = match theme {
        "Yaru" => 0xE95420,
        "Yaru-prussiangreen" => 0x308280,
        "Yaru-bark" => 0x787859,
        "Yaru-blue" => 0x0073E5,
        "Yaru-wartybrown" => 0xB39169,
        "Yaru-magenta" => 0xB34CB3,
        "Yaru-olive" => 0x4B8501,
        "Yaru-purple" => 0x7764D8,
        "Yaru-sage" => 0x657B69,
        "Yaru-red" => 0xDA3450,
        "Yaru-viridian" => 0x03875B,
        _ => return AccentColor(None),
    };
    AccentColor(Some(srgba_from_u32_rgb(color)))
}

#[cfg(feature = "accent-color")]
fn srgba_from_u32_rgb(value: u32) -> Srgba {
    Srgba::from_u8_array([
        (value >> 16 & 0xFF) as u8,
        (value >> 8 & 0xFF) as u8,
        (value & 0xFF) as u8,
        0xFF,
    ])
}

#[cfg(feature = "reduced-motion")]
fn parse_reduced_motion(value: Value) -> ReducedMotion {
    match u32::try_from(value) {
        // > `1`: Reduced motion
        Ok(1) => ReducedMotion::Reduce,
        // > `0`: No preference
        // > Unknown values should be treated as `0` (no preference).
        Ok(0) | Ok(_) | Err(_) => ReducedMotion::NoPreference,
    }
}

#[cfg(feature = "reduced-motion")]
fn parse_enable_animation(value: Value) -> ReducedMotion {
    match bool::try_from(value) {
        Ok(false) => ReducedMotion::Reduce,
        Ok(true) | Err(_) => ReducedMotion::NoPreference,
    }
}

// Stored as integer (milliseconds):
// <https://gitlab.gnome.org/GNOME/gsettings-desktop-schemas/-/blob/6ad9aaea4dc2929770f2fdf9112280aa5081b6de/schemas/org.gnome.desktop.peripherals.gschema.xml.in#L139>
#[cfg(feature = "double-click-interval")]
fn parse_double_click(value: Value) -> DoubleClickInterval {
    // We can't directly convert value to u64 because the underlying value is an i32.
    let value = i32::try_from(value)
        .ok()
        .and_then(|v| u64::try_from(v).ok())
        .map(Duration::from_millis);
    DoubleClickInterval(value)
}

// Stored as a boolean:
// <https://gitlab.gnome.org/GNOME/gsettings-desktop-schemas/-/blob/6ad9aaea4dc2929770f2fdf9112280aa5081b6de/schemas/org.gnome.desktop.interface.gschema.xml.in#L261>
#[cfg(feature = "scrollbar-visibility")]
fn parse_overlay_scrolling(value: Value) -> ScrollbarVisibility {
    match bool::try_from(value) {
        Ok(false) => ScrollbarVisibility::Always,
        Ok(true) => ScrollbarVisibility::Auto,
        Err(_) => ScrollbarVisibility::NoPreference,
    }
}

#[cfg(feature = "accent-color")]
fn is_ubuntu() -> bool {
    use std::sync::LazyLock;
    static IS_UBUNTU: LazyLock<bool> = LazyLock::new(|| {
        let current_desktop = env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
        current_desktop.split(':').any(|name| name == "ubuntu")
    });
    *IS_UBUNTU
}

#[derive(Debug, Default)]
struct State {
    #[cfg(feature = "reduced-motion")]
    has_reduced_motion_setting: bool,
    #[cfg(feature = "accent-color")]
    has_accent_color: bool,
}
