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
    }

    private fun protectWindow() {
        window.setFlags(WindowManager.LayoutParams.FLAG_SECURE, WindowManager.LayoutParams.FLAG_SECURE)
    }
}
