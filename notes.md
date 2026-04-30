## Possible extensions:
* Power Saver Mode:
  * Linux: PowerProfileMonitor portal (power-saver-enabled)
  * Windows: PowerManager.EnergySaverStatus (WinRT)
  * macOS: lowPowerModeEnabled <https://developer.apple.com/documentation/foundation/nsprocessinfo/1617047-lowpowermodeenabled>
* Forced colors / System Colors
* metered connection / prefers-reduced-data
  * Linux: NetworkMonitor portal
  * Windows: https://learn.microsoft.com/en-us/previous-versions/windows/apps/hh452990(v=win.10)
  * macOS: ?
  * web: prefers-reduced-data (not yet implemented anywhere)
* Text scaling factor
  * GNOME: org.gnome.desktop.interface text-scaling-factor
  * Windows: UISettings.TextScaleFactor
  * macOS: n/a
  * Web: n/a

## Prior Art
### libadwaita
* <https://gitlab.gnome.org/GNOME/libadwaita/-/blob/main/src/adw-settings-impl-macos.c>
* <https://gitlab.gnome.org/GNOME/libadwaita/-/blob/main/src/adw-settings-impl-portal.c>
* <https://gitlab.gnome.org/GNOME/libadwaita/-/blob/main/src/adw-settings-impl-win32.c>

### gecko
* <https://github.com/mozilla/gecko-dev/blob/71aada9d4055e420f91f3d0fa107f0328763e40b/widget/windows/nsLookAndFeel.cpp>
* <https://github.com/mozilla/gecko-dev/blob/71aada9d4055e420f91f3d0fa107f0328763e40b/widget/cocoa/nsLookAndFeel.mm>
* <https://github.com/mozilla/gecko-dev/blob/71aada9d4055e420f91f3d0fa107f0328763e40b/widget/gtk/nsLookAndFeel.cpp>

### Chromium
* <https://source.chromium.org/chromium/chromium/src/+/main:ui/gfx/animation/animation_win.cc;l=37;drc=71bd8f1596b301a842247d3488f901d7ae3dfad2;bpv=0;bpt=0>
