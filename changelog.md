# Changelog
## 0.2.3
* Fixed a segfault on Windows.
* Added a new [`ScrollbarVisibility`](https://docs.rs/mundy/0.2.3/mundy/struct.ScrollbarVisibility.html) preference.
* Added support for Ubuntu's legacy accent colors (still used in 24.04 LTS).
* Updated the `jni` dependency to `0.22`.

## 0.2.2
* Added support for the new `reduced-motion` preference from the [XDG Settings portal](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Settings.html).

## 0.2.1
* Updated `windows` to 0.62

## 0.2.0
* Added support for Android.
* The MSRV was previously not explicitly defined.
  It is now set to `1.80.0`.
* Removes conversions from `Srgba` to bevy and epaint color types.
* Add convenience methods on the preference enums
  for easy testing such as `is_dark`, `is_light`, etc.

## 0.1.10
* Add `Preferences::once_blocking` - an easy way to retrieve the preferences once. #5
* Update `epaint` from 0.31 to 0.32

## 0.1.9
* Update `bevy_color` to 0.16

## 0.1.8
* ⬆️ Update `windows` to 0.61

## 0.1.7
* ⬆️  Update `windows` to 0.60
* ⬆️  Update `epaint` to 0.31

## 0.1.6
* ⬆️  Update `objc2` to 0.6

## 0.1.5
* ⬆️  Update `epaint` to 0.30

## 0.1.4
* ✨ Add support for reading the double click interval preference.

## 0.1.3
* ⬆️  Update `bevy_color` to 0.15

## 0.1.2
* ⬆️  Update `zbus` from 4.4 to 5.0
* Replace `futures-util` with `futures-lite`

## 0.1.1
Initial release
