# mundy 🐋

[![Docs](https://img.shields.io/docsrs/mundy/latest)](https://docs.rs/mundy)
[![Crate Version](https://img.shields.io/crates/v/mundy)](https://crates.io/crates/mundy)

Your friendly neighbourhood ~~whale~~ crate for reading various system-level
accessibility and UI preferences across platforms 🐋

The following preferences are supported:
* [`AccentColor`](https://docs.rs/mundy/latest/mundy/struct.AccentColor.html)—The user's current system wide accent color preference.
* [`ColorScheme`](https://docs.rs/mundy/latest/mundy/enum.ColorScheme.html)—The user's preference for either light or dark mode.
* [`Contrast`](https://docs.rs/mundy/latest/mundy/enum.Contrast.html)—The user's preferred contrast level.
* [`ReducedMotion`](https://docs.rs/mundy/latest/mundy/enum.ReducedMotion.html)—The user's reduced motion preference.
* [`ReducedTransparency`](https://docs.rs/mundy/latest/mundy/enum.ReducedTransparency.html)—The user's reduced transparency preference.
* [`DoubleClickInterval`](https://docs.rs/mundy/latest/mundy/struct.DoubleClickInterval.html)—The maximum amount of time allowed between the first and second click.
* [`ScrollbarVisibility`](https://docs.rs/mundy/latest/mundy/struct.ScrollbarVisibility.html)—Whether the scrollbar should be always visible or auto-hide.

## Example
```rust,no_run
use mundy::{Preferences, Interest};
use futures_lite::StreamExt as _;

// Interest tells mundy which preferences it should monitor for you.
// use `Interest::All` if you're interested in all preferences.
let mut stream = Preferences::stream(Interest::AccentColor);

async {
    while let Some(preferences) = stream.next().await {
        eprintln!("accent color: {:?}", preferences.accent_color);
    }
};
```

## [Docs](https://docs.rs/mundy)

## [Examples](./examples/readme.md)

## License
Licensed under the Apache License, Version 2.0 ([license.txt](license.txt) or <http://www.apache.org/licenses/LICENSE-2.0>)

## Contribution
Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
licensed as above, without any additional terms or conditions.
