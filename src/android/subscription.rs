use super::result::Result;
use super::support::{java_vm, MundySupportRef};
use crate::callback_utils::{CallbackHandle, Callbacks};
use std::mem;
use std::sync::RwLock;

pub(crate) type CallbackFn = Box<dyn Fn() + Send + Sync>;

#[derive(Debug)]
pub(crate) struct Subscription {
    handle: CallbackHandle,
}

impl Drop for Subscription {
    fn drop(&mut self) {
        if let Err(_e) = unsubscribe(mem::take(&mut self.handle)) {
            #[cfg(feature = "log")]
            log::warn!("Error while unsubscribing: {_e:#?}");
        }
    }
}

pub(crate) fn subscribe(callback: impl Fn() + Send + Sync + 'static) -> Result<Subscription> {
    let mut callbacks = CALLBACKS.write().expect("lock poisoned");
    if callbacks.is_empty() {
        subscribe_java()?;
    }
    let handle = callbacks.add(Box::new(callback))?;
    Ok(Subscription { handle })
}

fn unsubscribe(handle: CallbackHandle) -> Result<()> {
    let mut callbacks = CALLBACKS.write().expect("lock poisoned");
    callbacks.remove(handle);
    if callbacks.is_empty() {
        unsubscribe_java()?;
    }
    Ok(())
}

pub(crate) fn on_configuration_changed() {
    let Ok(callbacks) = CALLBACKS.read() else {
        return;
    };
    for callback in callbacks.iter() {
        callback();
    }
}

pub(crate) fn on_preferences_changed() {
    on_configuration_changed();
}

static CALLBACKS: RwLock<Callbacks<CallbackFn>> = RwLock::new(Callbacks::new());

fn subscribe_java() -> Result<()> {
    java_vm().attach_current_thread(|env| {
        let support = MundySupportRef::get()?;
        support.subscribe(env)?;
        Ok(())
    })
}

fn unsubscribe_java() -> Result<()> {
    java_vm().attach_current_thread(|env| {
        let support = MundySupportRef::get()?;
        support.unsubscribe(env)?;
        Ok(())
    })
}
