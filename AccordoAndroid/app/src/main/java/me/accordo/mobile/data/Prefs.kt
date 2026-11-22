// SPDX-License-Identifier: GPL-3.0-or-later
// Plain preferences of this phone — nothing secret (secrets go through the core's Keystore store).
// The desktop keeps the same things in its state.json.

package me.accordo.mobile.data

import android.content.Context
import android.content.SharedPreferences

object Prefs {
    private var prefs: SharedPreferences? = null

    /** Once, in MainActivity, before the first screen reads anything. */
    fun init(context: Context) {
        prefs = context.applicationContext.getSharedPreferences("accordo", Context.MODE_PRIVATE)
    }

    /** The network picked in the side menu (the desktop's `active_chain_id`). */
    var chainId: String?
        get() = prefs?.getString("chain_id", null)
        set(value) {
            prefs?.edit()?.putString("chain_id", value)?.apply()
        }
}
