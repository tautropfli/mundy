#![cfg_attr(docsrs, feature(doc_cfg))]
#![forbid(
    clippy::dbg_macro,
    clippy::missing_safety_doc,
    clippy::unnecessary_safety_comment,
    clippy::unnecessary_safety_doc,
    unsafe_op_in_unsafe_fn
)]
#![deny(clippy::unwrap_used)]

//! Your friendly neighbourhood ~~whale~~ crate for reading various system-level
//! accessibility and UI preferences across platforms 🐋
//!
//! The following preferences are supported:
//! * [`AccentColor`]—The user's current system wide accent color preference.
//! * [`ColorScheme`]—The user's preference for either light or dark mode.
//! * [`Contrast`]—The user's preferred contrast level.
//! * [`ReducedMotion`]—The user's reduced motion preference.
//! * [`ReducedTransparency`]—The user's reduced transparency preference.
//! * [`DoubleClickInterval`]—The maximum amount of time allowed between the first and second click.
//! * [`ScrollbarVisibility`]—Whether the scrollbar should be always visible or auto-hide.
//!
//! Note that each preference has a corresponding [feature flag](`feature_flags`).
//! By turning off [default features](https://doc.rust-lang.org/cargo/reference/features.html#the-default-feature)
//! you will only "pay" for what you actually need.
//!
//! ## Example
//! The easiest way to get the preferences is to use the
//! [`Preferences::stream`] function to create a stream that is continually
//! updated when things change:
//!
//! ```no_run
//! use mundy::{Preferences, Interest};
//! use futures_lite::StreamExt as _;
//!
//! // Interest tells mundy which preferences it should monitor for you.
//! // use `Interest::All` if you're interested in all preferences.
//! let mut stream = Preferences::stream(Interest::AccentColor);
//! # let _ = async move {
//! while let Some(preferences) = stream.next().await {
//!     eprintln!("accent color: {:?}", preferences.accent_color);
//! }
//! # };
//! ```
//!
//! Alternatively, there's [`Preferences::subscribe`] which
//! accepts a simple callback function instead.
//!
//! ## Errors
//! Most errors (except some fatal errors at startup) are simply ignored
//! and the default value for the preference (which is usually `NoPreference`) is returned.
//! It can be useful to turn on the `log` feature to find out what's going on.
//!
//! <br>
//!
//! <small>«*I believe in a universe that doesn't care and people
//! who do. [...] but this whale is pretty cool.* ― Angus</small>

use futures_lite::Stream;
use pin_project_lite::pin_project;
use std::time::Duration;
use stream_utils::Dedup;

#[macro_use]
mod impls;
mod interest;
pub use interest::*;
mod async_rt;
#[cfg(feature = "callback")]
mod callback;
#[cfg(feature = "callback")]
pub use callback::*;
#[cfg(feature = "accent-color")]
mod color;
#[cfg(feature = "accent-color")]
pub use color::*;

#[cfg(not(test))]
cfg::any_feature! {
    #[cfg(any(
        target_os = "android",
        target_os = "windows"
    ))]
    mod callback_utils;
}
#[cfg(test)]
mod callback_utils;

mod stream_utils;

/// Contains platform-specific functionality.
pub mod platform {
    /// On Android, mundy requires access to the JVM and the current [`Context`].
    /// To access these objects, mundy uses the [`ndk-context`] crate.
    ///
    /// Before calling any of mundy's functions, you need to make sure that the [`ndk-context`]
    /// is initialized. If you are writing an Android app using pure Rust using the [`android-activity`]
    /// or [`winit`] crates, then this is already done for you.
    ///
    /// If you want to listen to changes to the [`ColorScheme`], then you will also need
    /// to call the [`crate::platform::android::on_configuration_changed`] function as needed.
    ///
    /// [`Context`]: https://developer.android.com/reference/android/content/Context
    /// [`ndk-context`]: https://docs.rs/ndk-context
    /// [`android-activity`]: https://docs.rs/android-activity
    /// [`winit`]: https://docs.rs/winit
    /// [`ColorScheme`]: `crate::ColorScheme`
    #[cfg(any(doc, target_os = "android"))]
    #[cfg_attr(docsrs, doc(cfg(target_os = "android")))]
    pub mod android {
        /// When certain preferences such as the [`ColorScheme`](`crate::ColorScheme`) change,
        /// Android calls the `onConfigurationChanged` method on your [`View`] or [`Activity`].
        /// Since there is no way for mundy to override these methods itself,
        /// you will need to override `onConfigurationChanged` and call this function.
        ///
        /// To avoid your activity being re-created, you will also need to add the `android:configChanges`
        /// attribute to your `AndroidManifest.xml`:
        /// ```xml
        /// <activity
        ///    android:name=".MyActivity"
        ///    android:configChanges="uiMode" />
        /// ```
        ///
        /// This tells the OS that you handle the configuration change yourself.
        ///
        /// If you use a tool like [xbuild], you can also do this in your Cargo.toml:
        /// ```toml
        /// [[package.metadata.android.application.activities]]
        /// name = "android.app.NativeActivity"
        /// configChanges = "uiMode"
        /// ```
        ///
        /// For more details, check out the documentation on handling [runtime changes]
        /// and implementing [dark mode].
        ///
        /// [`View`]: https://developer.android.com/reference/kotlin/android/view/View#onconfigurationchanged
        /// [`Activity`]: https://developer.android.com/reference/android/app/Activity#onConfigurationChanged(android.content.res.Configuration)
        /// [xbuild]: https://github.com/rust-mobile/xbuild
        /// [runtime changes]: https://developer.android.com/guide/topics/resources/runtime-changes
        /// [dark mode]: https://developer.android.com/develop/ui/views/theming/darktheme#config-changes
        pub fn on_configuration_changed() {
            #[cfg(target_os = "android")]
            crate::cfg::any_feature! {
                crate::imp::on_configuration_changed();
            }
        }
    }
}

/// # Feature Flags
///
/// * `log`—Enable logging.
/// * `callback`—Enable the synchronous [`Preferences::subscribe`] function (*default*).
/// * `color-scheme`—Enable support for [`ColorScheme`] (*default*).
/// * `contrast`—Enable support for [`Contrast`] (*default*).
/// * `reduced-motion`—Enable support for [`ReducedMotion`] (*default*).
/// * `reduced-transparency`—Enable support for [`ReducedTransparency`] (*default*).
/// * `accent-color`—Enable support for [`AccentColor`] (*default*).
/// * `double-click-interval`—Enable support for [`DoubleClickInterval`] (*default*).
/// * `scrollbar-visibility`—Enable support for [`ScrollbarVisibility`] (*default*).
/// * (Linux) `async-io`—Use `zbus` with `async-io` (*default*).
/// * (Linux) `tokio`—Use `zbus` with `tokio` instead of `async-io`.
///
/// ## Turning Off Default Features
///
/// If you turn off [default features](https://doc.rust-lang.org/cargo/reference/features.html#the-default-feature),
/// you will have to pick between one of the two available async runtimes `async-io` and `tokio` to enable.
#[cfg(doc)]
#[cfg_attr(docsrs, doc(cfg(doc)))]
pub mod feature_flags {}

#[cfg(doctest)]
#[doc = include_str!("../readme.md")]
pub mod readme_doctest {}

/// A collection of preferences retrieved by calling either
/// [`Preferences::stream`] or [`Preferences::subscribe`].
///
/// Which fields are filled in is determined by the [`Interest`]
/// you provide when creating a stream or subscription.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct Preferences {
    /// The user's preference for either light or dark mode.
    #[cfg(feature = "color-scheme")]
    pub color_scheme: ColorScheme,
    /// The user's preferred contrast level.
    #[cfg(feature = "contrast")]
    pub contrast: Contrast,
    /// The user's reduced motion preference.
    #[cfg(feature = "reduced-motion")]
    pub reduced_motion: ReducedMotion,
    /// The user's reduced transparency preference.
    #[cfg(feature = "reduced-transparency")]
    pub reduced_transparency: ReducedTransparency,
    /// The user's current system wide accent color preference.
    #[cfg(feature = "accent-color")]
    pub accent_color: AccentColor,
    /// The maximum amount of time that may occur between the first and second click
    /// event for it to count as double click.
    #[cfg(feature = "double-click-interval")]
    pub double_click_interval: DoubleClickInterval,
    /// Whether the scrollbar should be always visible or auto-hide (overlay).
    #[cfg(feature = "scrollbar-visibility")]
    pub scrollbar_visibility: ScrollbarVisibility,
}

impl Preferences {
    /// Creates a new stream for a selection of system preferences given by `interests`.
    /// Should be called from the main thread.
    ///
    /// The stream is guaranteed to contain at least one item with the initial preferences.
    ///
    /// You can use [`Preferences::subscribe`] if you don't want to manage
    /// spawning an async runtime yourself.
    ///
    #[doc = include_str!("doc/caveats.md")]
    pub fn stream(interest: Interest) -> PreferencesStream {
        let inner = if interest.is_empty() {
            imp::default_stream()
        } else {
            imp::stream(interest)
        };
        PreferencesStream {
            inner: Dedup::new(inner),
        }
    }

    /// Retrieves a selection of system preferences given by `interests`.
    /// Should be called from the main thread.
    ///
    /// You should generally prefer [`Preferences::stream()`] or [`Preferences::subscribe()`] as
    /// they provide updates when the user changes the preferences.
    ///
    /// Returns [`None`] if the preferences cannot be retrieved within the given timeout.
    ///
    #[doc = include_str!("doc/caveats.md")]
    pub fn once_blocking(interest: Interest, timeout: Duration) -> Option<Self> {
        if interest.is_empty() {
            return Some(Default::default());
        }
        imp::once_blocking(interest, timeout).map(Self::from)
    }

    /// Creates a new subscription for a selection of system preferences given by `interests`.
    ///
    /// The provided callback is guaranteed to be called at least once with the initial values
    /// and is subsequently called when preferences are updated.
    ///
    #[doc = include_str!("doc/caveats.md")]
    #[cfg(feature = "callback")]
    pub fn subscribe(interest: Interest, callback: impl CallbackFn) -> Subscription {
        Preferences::subscribe_impl(interest, callback)
    }
}

pin_project! {
    /// A stream that continually yields preferences
    /// whenever they are changed. Created by [`Preferences::stream()`].
    pub struct PreferencesStream {
        #[pin] inner: Dedup<imp::PreferencesStream>,
    }
}

#[cfg(test)]
static_assertions::assert_impl_all!(PreferencesStream: Send);

impl Stream for PreferencesStream {
    type Item = Preferences;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let this = self.project();
        this.inner.poll_next(cx).map(|o| o.map(Preferences::from))
    }
}

impls! {
    #[cfg(target_os = "linux")]
    mod freedesktop supports {
        "color-scheme" color_scheme,
        "contrast" contrast,
        "reduced-motion" reduced_motion,
        "accent-color" accent_color,
        "double-click-interval" double_click_interval,
        "scrollbar-visibility" scrollbar_visibility,
    };

    #[cfg(windows)]
    mod windows supports {
        "color-scheme" color_scheme,
        "contrast" contrast,
        "reduced-motion" reduced_motion,
        "accent-color" accent_color,
        "reduced-transparency" reduced_transparency,
        "double-click-interval" double_click_interval,
        "scrollbar-visibility" scrollbar_visibility,
    };

    #[cfg(target_os = "macos")]
    mod macos supports {
        "color-scheme" color_scheme,
        "contrast" contrast,
        "reduced-motion" reduced_motion,
        "reduced-transparency" reduced_transparency,
        "accent-color" accent_color,
        "double-click-interval" double_click_interval,
        "scrollbar-visibility" scrollbar_visibility,
    };

    #[cfg(all(target_family = "wasm", target_os = "unknown"))]
    mod web supports {
        "color-scheme" color_scheme,
        "contrast" contrast,
        "reduced-motion" reduced_motion,
        "accent-color" accent_color,
        "reduced-transparency" reduced_transparency,
    };

    #[cfg(target_os = "android")]
    mod android supports {
        "color-scheme" color_scheme,
        "contrast" contrast,
        "reduced-motion" reduced_motion,
        "accent-color" accent_color,
    };
}

/// The user's preference for either light or dark mode. This corresponds to the [`prefers-color-scheme`] CSS media feature.
///
/// <details>
/// <summary style="cursor: pointer">
///
/// #### Platform-specific Sources
///
/// </summary>
///
/// * Linux: `org.freedesktop.appearance color-scheme` from the [XDG Settings portal][xdg].
/// * Windows: [`UISettings.GetColorValue(UIColorType::Foreground)`](https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/ui/apply-windows-themes#know-when-dark-mode-is-enabled)
/// * macOS: `NSApplication.effectiveAppearance`
/// * Web: `@media (prefers-color-scheme: ...)`
/// * Android: [`Configuration.uiMode`]
///
/// </details>
///
/// [`prefers-color-scheme`]: https://developer.mozilla.org/en-US/docs/Web/CSS/@media/prefers-color-scheme
/// [xdg]: https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.impl.portal.Settings.html
/// [`Configuration.uiMode`]: https://developer.android.com/reference/android/content/res/Configuration#uiMode
#[derive(Debug, Default, Clone, Copy, PartialEq)]
#[cfg(feature = "color-scheme")]
pub enum ColorScheme {
    /// Indicates that the user has not expressed an active preference,
    /// that the current platform doesn't support a color scheme preference
    /// or that an error occurred while trying to retrieve the preference.
    #[default]
    NoPreference,
    /// Indicates that the user prefers an interface with a light appearance.
    Light,
    /// Indicates that the user prefers an interface with a dark appearance.
    Dark,
}

#[cfg(feature = "color-scheme")]
impl ColorScheme {
    pub fn is_no_preference(self) -> bool {
        matches!(self, ColorScheme::NoPreference)
    }

    pub fn is_dark(self) -> bool {
        matches!(self, ColorScheme::Dark)
    }

    pub fn is_light(self) -> bool {
        matches!(self, ColorScheme::Light)
    }
}

/// The user's preferred contrast level. This corresponds to the [`prefers-contrast`] CSS media feature.
///
/// <details>
/// <summary style="cursor: pointer">
///
/// #### Platform-specific Sources
///
/// </summary>
///
/// * Linux: `org.freedesktop.appearance contrast` from the [XDG Settings portal][xdg].
/// * Windows: [`AccessibilitySettings.HighContrast`](https://learn.microsoft.com/en-us/uwp/api/windows.ui.viewmanagement.accessibilitysettings.highcontrast)
/// * macOS: [`accessibilityDisplayShouldIncreaseContrast`](https://developer.apple.com/documentation/appkit/nsworkspace/1526290-accessibilitydisplayshouldincrea)
/// * Web: `@media (prefers-contrast: ...)`
/// * Android: `Settings.Secure.ACCESSIBILITY_HIGH_TEXT_CONTRAST_ENABLED` and [`UiModeManager.getContrast`]
///
/// </details>
///
/// [xdg]: https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.impl.portal.Settings.html
/// [`UiModeManager.getContrast`]: https://developer.android.com/reference/android/app/UiModeManager#getContrast()
/// [`prefers-contrast`]: https://developer.mozilla.org/en-US/docs/Web/CSS/@media/prefers-contrast
#[derive(Debug, Default, Clone, Copy, PartialEq)]
#[cfg(feature = "contrast")]
pub enum Contrast {
    /// Indicates that the user has not expressed an active preference,
    /// that the current platform doesn't support a contrast preference
    /// or that an error occurred while trying to retrieve the preference.
    #[default]
    NoPreference,
    /// Indicates that the user prefers an interface with a higher level of contrast.
    More,
    /// Indicates that the user prefers an interface with a lower level of contrast.
    Less,
    /// Indicates that the user has configured a specific set of colors (forced color mode)
    /// and the contrast from these colors neither matches [`Contrast::More`] or [`Contrast::Less`].
    Custom,
}

#[cfg(feature = "contrast")]
impl Contrast {
    pub fn is_no_preference(self) -> bool {
        matches!(self, Contrast::NoPreference)
    }

    pub fn is_more(self) -> bool {
        matches!(self, Contrast::More)
    }

    pub fn is_less(self) -> bool {
        matches!(self, Contrast::Less)
    }

    pub fn is_custom(self) -> bool {
        matches!(self, Contrast::Custom)
    }
}

/// The user prefers to have a minimal amount of motion. Especially motion that simulates the third dimension.
/// This corresponds to the [`prefers-reduced-motion`] CSS media feature.
///
/// Such motion can cause discomfort to people with [vestibular disorders](https://www.a11yproject.com/posts/understanding-vestibular-disorders/).
///
/// <details>
/// <summary style="cursor: pointer">
///
/// #### Platform-specific Sources
///
/// </summary>
///
/// * Linux: `org.freedesktop.appearance reduced-motion` from the [XDG Settings portal][xdg].
/// * Windows: [`UISettings.AnimationsEnabled`](https://learn.microsoft.com/en-us/uwp/api/windows.ui.viewmanagement.uisettings.animationsenabled)
/// * macOS: [`accessibilityDisplayShouldReduceMotion`](https://developer.apple.com/documentation/appkit/nsworkspace/1644069-accessibilitydisplayshouldreduce)
/// * Web: `@media (prefers-reduced-motion: ...)`
/// * Android: [`Settings.Global.ANIMATOR_DURATION_SCALE`]
///
/// </details>
///
/// [xdg]: https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.impl.portal.Settings.html
/// [`Settings.Global.ANIMATOR_DURATION_SCALE`]: https://developer.android.com/reference/android/provider/Settings.Global#ANIMATOR_DURATION_SCALE
/// [`prefers-reduced-motion`]: https://developer.mozilla.org/en-US/docs/Web/CSS/@media/prefers-reduced-motion
#[derive(Debug, Default, Clone, Copy, PartialEq)]
#[cfg(feature = "reduced-motion")]
pub enum ReducedMotion {
    /// Indicates that the user has not expressed an active preference,
    /// that the current platform doesn't support a reduced motion preference
    /// or that an error occurred while trying to retrieve the preference.
    #[default]
    NoPreference,
    /// Indicates that the user prefers a minimal amount of motion.
    Reduce,
}

#[cfg(feature = "reduced-motion")]
impl ReducedMotion {
    pub fn is_no_preference(self) -> bool {
        matches!(self, ReducedMotion::NoPreference)
    }

    pub fn is_reduce(self) -> bool {
        matches!(self, ReducedMotion::Reduce)
    }
}

/// Indicates that applications should not use transparent or semitransparent backgrounds.
/// This corresponds to the [`prefers-reduced-transparency`] CSS media feature.
///
/// <details>
/// <summary style="cursor: pointer">
///
/// #### Platform-specific Sources
///
/// </summary>
///
/// * Windows: [`UISettings.AdvancedEffectsEnabled`](https://learn.microsoft.com/en-us/uwp/api/windows.ui.viewmanagement.uisettings.advancedeffectsenabled)
/// * macOS: [`accessibilityDisplayShouldReduceTransparency`](https://developer.apple.com/documentation/appkit/nsworkspace/1533006-accessibilitydisplayshouldreduce)
/// * Web: `@media (prefers-reduced-transparency: ...)`
/// * Linux: Unsupported
/// * Android: Unsupported
///
/// </details>
///
/// [`prefers-reduced-transparency`]: https://developer.mozilla.org/en-US/docs/Web/CSS/@media/prefers-reduced-transparency
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg(feature = "reduced-transparency")]
pub enum ReducedTransparency {
    /// Indicates that the user has not expressed an active preference,
    /// that the current platform doesn't support a reduced transparency preference
    /// or that an error occurred while trying to retrieve the preference.
    #[default]
    NoPreference,
    /// Indicates that the user prefers an interface with no transparent
    /// or semitransparent backgrounds.
    Reduce,
}

#[cfg(feature = "reduced-transparency")]
impl ReducedTransparency {
    pub fn is_no_preference(self) -> bool {
        matches!(self, ReducedTransparency::NoPreference)
    }

    pub fn is_reduce(self) -> bool {
        matches!(self, ReducedTransparency::Reduce)
    }
}

/// Whether the scrollbar should be always visible or auto-hide (overlay). \
/// This preference may change depending on which input devices are connected.
///
/// <details>
/// <summary style="cursor: pointer">
///
/// #### Platform-specific Sources
///
/// </summary>
///
/// * Linux (GNOME-only): `org.gnome.desktop.interface overlay-scrolling` from the [XDG Settings portal][xdg].
/// * Windows: [`UISettings.AutoHideScrollBars`](https://learn.microsoft.com/en-us/uwp/api/windows.ui.viewmanagement.uisettings.autohidescrollbars)
/// * macOS: [`NSScroller.preferredScrollerStyle`](https://developer.apple.com/documentation/appkit/nsscroller/preferredscrollerstyle)
/// * Web: Unsupported
/// * Android: Unsupported
///
/// </details>
///
/// [xdg]: https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.impl.portal.Settings.html
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg(feature = "scrollbar-visibility")]
pub enum ScrollbarVisibility {
    /// Indicates that the user has not expressed an active preference,
    /// that the current platform doesn't support a scrollbar visibility preference
    /// or that an error occurred while trying to retrieve the preference.
    #[default]
    NoPreference,
    /// Automatically show the scrollbars while the user is scrolling
    /// and hide them otherwise. This is also known as «overlay scrolling».
    Auto,
    /// Always show the scrollbars.
    Always,
}

#[cfg(feature = "scrollbar-visibility")]
impl ScrollbarVisibility {
    pub fn is_no_preference(self) -> bool {
        matches!(self, ScrollbarVisibility::NoPreference)
    }

    pub fn is_auto(self) -> bool {
        matches!(self, ScrollbarVisibility::Auto)
    }

    pub fn is_always(self) -> bool {
        matches!(self, ScrollbarVisibility::Always)
    }
}

/// The user's current system wide accent color preference.
///
/// <details>
/// <summary style="cursor: pointer">
///
/// #### Platform-specific Sources
///
/// </summary>
///
/// * Linux: `org.freedesktop.appearance accent-color` from the [XDG Settings portal][xdg].
/// * Windows: [`UISettings.GetColorValue(UIColorType::Accent)`](https://learn.microsoft.com/en-us/uwp/api/windows.ui.viewmanagement.uisettings)
/// * macOS: [`NSColor.controlAccentColor`](https://developer.apple.com/documentation/appkit/nscolor/3000782-controlaccentcolor)
/// * Web: The [`AccentColor`](https://developer.mozilla.org/en-US/docs/Web/CSS/system-color#accentcolor) system color.
/// * Android: `android.R.attr.colorAccent`
///
/// </details>
///
/// [xdg]: https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.impl.portal.Settings.html
#[derive(Debug, Default, Clone, Copy, PartialEq)]
#[cfg(feature = "accent-color")]
pub struct AccentColor(pub Option<Srgba>);

/// The maximum amount of time that may occur between the first and second click
/// event for it to count as double click.
///
/// A typical value for this preference is ~500 ms.
///
/// <details>
/// <summary style="cursor: pointer">
///
/// #### Platform-specific Sources
///
/// </summary>
///
/// * Linux (GNOME-only): `org.gnome.desktop.peripherals.mouse double-click` from the [XDG Settings portal][xdg].
/// * Windows: [`GetDoubleClickTime`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getdoubleclicktime)
/// * macOS: [`NSEvent.doubleClickInterval`](https://developer.apple.com/documentation/appkit/nsevent/1528384-doubleclickinterval)
/// * Web: Unsupported
/// * Android: Unsupported
///
/// </details>
///
/// [xdg]: https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.impl.portal.Settings.html
#[derive(Debug, Default, Clone, Copy, PartialEq)]
#[cfg(feature = "double-click-interval")]
pub struct DoubleClickInterval(pub Option<std::time::Duration>);
