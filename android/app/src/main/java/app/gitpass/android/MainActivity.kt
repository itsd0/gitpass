package app.gitpass.android

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.gitpass.android.ui.theme.*
import app.gitpass.core.generatePassword

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            GitpassTheme {
                GitpassApp()
            }
        }
    }
}

@Composable
fun GitpassApp() {
    var generatedSecret by remember { mutableStateOf<String?>(null) }
    var bridgeStatus by remember { mutableStateOf<String?>(null) }
    var errorMessage by remember { mutableStateOf<String?>(null) }

    fun runSmokeTest() {
        try {
            // Real Kotlin-to-UniFFI native call: generate password of length 16
            val pw = generatePassword(16UL)
            generatedSecret = pw
            bridgeStatus = "OK (len=${pw.length})"
            errorMessage = null
        } catch (e: Throwable) {
            errorMessage = e.message ?: "Native FFI invocation failed"
            bridgeStatus = "Error"
        }
    }

    LaunchedEffect(Unit) {
        runSmokeTest()
    }

    Scaffold(
        modifier = Modifier.fillMaxSize(),
        containerColor = GitpassBackground
    ) { innerPadding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(innerPadding)
                .padding(24.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(20.dp, Alignment.CenterVertically)
        ) {
            Text(
                text = stringResource(id = R.string.app_name),
                fontSize = 32.sp,
                fontWeight = FontWeight.Bold,
                color = GitpassAccent
            )

            Text(
                text = stringResource(id = R.string.app_tagline),
                fontSize = 14.sp,
                color = GitpassSecondary
            )

            Card(
                modifier = Modifier.fillMaxWidth(),
                colors = CardDefaults.cardColors(containerColor = GitpassSurface),
                shape = RoundedCornerShape(12.dp)
            ) {
                Column(
                    modifier = Modifier.padding(16.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp)
                ) {
                    Text(
                        text = stringResource(id = R.string.bridge_status_title),
                        fontWeight = FontWeight.SemiBold,
                        color = GitpassAccent,
                        fontSize = 16.sp
                    )
                    Text(
                        text = bridgeStatus ?: stringResource(id = R.string.bridge_status_generating),
                        color = GitpassOnSurface,
                        fontSize = 14.sp
                    )

                    if (generatedSecret != null) {
                        Spacer(modifier = Modifier.height(8.dp))
                        Text(
                            text = stringResource(id = R.string.smoke_test_result_title),
                            fontSize = 12.sp,
                            color = GitpassSecondary
                        )
                        Text(
                            text = generatedSecret!!,
                            fontFamily = FontFamily.Monospace,
                            fontWeight = FontWeight.Bold,
                            color = GitpassAccent,
                            fontSize = 16.sp
                        )
                    }

                    if (errorMessage != null) {
                        Spacer(modifier = Modifier.height(8.dp))
                        Text(
                            text = errorMessage!!,
                            color = MaterialTheme.colorScheme.error,
                            fontSize = 12.sp
                        )
                    }
                }
            }

            Button(
                onClick = { runSmokeTest() },
                colors = ButtonDefaults.buttonColors(
                    containerColor = GitpassAccent,
                    contentColor = GitpassBackground
                ),
                shape = RoundedCornerShape(8.dp)
            ) {
                Text(
                    text = stringResource(id = R.string.smoke_test_button),
                    fontWeight = FontWeight.Bold
                )
            }
        }
    }
}
