use std::ops::BitOr;

/// Interest used when creating a [stream] or a [subscription].
/// They indicate which preferences should be retrieved and monitored.
///
/// [stream]: `crate::Preferences::stream`
/// [subscription]: `crate::Preferences::subscribe`
#[derive(Debug, Default, Clone, Copy)]
pub struct Interest(u8);

macro_rules! impl_interest {
    (impl $name:ident { $(#[cfg($cfg:meta)] $(#[$($meta:meta)*])* $vis:vis const $ident:ident: $ty:ty = $expr:expr;)* }) => {
        #[allow(non_upper_case_globals)]
        impl $name {
            /// A shortcut for all available preferences.
            ///
            /// Note that only the preferences that have their
            /// corresponding feature enabled will actually be reported.
            pub const All: $name = {
                #[allow(unused_mut)]
                let mut value = 0;
                $(
                    #[cfg($cfg)]
                    {
                        value |= Self::$ident.0;
                    }
                )*
                $name(value)
            };

            $(#[cfg($cfg)] $(#[$($meta)*])* $vis const $ident: $ty = $expr;)*
        }
    };
}

impl_interest! {
    impl Interest {
        #[cfg(feature = "color-scheme")]
        /// Retrieve the [`ColorScheme`](`crate::ColorScheme`) preference
        /// and store it in [`Preferences::color_scheme`](`crate::Preferences::color_scheme`).
        pub const ColorScheme: Interest = Interest(1 << 0);

        #[cfg(feature = "contrast")]
        /// Retrieve the [`Contrast`](`crate::Contrast`) preference
        /// and store it in [`Preferences::contrast`](`crate::Preferences::contrast`).
        pub const Contrast: Interest = Interest(1 << 1);

        #[cfg(feature = "reduced-motion")]
        /// Retrieve the [`ReducedMotion`](`crate::ReducedMotion`) preference
        /// and store it in [`Preferences::reduced_motion`](`crate::Preferences::reduced_motion`).
        pub const ReducedMotion: Interest = Interest(1 << 2);

        #[cfg(feature = "reduced-transparency")]
        /// Retrieve the [`ReducedTransparency`](`crate::ReducedTransparency`) preference
        /// and store it in [`Preferences::reduced_transparency`](`crate::Preferences::reduced_transparency`).
        pub const ReducedTransparency: Interest = Interest(1 << 3);

        #[cfg(feature = "accent-color")]
        /// Retrieve the [`AccentColor`](`crate::AccentColor`) preference
        /// and store it in [`Preferences::accent_color`](`crate::Preferences::accent_color`).
        pub const AccentColor: Interest = Interest(1 << 4);

        #[cfg(feature = "double-click-interval")]
        /// Retrieve the [`DoubleClickInterval`](`crate::DoubleClickInterval`) preference
        /// and store it in [`Preferences::double_click_interval`](`crate::Preferences::double_click_interval`).
        pub const DoubleClickInterval: Interest = Interest(1 << 5);

        #[cfg(feature = "scrollbar-visibility")]
        /// Retrieve the [`ScrollbarVisibility`](`crate::ScrollbarVisibility`) preference
        /// and store it in [`Preferences::scrollbar_visibility`](`crate::Preferences::scrollbar_visibility`).
        pub const ScrollbarVisibility: Interest = Interest(1 << 6);
    }
}

#[cfg(target_os = "macos")]
#[allow(non_upper_case_globals)]
impl Interest {
    #[cfg(feature = "_macos-accessibility")]
    pub(crate) const MacOSAccessibility: Interest = {
        let mut value = 0;
        #[cfg(feature = "contrast")]
        {
            value |= Interest::Contrast.0;
        }
        #[cfg(feature = "reduced-motion")]
        {
            value |= Interest::ReducedMotion.0;
        }
        #[cfg(feature = "reduced-transparency")]
        {
            value |= Interest::ReducedTransparency.0;
        }
        Interest(value)
    };
}

#[cfg(target_os = "linux")]
#[allow(non_upper_case_globals)]
impl Interest {
    #[cfg(feature = "_gnome_only")]
    pub(crate) const GnomeOnly: Interest = {
        let mut value = 0;
        #[cfg(feature = "reduced-motion")]
        {
            value |= Interest::ReducedMotion.0;
        }
        #[cfg(feature = "double-click-interval")]
        {
            value |= Interest::DoubleClickInterval.0;
        }
        #[cfg(feature = "scrollbar-visibility")]
        {
            value |= Interest::ScrollbarVisibility.0;
        }
        Interest(value)
    };
}

impl Interest {
    /// Tests if `self` is a superset of `other`.
    /// In other words: Tests if all flags set in `other` are set in `self.
    pub fn is(self, other: Self) -> bool {
        (other.0 & self.0) == other.0
    }

    /// Tests if `self` is empty.
    /// In other words: Tests if no flags are set.
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl BitOr for Interest {
    type Output = Interest;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}
