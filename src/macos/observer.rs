#[cfg(feature = "accent-color")]
use super::get_accent_color;
#[cfg(feature = "contrast")]
use super::get_contrast;
#[cfg(feature = "reduced-motion")]
use super::get_reduced_motion;
#[cfg(feature = "reduced-transparency")]
use super::get_reduced_transparency;
#[cfg(feature = "scrollbar-visibility")]
use super::get_scrollbar_visibility;
#[cfg(feature = "color-scheme")]
use super::main_thread::run_on_main_async;
#[cfg(feature = "_macos-accessibility")]
use super::preference::AccessibilityPreferences;
use super::preference::Preference;
#[cfg(feature = "color-scheme")]
use super::to_color_scheme;
use crate::Interest;
use futures_channel::mpsc;
use objc2::rc::Retained;
#[cfg(feature = "color-scheme")]
use objc2::runtime::AnyObject;
#[cfg(any(feature = "accent-color", feature = "_macos-accessibility"))]
use objc2::sel;
use objc2::{define_class, msg_send, AllocAnyThread as _, DeclaredClass};
#[cfg(feature = "color-scheme")]
use objc2_app_kit::NSAppearance;
use objc2_app_kit::NSApplication;
#[cfg(feature = "scrollbar-visibility")]
use objc2_app_kit::NSPreferredScrollerStyleDidChangeNotification;
#[cfg(feature = "accent-color")]
use objc2_app_kit::NSSystemColorsDidChangeNotification;
#[cfg(feature = "_macos-accessibility")]
use objc2_app_kit::NSWorkspace;
#[cfg(feature = "_macos-accessibility")]
use objc2_app_kit::NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification;
#[cfg(feature = "accent-color")]
use objc2_foundation::NSNotificationCenter;
use objc2_foundation::NSObject;
#[cfg(feature = "color-scheme")]
use objc2_foundation::{
    ns_string, NSDictionary, NSKeyValueChangeKey, NSKeyValueChangeNewKey,
    NSKeyValueObservingOptions, NSObjectNSKeyValueObserverRegistration as _, NSString,
};
#[cfg(feature = "color-scheme")]
use std::ffi::c_void;
#[cfg(feature = "color-scheme")]
use std::ptr;

pub(crate) struct ObserverRegistration {
    observer: Retained<Observer>,
    interest: Interest,
}

#[cfg(feature = "color-scheme")]
fn effective_appearance_key() -> &'static NSString {
    ns_string!("effectiveAppearance")
}

impl Observer {
    pub(crate) fn register(
        #[cfg_attr(not(feature = "color-scheme"), expect(unused_variables))]
        application: &NSApplication,
        sender: mpsc::UnboundedSender<Preference>,
        interest: Interest,
    ) -> ObserverRegistration {
        let observer = Self::new(sender);

        #[cfg(feature = "color-scheme")]
        if interest.is(Interest::ColorScheme) {
            // SAFETY: The observer is removed on drop.
            unsafe {
                application.addObserver_forKeyPath_options_context(
                    &observer,
                    effective_appearance_key(),
                    NSKeyValueObservingOptions::New | NSKeyValueObservingOptions::Old,
                    ptr::null_mut(),
                );
            }
        }

        #[cfg(feature = "_macos-accessibility")]
        if interest.is(Interest::MacOSAccessibility) {
            let workspace = NSWorkspace::sharedWorkspace();
            let notification_center = workspace.notificationCenter();
            // SAFETY: The observer is removed on drop.
            unsafe {
                notification_center.addObserver_selector_name_object(
                    &observer,
                    sel!(accessibilityDisplayOptionsDidChange),
                    Some(NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification),
                    None,
                );
            }
        }

        #[cfg(feature = "accent-color")]
        if interest.is(Interest::AccentColor) {
            // SAFETY: The observer is removed on drop.
            unsafe {
                // We're reacting to `NSSystemColorsDidChangeNotification` instead of the sometimes
                // used "AppleColorPreferencesChangedNotification" for two reasons:
                // * The former is officially documented while the latter is not.
                // * When reacting to the latter, `NSColor::controlAccentColor()` is sometimes not updated yet.
                let notification_center = NSNotificationCenter::defaultCenter();
                notification_center.addObserver_selector_name_object(
                    &observer,
                    sel!(systemColorsDidChange),
                    Some(NSSystemColorsDidChangeNotification),
                    None,
                );
            }
        }

        #[cfg(feature = "scrollbar-visibility")]
        if interest.is(Interest::ScrollbarVisibility) {
            // SAFETY: The observer is removed on drop.
            unsafe {
                let notification_center = NSNotificationCenter::defaultCenter();
                notification_center.addObserver_selector_name_object(
                    &observer,
                    sel!(scrollerStyleDidChange),
                    Some(NSPreferredScrollerStyleDidChangeNotification),
                    None,
                );
            }
        }

        ObserverRegistration { observer, interest }
    }

    fn new(sender: mpsc::UnboundedSender<Preference>) -> Retained<Observer> {
        let observer = Observer::alloc().set_ivars(Ivars { sender });
        // SAFETY: Our instance is allocated and the instance vars are set.
        unsafe { msg_send![super(observer), init] }
    }
}

impl Drop for ObserverRegistration {
    fn drop(&mut self) {
        #[cfg(feature = "color-scheme")]
        if self.interest.is(Interest::ColorScheme) {
            let observer = self.observer.clone();
            // Note that this leaks if there's no queue running on the main thread
            // (I don't think that there's anything we can do?)
            run_on_main_async(move |mtm| {
                let application = NSApplication::sharedApplication(mtm);
                unsafe {
                    application.removeObserver_forKeyPath(&observer, effective_appearance_key());
                }
            });
        }

        #[cfg(feature = "_macos-accessibility")]
        if self.interest.is(Interest::MacOSAccessibility) {
            let workspace = NSWorkspace::sharedWorkspace();
            let notification_center = workspace.notificationCenter();
            unsafe {
                notification_center.removeObserver(&self.observer);
            }
        }

        #[cfg(feature = "accent-color")]
        if self.interest.is(Interest::AccentColor) {
            unsafe {
                let notification_center = NSNotificationCenter::defaultCenter();
                notification_center.removeObserver(&self.observer);
            }
        }

        #[cfg(feature = "scrollbar-visibility")]
        if self.interest.is(Interest::ScrollbarVisibility) {
            unsafe {
                let notification_center = NSNotificationCenter::defaultCenter();
                notification_center.removeObserver(&self.observer);
            }
        }
    }
}

define_class! {
    // SAFETY:
    // - The superclass NSObject does not have any subclassing requirements.
    // - `MyCustomObject` does not implement `Drop`.
    #[unsafe(super(NSObject))]
    #[name = "MundyObserver"]
    #[ivars = Ivars]
    pub(crate) struct Observer;

    impl Observer {
        #[cfg(feature = "accent-color")]
        #[unsafe(method(systemColorsDidChange))]
        fn system_colors_did_change(&self) {
            _ = self.ivars().sender.unbounded_send(Preference::AccentColor(get_accent_color()));
        }

        #[cfg(feature = "scrollbar-visibility")]
        #[unsafe(method(scrollerStyleDidChange))]
        fn scroller_style_did_change(&self) {
            let sender = self.ivars().sender.clone();
            run_on_main_async(move |mtm| {
                _ = sender.unbounded_send(Preference::ScrollbarVisibility(get_scrollbar_visibility(mtm)));
            });
        }

        #[cfg(feature = "_macos-accessibility")]
        #[unsafe(method(accessibilityDisplayOptionsDidChange))]
        fn accessibility_options_did_change(&self) {
            let mut prefs = AccessibilityPreferences::default();
            #[cfg(feature = "contrast")]
            {
                prefs.contrast = get_contrast();
            }
            #[cfg(feature = "reduced-motion")]
            {
                prefs.reduced_motion = get_reduced_motion();
            }
            #[cfg(feature = "reduced-transparency")]
            {
                prefs.reduced_transparency = get_reduced_transparency();
            }
            _ = self.ivars().sender.unbounded_send(Preference::Accessibility(prefs));
        }

        #[cfg(feature = "color-scheme")]
        #[unsafe(method(observeValueForKeyPath:ofObject:change:context:))]
        fn observe_value(
            &self,
            key_path: Option<&NSString>,
            _object: Option<&AnyObject>,
            change: Option<&NSDictionary<NSKeyValueChangeKey, AnyObject>>,
            _context: *mut c_void,
        )
        {
            if key_path == Some(effective_appearance_key()) {
                let change = change.expect("requested a change dictionary in `addObserver`, but none was provided");
                let new = change.objectForKey(unsafe { NSKeyValueChangeNewKey }).expect("requested change dictionary did not contain `NSKeyValueChangeNewKey`");
                let new: &NSAppearance = new.downcast_ref().expect("effectiveAppearance is NSAppearance");
                _ = self.ivars().sender.unbounded_send(Preference::ColorScheme(to_color_scheme(new)));
            }
        }
    }
}

#[derive(Clone)]
pub(crate) struct Ivars {
    sender: mpsc::UnboundedSender<Preference>,
}

#[cfg(test)]
static_assertions::assert_impl_all!(Ivars: Send, Sync);
