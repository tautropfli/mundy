//! Follows the power settings that tell whether a power saving mode is on through
//! [`PowerSettingRegisterNotification`], the route of Microsoft's [energy saver sample].
//!
//! `Windows.System.Power.PowerManager.EnergySaverStatus` (WinRT) is not used because it only
//! follows battery saver: since Windows 11 24H2 it reports `Off` while energy saver runs in
//! its standard mode ([WindowsAppSDK#6484]).
//!
//! [`PowerSettingRegisterNotification`]: https://learn.microsoft.com/en-us/windows/win32/api/powersetting/nf-powersetting-powersettingregisternotification
//! [energy saver sample]: https://github.com/microsoft/Windows-classic-samples/tree/main/Samples/PowerSettingRegisterNotification
//! [WindowsAppSDK#6484]: https://github.com/microsoft/WindowsAppSDK/pull/6484

use crate::callback_utils::{CallbackHandle, Callbacks};
use crate::PowerSaver;
use std::error::Error;
use std::ffi::c_void;
use std::mem;
use std::panic::catch_unwind;
use std::ptr;
use std::sync::RwLock;
use windows::core::GUID;
use windows::Win32::Foundation::{ERROR_SUCCESS, HANDLE};
use windows::Win32::System::Power::{
    PowerSettingRegisterNotification, PowerSettingUnregisterNotification,
    DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS, HPOWERNOTIFY, POWERBROADCAST_SETTING,
};
use windows::Win32::System::SystemServices::GUID_POWER_SAVING_STATUS;
use windows::Win32::UI::WindowsAndMessaging::DEVICE_NOTIFY_CALLBACK;

/// Not yet part of the `windows` crate. Value from the [list of power setting GUIDs].
///
/// [list of power setting GUIDs]: https://learn.microsoft.com/en-us/windows/win32/power/power-setting-guids
const GUID_ENERGY_SAVER_STATUS: GUID = GUID::from_u128(0x550e8400_e29b_41d4_a716_446655440000);

// The values of `ENERGY_SAVER_STATUS` from `winnt.h`.
const ENERGY_SAVER_STANDARD: u32 = 1;
const ENERGY_SAVER_HIGH_SAVINGS: u32 = 2;

/// The value of `GUID_POWER_SAVING_STATUS` while battery saver is on.
const BATTERY_SAVER_ON: u32 = 1;

/// A power setting that tells whether a power saving mode is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PowerSetting {
    /// `GUID_ENERGY_SAVER_STATUS`: energy saver, Windows 11 24H2 and later.
    EnergySaver,
    /// `GUID_POWER_SAVING_STATUS`: battery saver, which energy saver replaced.
    BatterySaver,
}

impl PowerSetting {
    const ALL: [PowerSetting; 2] = [PowerSetting::EnergySaver, PowerSetting::BatterySaver];

    fn guid(self) -> GUID {
        match self {
            PowerSetting::EnergySaver => GUID_ENERGY_SAVER_STATUS,
            PowerSetting::BatterySaver => GUID_POWER_SAVING_STATUS,
        }
    }

    fn from_guid(guid: GUID) -> Option<Self> {
        PowerSetting::ALL
            .into_iter()
            .find(|setting| setting.guid() == guid)
    }
}

/// The new value of a power setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PowerSettingChange {
    pub(crate) setting: PowerSetting,
    pub(crate) value: u32,
}

/// The last value that each power setting reported.
///
/// Both settings are followed because either may be the one that the system uses:
/// energy saver replaced battery saver in Windows 11 24H2.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct PowerSaverStatus {
    energy_saver: Option<u32>,
    battery_saver: Option<u32>,
}

impl PowerSaverStatus {
    pub(crate) fn apply(&mut self, change: PowerSettingChange) {
        match change.setting {
            PowerSetting::EnergySaver => self.energy_saver = Some(change.value),
            PowerSetting::BatterySaver => self.battery_saver = Some(change.value),
        }
    }

    pub(crate) fn has_reported(&self, setting: PowerSetting) -> bool {
        match setting {
            PowerSetting::EnergySaver => self.energy_saver.is_some(),
            PowerSetting::BatterySaver => self.battery_saver.is_some(),
        }
    }

    /// Both modes of energy saver count as enabled: Microsoft asks apps to save energy in either
    /// ([`GUID_ENERGY_SAVER_STATUS`](https://learn.microsoft.com/en-us/windows/win32/power/power-setting-guids)).
    pub(crate) fn preference(&self) -> PowerSaver {
        let energy_saver = matches!(
            self.energy_saver,
            Some(ENERGY_SAVER_STANDARD | ENERGY_SAVER_HIGH_SAVINGS)
        );
        let battery_saver = self.battery_saver == Some(BATTERY_SAVER_ON);
        if energy_saver || battery_saver {
            PowerSaver::Enabled
        } else {
            PowerSaver::NoPreference
        }
    }
}

pub(crate) type CallbackFn = Box<dyn Fn(PowerSettingChange) + Send + Sync>;

/// The callbacks of all registrations. A registration finds its callback by the id that it passes
/// as the context, so a notification that is still delivered after the registration
/// ended finds nothing instead of freed memory.
static CALLBACKS: RwLock<Callbacks<CallbackFn>> = RwLock::new(Callbacks::new());

/// Registers `callback` for the changes of each power setting, which Windows also calls
/// with each known setting's current value right after the registration, on a thread of its own.
/// The registrations end when the returned guard is dropped.
pub(crate) fn register_power_setting_notifications(
    callback: CallbackFn,
) -> Result<PowerSettingsGuard, Box<dyn Error>> {
    let callback = register_callback(callback)?;
    // Boxed so that it outlives the registrations, as no documentation says that Windows copies it.
    let parameters = Box::new(DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS {
        Callback: Some(power_setting_callback),
        Context: callback.0.id() as *mut c_void,
    });
    // Energy saver first, see `PowerSettingsGuard::has_initial_values`.
    let energy_saver = register(PowerSetting::EnergySaver, &parameters);
    let battery_saver = register(PowerSetting::BatterySaver, &parameters);
    Ok(PowerSettingsGuard {
        energy_saver,
        battery_saver,
        _parameters: parameters,
        _callback: callback,
    })
}

pub(crate) struct PowerSettingsGuard {
    energy_saver: Option<HPOWERNOTIFY>,
    battery_saver: Option<HPOWERNOTIFY>,
    _parameters: Box<DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS>,
    _callback: CallbackGuard,
}

impl PowerSettingsGuard {
    /// Whether `status` holds the values that Windows reports right after the registration.
    ///
    /// Only battery saver, which every version since Windows 10 knows, is awaited:
    /// Windows accepts a registration for a setting that it does not know and never reports it,
    /// as versions before Windows 11 24H2 do not know energy saver.
    /// Energy saver is registered first, and Windows reported it first, on the same thread;
    /// were it to come later, it would come as a change.
    pub(crate) fn has_initial_values(&self, status: &PowerSaverStatus) -> bool {
        self.battery_saver.is_none() || status.has_reported(PowerSetting::BatterySaver)
    }
}

impl Drop for PowerSettingsGuard {
    fn drop(&mut self) {
        // The fields, among them the callback and the parameters, drop after this.
        for registration in [self.energy_saver.take(), self.battery_saver.take()]
            .into_iter()
            .flatten()
        {
            // SAFETY: The handle came from `PowerSettingRegisterNotification` and is unregistered once.
            _ = unsafe { PowerSettingUnregisterNotification(registration) };
        }
    }
}

fn register(
    setting: PowerSetting,
    parameters: &DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS,
) -> Option<HPOWERNOTIFY> {
    let guid = setting.guid();
    let mut registration = ptr::null_mut();
    // SAFETY:
    // * `DEVICE_NOTIFY_CALLBACK` asks for a pointer to `DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS`,
    //   which outlives the registration (see `PowerSettingsGuard`).
    // * `registration` is a valid out pointer.
    let error = unsafe {
        PowerSettingRegisterNotification(
            &guid,
            DEVICE_NOTIFY_CALLBACK,
            HANDLE(ptr::from_ref(parameters).cast_mut().cast()),
            &mut registration,
        )
    };
    if error == ERROR_SUCCESS {
        Some(HPOWERNOTIFY(registration as isize))
    } else {
        #[cfg(feature = "log")]
        log::debug!("failed to register for power setting {setting:?}: {error:?}");
        None
    }
}

unsafe extern "system" fn power_setting_callback(
    context: *const c_void,
    _notification_type: u32,
    setting: *const c_void,
) -> u32 {
    let id = context as usize;
    // SAFETY: For a power setting notification, `setting` points to a `POWERBROADCAST_SETTING`.
    if let Some(change) = unsafe { read_change(setting.cast()) } {
        _ = catch_unwind(|| {
            if let Ok(callbacks) = CALLBACKS.read() {
                if let Some(callback) = callbacks.get(id) {
                    callback(change);
                }
            }
        });
    }
    ERROR_SUCCESS.0
}

/// Reads the change of a power setting that is followed and holds a `DWORD`, as both settings do.
///
/// # Safety
/// `setting` must point to a `POWERBROADCAST_SETTING` whose `Data` holds `DataLength` bytes.
unsafe fn read_change(setting: *const POWERBROADCAST_SETTING) -> Option<PowerSettingChange> {
    // SAFETY: Guaranteed by the caller.
    let (guid, length) = unsafe { ((*setting).PowerSetting, (*setting).DataLength) };
    let setting_kind = PowerSetting::from_guid(guid)?;
    if usize::try_from(length).ok()? != mem::size_of::<u32>() {
        return None;
    }
    // SAFETY: `Data` holds `DataLength` bytes, which is the size of a `u32`.
    // The struct declares only the first of them and so it is read unaligned through a raw pointer.
    let value = unsafe { ptr::addr_of!((*setting).Data).cast::<u32>().read_unaligned() };
    Some(PowerSettingChange {
        setting: setting_kind,
        value,
    })
}

fn register_callback(callback: CallbackFn) -> Result<CallbackGuard, Box<dyn Error>> {
    let mut callbacks = CALLBACKS.write().expect("lock poisoned");
    Ok(CallbackGuard(callbacks.add(callback)?))
}

struct CallbackGuard(CallbackHandle);

impl Drop for CallbackGuard {
    fn drop(&mut self) {
        if let Ok(mut callbacks) = CALLBACKS.write() {
            callbacks.remove(mem::take(&mut self.0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    #[test]
    fn reports_the_initial_values_right_after_the_registration() {
        let (sender, receiver) = mpsc::channel();
        let guard = register_power_setting_notifications(Box::new(move |change| {
            _ = sender.send(change);
        }))
        .expect("registered the callback");
        assert!(guard.battery_saver.is_some(), "every Windows since 10 knows battery saver");
        // Windows reported within 1 ms in testing; the deadline only keeps a broken system from hanging the test.
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut status = PowerSaverStatus::default();
        while !guard.has_initial_values(&status) {
            let change = receiver
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("reported before the deadline");
            status.apply(change);
        }
        assert!(status.has_reported(PowerSetting::BatterySaver));
        // `has_initial_values` relies on energy saver, where Windows knows it, coming before battery saver.
        // Both came within 0.2 ms in testing, so 50 ms is ample for a late one to show.
        let late = receiver.recv_timeout(Duration::from_millis(50));
        assert!(
            !matches!(
                late,
                Ok(PowerSettingChange {
                    setting: PowerSetting::EnergySaver,
                    ..
                })
            ),
            "energy saver was reported after battery saver"
        );
    }

    /// A `POWERBROADCAST_SETTING` with room for a `DWORD`.
    #[repr(C)]
    struct Notification {
        guid: GUID,
        length: u32,
        data: [u8; 4],
    }

    fn read(guid: GUID, length: u32, value: u32) -> Option<PowerSettingChange> {
        let notification = Notification {
            guid,
            length,
            data: value.to_ne_bytes(),
        };
        unsafe { read_change(ptr::from_ref(&notification).cast()) }
    }

    #[test]
    fn reads_the_followed_settings() {
        assert_eq!(
            Some(PowerSettingChange {
                setting: PowerSetting::EnergySaver,
                value: 2
            }),
            read(GUID_ENERGY_SAVER_STATUS, 4, 2)
        );
        assert_eq!(
            Some(PowerSettingChange {
                setting: PowerSetting::BatterySaver,
                value: 1
            }),
            read(GUID_POWER_SAVING_STATUS, 4, 1)
        );
    }

    #[test]
    fn ignores_other_settings_and_sizes() {
        assert_eq!(None, read(GUID::zeroed(), 4, 1));
        assert_eq!(None, read(GUID_ENERGY_SAVER_STATUS, 1, 1));
        assert_eq!(None, read(GUID_POWER_SAVING_STATUS, 8, 1));
    }

    #[test]
    fn enabled_while_either_saver_is_on() {
        let status = |energy_saver: Option<u32>, battery_saver: Option<u32>| {
            PowerSaverStatus {
                energy_saver,
                battery_saver,
            }
            .preference()
        };
        // Energy saver: 0 off, 1 standard, 2 high savings, 3 unknown. Battery saver: 0 off, 1 on.
        let table = [
            (None, None, PowerSaver::NoPreference),
            (Some(0), Some(0), PowerSaver::NoPreference),
            (Some(1), Some(0), PowerSaver::Enabled),
            (Some(2), Some(0), PowerSaver::Enabled),
            (Some(3), Some(0), PowerSaver::NoPreference),
            (None, Some(1), PowerSaver::Enabled),
            (Some(0), Some(1), PowerSaver::Enabled),
            (Some(1), None, PowerSaver::Enabled),
        ];
        for (energy_saver, battery_saver, expected) in table {
            assert_eq!(
                expected,
                status(energy_saver, battery_saver),
                "energy saver {energy_saver:?}, battery saver {battery_saver:?}"
            );
        }
    }

    #[test]
    fn each_change_keeps_the_other_setting() {
        let mut status = PowerSaverStatus::default();
        assert!(!status.has_reported(PowerSetting::EnergySaver));
        assert!(!status.has_reported(PowerSetting::BatterySaver));
        status.apply(PowerSettingChange {
            setting: PowerSetting::BatterySaver,
            value: 1,
        });
        status.apply(PowerSettingChange {
            setting: PowerSetting::EnergySaver,
            value: 0,
        });
        assert!(status.has_reported(PowerSetting::EnergySaver));
        assert!(status.has_reported(PowerSetting::BatterySaver));
        assert_eq!(PowerSaver::Enabled, status.preference());
    }
}
