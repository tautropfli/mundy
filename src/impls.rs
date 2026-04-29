macro_rules! impls {
    ($(
        #[cfg($($meta:meta)*)]
        mod $mod:ident supports { $($feature:literal $setting:ident),* $(,)? };
    )*) => {
        cfg_if::cfg_if! {
            if #[cfg(any())] { }
            $(
                else if #[cfg(all($($meta)*, any($(feature = $feature),*)))]
                {
                    #[cfg_attr(test, allow(unused_imports))]
                    mod cfg { pub(crate) use enabled as any_feature; }

                    mod $mod;
                    use $mod as imp;
                    impls!(@struct { $($feature $setting),* });
                }
            )*
            else {
                #[cfg_attr(test, allow(unused_imports))]
                mod cfg { pub(crate) use disabled as any_feature; }

                mod fallback;
                use fallback as imp;
                impls!(@struct { });
            }
        }
    };
    (@type color_scheme) => { ColorScheme };
    (@type contrast) => { Contrast };
    (@type reduced_motion) => { ReducedMotion };
    (@type reduced_transparency) => { ReducedTransparency };
    (@type accent_color) => { AccentColor };
    (@type double_click_interval) => { DoubleClickInterval };
    (@type scrollbar_visibility) => { ScrollbarVisibility };
    (@struct { $($feature:literal $setting:ident),* }) => {
        #[derive(Debug, Default, Clone, Copy, PartialEq)]
        pub(crate) struct AvailablePreferences {
            $(
                #[cfg(feature = $feature)]
                $setting : impls!(@type $setting),
            )*
        }

        impl From<AvailablePreferences> for Preferences {
            fn from(_value: AvailablePreferences) -> Preferences {
                Preferences {
                    $(
                        #[cfg(feature = $feature)]
                        $setting: _value.$setting,
                    )*
                    ..Default::default()
                }
            }
        }
    }
}

// This trick was shamelessly stolen from bevy_platform:
// <https://github.com/bevyengine/bevy/blob/main/crates/bevy_platform/src/cfg.rs>

/// This macro passes the provided code because at least one supported preference feature is currently active.
#[allow(unused_macros)]
macro_rules! disabled {
    () => { false };
    (if { $($p:tt)* } else { $($n:tt)* }) => { $($n)* };
    ($($p:tt)*) => {};
}

/// This macro suppresses the provided code because no supported preference feature is currently active.
#[allow(unused_macros)]
macro_rules! enabled {
    () => { true };
    (if { $($p:tt)* } else { $($n:tt)* }) => { $($p)* };
    ($($p:tt)*) => { $($p)* };
}
