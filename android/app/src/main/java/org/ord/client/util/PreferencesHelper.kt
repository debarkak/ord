package org.ord.client.util

import android.content.Context
import android.content.SharedPreferences

class PreferencesHelper(context: Context) {
    private val prefs: SharedPreferences = context.getSharedPreferences("ord_prefs", Context.MODE_PRIVATE)

    var preferredWidth: Int
        get() = prefs.getInt("pref_width", 0) // 0 = Native
        set(value) = prefs.edit().putInt("pref_width", value).apply()

    var preferredHeight: Int
        get() = prefs.getInt("pref_height", 0) // 0 = Native
        set(value) = prefs.edit().putInt("pref_height", value).apply()

    var keepScreenOn: Boolean
        get() = prefs.getBoolean("pref_keep_screen_on", true)
        set(value) = prefs.edit().putBoolean("pref_keep_screen_on", value).apply()

    var showHud: Boolean
        get() = prefs.getBoolean("pref_show_hud", false)
        set(value) = prefs.edit().putBoolean("pref_show_hud", value).apply()

    var lastHostIp: String
        get() = prefs.getString("pref_last_ip", "") ?: ""
        set(value) = prefs.edit().putString("pref_last_ip", value).apply()
}
