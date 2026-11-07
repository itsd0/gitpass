package app.gitpass.android

import android.os.Bundle
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import app.gitpass.android.ui.GitpassRoot
import app.gitpass.android.ui.theme.GitpassTheme

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        protectWindow()
        val app = application as GitpassApplication
        setContent {
            GitpassTheme {
                GitpassRoot(app)
            }
        }
    }

    override fun onResume() {
        super.onResume()
        protectWindow()
        // Main-thread delays do not advance in deep sleep; check the
        // monotonic deadline as soon as the app is visible again.
        (application as GitpassApplication).session.enforceInactivity()
    }

    private fun protectWindow() {
        window.setFlags(WindowManager.LayoutParams.FLAG_SECURE, WindowManager.LayoutParams.FLAG_SECURE)
    }
}
