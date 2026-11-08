#[cfg(feature = "color-scheme")]
use crate::ColorScheme;
#[cfg(feature = "contrast")]
use crate::Contrast;
#[cfg(feature = "double-click-interval")]
use crate::DoubleClickInterval;
#[cfg(feature = "power-saver")]
use crate::PowerSaver;
#[cfg(feature = "reduced-motion")]
use crate::ReducedMotion;
#[cfg(feature = "scrollbar-visibility")]
use crate::ScrollbarVisibility;
#[cfg(feature = "accent-color")]
use crate::{AccentColor, Srgba};

use crate::async_rt::block_on;
#[cfg(feature = "power-saver")]
use crate::stream_utils::Merge;
use crate::stream_utils::{Left, Right, Scan};
use crate::{AvailablePreferences, Interest};
use cfg_if::cfg_if;
use futures_lite::{stream, FutureExt as _, Stream, StreamExt as _};
#[cfg(feature = "power-saver")]
use std::collections::HashMap;
#[cfg(feature = "accent-color")]
use std::env;
use std::time::Duration;
#[cfg(feature = "power-saver")]
use zbus::proxy::{Builder, CacheProperties};
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

#[cfg(all(feature = "log", feature = "_settings-portal"))]
fn log_initial_settings_retrieval_error(err: &zbus::Error) {
    log::warn!("error retrieving the initial setting values: {err:?}");
}

#[cfg(all(not(feature = "log"), feature = "_settings-portal"))]
fn log_initial_settings_retrieval_error(_err: &zbus::Error) {}

#[cfg(feature = "log")]
fn log_message_error(err: &zbus::Error) {
    log::debug!("failed to process incoming dbus message: {err:?}");
}

#[cfg(not(feature = "log"))]
fn log_message_error(_err: &zbus::Error) {}

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
#[cfg(feature = "power-saver")]
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";
#[cfg(feature = "power-saver")]
const POWER_PROFILE_MONITOR: &str = "org.freedesktop.portal.PowerProfileMonitor";
#[cfg(feature = "power-saver")]
const POWER_SAVER_ENABLED: &str = "power-saver-enabled";
#[cfg(feature = "_settings-portal")]
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
    stream::once_future(subscribe(interest)).flat_map(move |subscription| {
        let Subscription {
            state,
            preferences,
            #[cfg(feature = "_settings-portal")]
            setting_changed,
            #[cfg(feature = "power-saver")]
            power_profile_changed,
        } = subscription;
        let initial_value = stream::once(preferences);
        #[cfg(feature = "_settings-portal")]
        let signals = optional_stream(setting_changed).map(Signal::SettingChanged);
        #[cfg(not(feature = "_settings-portal"))]
        let signals = stream::empty();
        #[cfg(feature = "power-saver")]
        let signals = Merge::new(
            signals,
            optional_stream(power_profile_changed).map(Signal::PowerProfileChanged),
        );
        initial_value.chain(changes(interest, state, preferences, signals))
    })
}

fn optional_stream(stream: Option<SignalStream<'static>>) -> impl Stream<Item = Message> {
    stream.map(Left).unwrap_or_else(|| Right(stream::empty()))
}

#[cfg_attr(not(feature = "_settings-portal"), expect(unused_mut))]
fn changes(
    interest: Interest,
    state: State,
    preferences: AvailablePreferences,
    stream: impl Stream<Item = Signal>,
) -> impl Stream<Item = AvailablePreferences> {
    Scan::new(
        stream,
        (state, preferences),
        move |(mut state, mut preferences), signal| async move {
            let result = match signal {
                #[cfg(feature = "_settings-portal")]
                Signal::SettingChanged(message) => {
                    apply_message(interest, &mut state, &mut preferences, message).await
                }
                #[cfg(feature = "power-saver")]
                Signal::PowerProfileChanged(message) => {
                    apply_power_profile_change(interest, &mut preferences, &message)
                }
            };
            if let Err(err) = result {
                log_message_error(&err);
            }
            Some(((state, preferences), preferences))
        },
    )
}

/// A signal from one of the portals that mundy subscribes to.
enum Signal {
    /// `SettingChanged` from the Settings portal.
    #[cfg(feature = "_settings-portal")]
    SettingChanged(Message),
    /// `PropertiesChanged` from the Power Profile Monitor portal.
    #[cfg(feature = "power-saver")]
    PowerProfileChanged(Message),
}

#[derive(Default)]
struct Subscription {
    state: State,
    preferences: AvailablePreferences,
    #[cfg(feature = "_settings-portal")]
    setting_changed: Option<SignalStream<'static>>,
    #[cfg(feature = "power-saver")]
    power_profile_changed: Option<SignalStream<'static>>,
}

async fn subscribe(interest: Interest) -> Subscription {
    let connection = match Connection::session().await {
        Ok(connection) => connection,
        Err(err) => {
            log_dbus_connection_error(&err);
            return Subscription::default();
        }
    };
    let mut subscription = Subscription::default();
    #[cfg(feature = "_settings-portal")]
    if uses_settings_portal(interest) {
        match settings_proxy(&connection).await {
            Ok(proxy) => {
                subscription.setting_changed = setting_changed(&proxy, interest)
                    .await
                    .inspect_err(log_dbus_connection_error)
                    .ok();
                (subscription.state, subscription.preferences) =
                    initial_preferences(&proxy, interest)
                        .await
                        .inspect_err(log_initial_settings_retrieval_error)
                        .unwrap_or_default();
            }
            Err(err) => log_dbus_connection_error(&err),
        }
    }
    #[cfg(feature = "power-saver")]
    if interest.is(Interest::PowerSaver) {
        // Subscribed before the first read so that no change falls between the two.
        subscription.power_profile_changed = power_profile_changed(&connection)
            .await
            .inspect_err(log_dbus_connection_error)
            .ok();
        subscription.preferences.power_saver = read_power_saver_enabled(&connection)
            .await
            .map(parse_power_saver_enabled)
            .unwrap_or_default();
    }
    subscription
}

/// Every preference except the power saver is read from the Settings portal.
#[cfg(all(feature = "_settings-portal", feature = "power-saver"))]
fn uses_settings_portal(interest: Interest) -> bool {
    !Interest::PowerSaver.is(interest)
}

#[cfg(all(feature = "_settings-portal", not(feature = "power-saver")))]
fn uses_settings_portal(_interest: Interest) -> bool {
    true
}

#[cfg(feature = "_settings-portal")]
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

#[cfg(feature = "_settings-portal")]
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

#[cfg(feature = "_settings-portal")]
async fn settings_proxy<'a>(connection: &Connection) -> zbus::Result<Proxy<'a>> {
    Proxy::new(
        connection,
        PORTAL,
        PORTAL_PATH,
        "org.freedesktop.portal.Settings",
    )
    .await
}

/// A proxy for one of the portal's interfaces that reads each property anew,
/// so that it doesn't subscribe to property changes itself.
#[cfg(feature = "power-saver")]
async fn uncached_portal_proxy<'a>(
    connection: &Connection,
    interface: &'static str,
) -> zbus::Result<Proxy<'a>> {
    Builder::new(connection)
        .destination(PORTAL)?
        .path(PORTAL_PATH)?
        .interface(interface)?
        .cache_properties(CacheProperties::No)
        .build()
        .await
}

#[cfg(feature = "power-saver")]
async fn power_profile_changed(connection: &Connection) -> zbus::Result<SignalStream<'static>> {
    uncached_portal_proxy(connection, PROPERTIES)
        .await?
        .receive_signal_with_args("PropertiesChanged", &[(0, POWER_PROFILE_MONITOR)])
        .await
}

#[cfg(feature = "power-saver")]
async fn read_power_saver_enabled(connection: &Connection) -> Option<Value<'static>> {
    uncached_portal_proxy(connection, POWER_PROFILE_MONITOR)
        .await
        .ok()?
        .get_property::<OwnedValue>(POWER_SAVER_ENABLED)
        .await
        .ok()
        .map(Value::from)
}

/// Reads `power-saver-enabled` from the changed properties, as GLib's `GPowerProfileMonitor` does:
/// <https://gitlab.gnome.org/GNOME/glib/-/blob/main/gio/gpowerprofilemonitorportal.c>
#[cfg(feature = "power-saver")]
fn apply_power_profile_change(
    interest: Interest,
    preferences: &mut AvailablePreferences,
    message: &Message,
) -> Result<(), zbus::Error> {
    let body = message.body();
    let (interface, mut changed, _invalidated): (&str, HashMap<&str, Value>, Vec<&str>) =
        body.deserialize()?;
    if interface == POWER_PROFILE_MONITOR && interest.is(Interest::PowerSaver) {
        if let Some(value) = changed.remove(POWER_SAVER_ENABLED) {
            preferences.power_saver = parse_power_saver_enabled(value);
        }
    }
    Ok(())
}

#[cfg(feature = "_settings-portal")]
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
#[cfg(feature = "_settings-portal")]
fn flatten_value(value: Value<'_>) -> Value<'_> {
    if let Value::Value(inner) = value {
        *inner
    } else {
        value
    }
}

#[cfg(feature = "_settings-portal")]
async fn setting_changed(
    proxy: &Proxy<'_>,
    interest: Interest,
) -> zbus::Result<SignalStream<'static>> {
    proxy
        .receive_signal_with_args("SettingChanged", signal_filter(interest))
        .await
}

#[cfg(feature = "_settings-portal")]
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

// Stored as a boolean:
// <https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.PowerProfileMonitor.html>
#[cfg(feature = "power-saver")]
fn parse_power_saver_enabled(value: Value) -> PowerSaver {
    match bool::try_from(value) {
        Ok(true) => PowerSaver::Enabled,
        Ok(false) | Err(_) => PowerSaver::NoPreference,
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

#[cfg(all(test, feature = "power-saver"))]
mod tests {
    use super::*;

    fn properties_changed(interface: &str, changed: &[(&str, Value<'_>)]) -> Message {
        let changed: HashMap<&str, &Value<'_>> =
            changed.iter().map(|(name, value)| (*name, value)).collect();
        let invalidated: Vec<&str> = Vec::new();
        Message::signal(PORTAL_PATH, PROPERTIES, "PropertiesChanged")
            .expect("valid header")
            .build(&(interface, changed, invalidated))
            .expect("valid body")
    }

    fn apply(interest: Interest, message: &Message) -> PowerSaver {
        let mut preferences = AvailablePreferences {
            power_saver: PowerSaver::Enabled,
            ..Default::default()
        };
        apply_power_profile_change(interest, &mut preferences, message).expect("valid signal");
        preferences.power_saver
    }

    #[test]
    fn parses_power_saver_enabled() {
        assert_eq!(PowerSaver::Enabled, parse_power_saver_enabled(Value::from(true)));
        assert_eq!(
            PowerSaver::NoPreference,
            parse_power_saver_enabled(Value::from(false))
        );
        assert_eq!(
            PowerSaver::NoPreference,
            parse_power_saver_enabled(Value::from(1u32)),
            "a value of the wrong type is no preference"
        );
    }

    #[test]
    fn applies_a_change_of_power_saver_enabled() {
        let disabled = properties_changed(
            POWER_PROFILE_MONITOR,
            &[(POWER_SAVER_ENABLED, Value::from(false))],
        );
        assert_eq!(PowerSaver::NoPreference, apply(Interest::PowerSaver, &disabled));
    }

    #[test]
    fn keeps_the_value_when_the_change_is_not_about_it() {
        let other_property =
            properties_changed(POWER_PROFILE_MONITOR, &[("version", Value::from(1u32))]);
        let other_interface = properties_changed(
            "org.freedesktop.portal.Other",
            &[(POWER_SAVER_ENABLED, Value::from(false))],
        );
        for message in [other_property, other_interface] {
            assert_eq!(PowerSaver::Enabled, apply(Interest::PowerSaver, &message));
        }
    }

    #[cfg(feature = "color-scheme")]
    #[test]
    fn keeps_the_value_without_interest() {
        let disabled = properties_changed(
            POWER_PROFILE_MONITOR,
            &[(POWER_SAVER_ENABLED, Value::from(false))],
        );
        assert_eq!(PowerSaver::Enabled, apply(Interest::ColorScheme, &disabled));
    }

    #[test]
    fn rejects_a_malformed_signal() {
        let message = Message::signal(PORTAL_PATH, PROPERTIES, "PropertiesChanged")
            .expect("valid header")
            .build(&(POWER_PROFILE_MONITOR,))
            .expect("valid body");
        let mut preferences = AvailablePreferences::default();
        assert!(apply_power_profile_change(Interest::PowerSaver, &mut preferences, &message).is_err());
    }
}
