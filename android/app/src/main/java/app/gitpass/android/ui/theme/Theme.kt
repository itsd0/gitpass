package app.gitpass.android.ui.theme

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable

private val DarkColorScheme = darkColorScheme(
    primary = GitpassAccent,
    secondary = GitpassAccent,
    background = GitpassBackground,
    surface = GitpassSurface,
    onPrimary = GitpassBackground,
    onBackground = GitpassOnBackground,
    onSurface = GitpassOnSurface,
)

@Composable
fun GitpassTheme(
    content: @Composable () -> Unit
) {
    MaterialTheme(
        colorScheme = DarkColorScheme,
        content = content
    )
}
