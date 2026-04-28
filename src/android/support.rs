use super::result::ArcError;
use super::subscription;
use jni::objects::JString;
use jni::refs::{Global, LoaderContext};
use jni::{bind_java_type, Env, JavaVM};
use std::path::PathBuf;
use std::sync::{Arc, LazyLock};
use std::{fs, ops};

type Result<T, E = BoxedError> = std::result::Result<T, E>;
type BoxedError = Box<dyn std::error::Error + Send + Sync + 'static>;

#[derive(Clone)]
pub(crate) struct MundySupportRef {
    global_ref: Arc<Global<MundySupport<'static>>>,
}

impl ops::Deref for MundySupportRef {
    type Target = MundySupport<'static>;

    fn deref(&self) -> &Self::Target {
        &self.global_ref
    }
}

impl MundySupportRef {
    pub(crate) fn get() -> Result<Self> {
        static INSTANCE: LazyLock<Result<MundySupportRef, ArcError>> =
            LazyLock::new(|| MundySupportRef::from_android_context().map_err(ArcError::from));
        INSTANCE.clone().map_err(Into::into)
    }

    fn from_android_context() -> Result<Self> {
        java_vm().attach_current_thread(|env| {
            let context = android_content_context(env);
            inject_dex_class(env, &context)?;
            let mundy_support = MundySupport::new(env, &context)?;
            let global_ref = Arc::new(env.new_global_ref(mundy_support)?);
            Ok(Self { global_ref })
        })
    }
}

pub(crate) fn java_vm() -> JavaVM {
    let ctx = ndk_context::android_context();
    // SAFETY: ndk_context gives us a valid pointer.
    unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }
}

bind_java_type! {
    pub(crate) MundySupport => garden.tau.mundy.MundySupport,
    type_map = {
        AndroidContext => android.content.Context
    },
    constructors {
        fn new(context: AndroidContext),
    },
    methods {
        pub(crate) fn get_night_mode() -> bool,
        pub(crate) fn get_high_contrast() -> bool,
        pub(crate) fn get_prefers_reduced_motion() -> bool,
        pub(crate) fn get_accent_color() -> i32,
        pub(crate) fn subscribe(),
        pub(crate) fn unsubscribe(),
    },
    native_methods {
        extern fn on_preferences_changed(),
    },
}

impl MundySupportNativeInterface for MundySupportAPI {
    type Error = jni::errors::Error;

    fn on_preferences_changed<'local>(
        _env: &mut ::jni::Env<'local>,
        _this: MundySupport<'local>,
    ) -> ::std::result::Result<(), Self::Error> {
        subscription::on_preferences_changed();
        Ok(())
    }
}

fn android_content_context<'env>(env: &mut Env<'env>) -> AndroidContext<'env> {
    let ctx = ndk_context::android_context();
    // SAFETY: ndk_context gives us a valid pointer.
    unsafe { AndroidContext::from_raw(env, ctx.context().cast()) }
}

// This is again adapted from netwatcher's source:
// <https://github.com/thombles/netwatcher/blob/f1353ba6b9a9e4e28a223a317564a3b34a649aae/src/watch_android.rs#L94>
fn inject_dex_class(env: &mut Env<'_>, context: &AndroidContext<'_>) -> Result<()> {
    static COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    if COUNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst) >= 1 {
        panic!("Who dare call me");
    }

    const MUNDY_DEX_BYTES: &[u8] = include_bytes!(env!("MUNDY_DEX_PATH"));

    // to enable backwards compat to API level 21, write to disk instead of loading in-memory
    let cache_dir = context.get_code_cache_dir(env)?;
    let cache_dir_path_jstring = cache_dir.get_absolute_path(env)?;
    let cache_dir_path: String = cache_dir_path_jstring.try_to_string(env)?;
    let temp_dex_path = PathBuf::from(cache_dir_path).join("mundy.dex");
    // delete the file if cleanup failed in previous run
    _ = fs::remove_file(&temp_dex_path);
    fs::write(&temp_dex_path, MUNDY_DEX_BYTES)?;

    // dex file must not be writable or it won't be loaded
    let mut perms = fs::metadata(&temp_dex_path)?.permissions();
    perms.set_readonly(true);
    fs::set_permissions(&temp_dex_path, perms)?;

    let parent_loader = context.get_class_loader(env)?;
    let temp_dex_path_str = temp_dex_path.to_string_lossy();
    let temp_dex_path_jstring = env.new_string(&temp_dex_path_str)?;
    let dex_loader = DexClassLoader::new(
        env,
        temp_dex_path_jstring,
        cache_dir_path_jstring,
        JString::null(),
        parent_loader,
    )?;
    let _mundy_support_api =
        MundySupportAPI::get(env, &LoaderContext::Loader(&dex_loader.as_class_loader()))?;

    _ = fs::remove_file(&temp_dex_path);

    Ok(())
}

bind_java_type! {
    AndroidContext => android.content.Context,
    type_map = {
        JavaFile => java.io.File,
    },
    methods {
        fn get_code_cache_dir() -> JavaFile,
        fn get_class_loader() -> JClassLoader
    }
}

bind_java_type! {
    DexClassLoader => dalvik.system.DexClassLoader,
    constructors {
        fn new(dex_path: JString, optimized_directory: JString, library_search_path: JString, parent: JClassLoader),
    },
    is_instance_of = {
        class_loader: JClassLoader,
    },
}

bind_java_type! {
    JavaFile => java.io.File,
    methods {
        fn get_absolute_path() -> JString
    }
}
