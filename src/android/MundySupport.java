package garden.tau.mundy;

import static android.content.Context.UI_MODE_SERVICE;

import android.app.UiModeManager.ContrastChangeListener;
import android.app.UiModeManager;
import android.content.BroadcastReceiver;
import android.content.ContentResolver;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.content.res.Configuration;
import android.content.res.TypedArray;
import android.database.ContentObserver;
import android.os.Build;
import android.os.Handler;
import android.os.PowerManager;
import android.provider.Settings;
import android.util.Log;
import android.view.ContextThemeWrapper;
import java.lang.Override;
import java.util.concurrent.Executor;

/**
 * Support class enabling the Rust crate mundy to access functionality which is not available in the NDK.
 * This class will be instantiated automatically via JNI and should not be used directly.
 */
public class MundySupport {
    private static final String TAG = "MundySupport";
    private ContentResolver contentResolver;
    private Context context;

    private ContrastChangeListener contrastChangeListener;
    private ContentObserver contentObserver;
    private BroadcastReceiver powerSaveModeReceiver;

    /**
     * Invoked only via JNI.
     * @param context Activity or other Context that can be used to get system services
     */
    public MundySupport(Context context) {
        this.context = context;
        this.contentResolver = context.getContentResolver();
    }

    public void subscribe() {
        try {
            trySubscribe();
        }
        catch (Exception e) {
            Log.w(TAG, "failed to set up subscription", e);
            throw e;
        }
    }

    private void trySubscribe() {
        Log.v(TAG, "subscribe called");

        final boolean notifyForDescendants = false;
        contentObserver = new ContentObserver(MundyBackgroundThread.getHandler()) {
            @Override
            public void onChange(boolean selfChange) {
                super.onChange(selfChange);
                onPreferencesChanged();
            }

            @Override
            public boolean deliverSelfNotifications() {
                return true;
            }
        };
        contentResolver.registerContentObserver(Settings.Secure.getUriFor("high_text_contrast_enabled"), notifyForDescendants, contentObserver);
        contentResolver.registerContentObserver(Settings.Global.getUriFor(Settings.Global.ANIMATOR_DURATION_SCALE), notifyForDescendants, contentObserver);

        // Registered with the application context, which outlives an activity that is re-created:
        // <https://developer.android.com/develop/background-work/background-tasks/broadcasts#context-registered-receivers>
        // A system broadcast, so the receiver needs no export flag on Android 14 and later:
        // <https://developer.android.com/about/versions/14/behavior-changes-14#runtime-receivers-exported>
        powerSaveModeReceiver = new BroadcastReceiver() {
            @Override
            public void onReceive(Context context, Intent intent) {
                onPreferencesChanged();
            }
        };
        final IntentFilter powerSaveModeFilter = new IntentFilter(PowerManager.ACTION_POWER_SAVE_MODE_CHANGED);
        final String broadcastPermission = null;
        context.getApplicationContext().registerReceiver(powerSaveModeReceiver, powerSaveModeFilter, broadcastPermission, MundyBackgroundThread.getHandler());

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            final UiModeManager uiModeManager = (UiModeManager) context.getSystemService(UI_MODE_SERVICE);
            contrastChangeListener = new ContrastChangeListener() {
                public void onContrastChanged(final float contrast) {
                    onPreferencesChanged();
                }
            };
            uiModeManager.addContrastChangeListener(context.getMainExecutor(), contrastChangeListener);
        }
    }

    public void unsubscribe() {
        Log.v(TAG, "unsubscribe called");

        contentResolver.unregisterContentObserver(contentObserver);
        context.getApplicationContext().unregisterReceiver(powerSaveModeReceiver);

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            final UiModeManager uiModeManager = (UiModeManager) context.getSystemService(UI_MODE_SERVICE);
            uiModeManager.removeContrastChangeListener(contrastChangeListener);
        }
    }

    public boolean getNightMode() {
        final int uiMode = context.getResources().getConfiguration().uiMode;
        return (uiMode & Configuration.UI_MODE_NIGHT_MASK) == Configuration.UI_MODE_NIGHT_YES;
    }

    // This is how both Firefox & Chromium do it; seems sensible to do it the same.
    // <https://github.com/mozilla-firefox/firefox/blob/ff058e8e75bfdd11a1bdbd1a706c3a4448bce335/mobile/android/geckoview/src/main/java/org/mozilla/gecko/GeckoSystemStateListener.java#L192>
    // <https://source.chromium.org/chromium/chromium/src/+/main:ui/accessibility/android/java/src/org/chromium/ui/accessibility/AccessibilityState.java;l=544;drc=057b542e8c6318874cb4ae6120a601ffdeac9c26>
    public boolean getHighContrast() {
        return getHighTextContrastEnabled() || getContrastLevel() == 1f;
    }

    // Both Firefox and Chromium read this preference to derive the prefers-reduced-motion preference:
    // * In Firefox, this is done in [GeckoSystemStateListener.java](https://github.com/mozilla-firefox/firefox/blob/ff058e8e75bfdd11a1bdbd1a706c3a4448bce335/mobile/android/geckoview/src/main/java/org/mozilla/gecko/GeckoSystemStateListener.java#L163)
    // * In Chromium, this is done in [AccessibilityState.java](https://source.chromium.org/chromium/chromium/src/+/main:ui/accessibility/android/java/src/org/chromium/ui/accessibility/AccessibilityState.java;l=494;drc=8564ceae60d27c21fa575d5fa0a12faae7ab252b)
    //
    // Quoting from the [docs](https://developer.android.com/reference/android/provider/Settings.Global#ANIMATOR_DURATION_SCALE):
    // > Setting to 0.0f will cause animations to end immediately.
    public boolean getPrefersReducedMotion() {
        return Settings.Global.getFloat(contentResolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f) == 0f;
    }

    // Again, this approach is shamelessly stolen from [Firefox](https://github.com/mozilla-firefox/firefox/blob/441506211f9c4c806ce0b6d2b17f67e0775d6ef7/mobile/android/geckoview/src/main/java/org/mozilla/gecko/GeckoAppShell.java#L77)
    public int getAccentColor() {
        final ContextThemeWrapper wrapper = new ContextThemeWrapper(context, android.R.style.TextAppearance);
        final TypedArray attributes = wrapper.obtainStyledAttributes(new int[]{ android.R.attr.colorAccent });
        final int index = attributes.getIndex(0);
        final int defaultValue = 0;
        final int color = attributes.getColor(index, defaultValue);
        attributes.recycle();
        return color;
    }

    // <https://developer.android.com/reference/android/os/PowerManager#isPowerSaveMode()>
    public boolean getPowerSaveMode() {
        final PowerManager powerManager = (PowerManager) context.getSystemService(Context.POWER_SERVICE);
        return powerManager.isPowerSaveMode();
    }

    private boolean getHighTextContrastEnabled() {
        return Settings.Secure.getInt(contentResolver, "high_text_contrast_enabled", 0) == 1;
    }

    private float getContrastLevel() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            final UiModeManager uiModeManager = (UiModeManager) context.getSystemService(UI_MODE_SERVICE);
            return uiModeManager.getContrast();
        }
        return 0f;
    }

    private native void onPreferencesChanged();
}
