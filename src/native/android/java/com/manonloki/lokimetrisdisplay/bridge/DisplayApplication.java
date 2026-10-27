package com.manonloki.lokimetrisdisplay.bridge;

import android.app.Application;
import android.content.Context;

/** Owns the process context before the official NSH view starts the app. */
public final class DisplayApplication extends Application {
    @Override
    public void onCreate() {
        super.onCreate();
        System.loadLibrary("nsh");
        nativeInit(this);
    }

    private static native void nativeInit(Context application);
}
