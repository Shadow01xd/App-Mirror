package com.tyu.app.preview

import androidx.compose.runtime.Composable
import androidx.compose.ui.tooling.preview.Preview
import com.tyu.app.mock.MockSessions
import com.tyu.app.feature.sessions.SessionsScreen
import com.tyu.app.feature.sessions.SessionsUiState
import com.tyu.app.ui.theme.TyuTheme

@Preview(name = "Actividad · servicios activos", showBackground = true, widthDp = 393, heightDp = 852)
@Composable
fun SessionsPreview() {
    TyuTheme { SessionsScreen(SessionsUiState(MockSessions.example), {}, {}) }
}
