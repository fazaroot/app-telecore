//! Template variables v1.1: `{{first_name}}`, `{{username}}`,
//! `{{chat_title}}`. Variabel tak dikenal dibiarkan apa adanya.
//! Nilai yang tidak diketahui (None) diganti string kosong.
//!
//! Ini berbeda dari `automation::render_template` yang mengganti SEMUA
//! `{{field}}` dari konteks JSON — fungsi ini sengaja dibatasi ke tiga
//! variabel penerima yang aman dipakai di pesan terkirim.

use crate::state::AppState;

#[derive(Debug, Clone, Default)]
pub struct TemplateVars {
    pub first_name: Option<String>,
    pub username: Option<String>,
    pub chat_title: Option<String>,
}

impl TemplateVars {
    pub fn new(
        first_name: Option<String>,
        username: Option<String>,
        chat_title: Option<String>,
    ) -> Self {
        Self {
            first_name,
            username,
            chat_title,
        }
    }
}

/// Ganti placeholder template. Pure — diuji unit.
pub fn render_template_vars(text: &str, vars: &TemplateVars) -> String {
    let mut out = text.to_string();
    out = out.replace("{{first_name}}", vars.first_name.as_deref().unwrap_or(""));
    out = out.replace("{{username}}", vars.username.as_deref().unwrap_or(""));
    out = out.replace("{{chat_title}}", vars.chat_title.as_deref().unwrap_or(""));
    out
}

/// Ambil data penerima dari DB bila tersedia:
/// - user: `bot_users` (bot_id + telegram_user_id = chat_id) -> first_name, username.
/// - chat: `bot_chats` (bot_id + chat_id) -> title.
/// Chat grup (chat_id negatif) umumnya hanya punya title; user tidak ditemukan
/// -> first_name/username kosong.
pub async fn lookup_template_vars(
    state: &AppState,
    bot_id: &str,
    chat_id: i64,
) -> TemplateVars {
    let user: Option<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT first_name, username FROM bot_users WHERE bot_id = ? AND telegram_user_id = ?",
    )
    .bind(bot_id)
    .bind(chat_id)
    .fetch_optional(&state.pool)
    .await
    .ok()
    .flatten();
    let chat: Option<(Option<String>,)> =
        sqlx::query_as("SELECT title FROM bot_chats WHERE bot_id = ? AND chat_id = ?")
            .bind(bot_id)
            .bind(chat_id)
            .fetch_optional(&state.pool)
            .await
            .ok()
            .flatten();
    TemplateVars {
        first_name: user.as_ref().and_then(|(f, _)| f.clone()),
        username: user.as_ref().and_then(|(_, u)| u.clone()),
        chat_title: chat.and_then(|(t,)| t),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_all_known_variables() {
        let vars = TemplateVars::new(
            Some("Astr".to_string()),
            Some("astr".to_string()),
            Some("Grup Keluarga".to_string()),
        );
        assert_eq!(
            render_template_vars(
                "Halo {{first_name}} (@{{username}}) di {{chat_title}}!",
                &vars
            ),
            "Halo Astr (@astr) di Grup Keluarga!"
        );
    }

    #[test]
    fn unknown_variables_are_left_as_is() {
        let vars = TemplateVars::new(Some("Astr".to_string()), None, None);
        assert_eq!(
            render_template_vars("Halo {{first_name}}, {{unknown}} tetap {{other_var}}", &vars),
            "Halo Astr, {{unknown}} tetap {{other_var}}"
        );
    }

    #[test]
    fn text_without_variables_is_unchanged() {
        let vars = TemplateVars::new(Some("Astr".to_string()), None, None);
        assert_eq!(
            render_template_vars("Halo dunia, tanpa variabel.", &vars),
            "Halo dunia, tanpa variabel."
        );
    }

    #[test]
    fn missing_values_become_empty_string() {
        let vars = TemplateVars::default();
        assert_eq!(
            render_template_vars("Halo {{first_name}} ({{username}})", &vars),
            "Halo  ()"
        );
    }

    #[test]
    fn repeated_variables_are_all_replaced() {
        let vars = TemplateVars::new(Some("Bo".to_string()), None, None);
        assert_eq!(
            render_template_vars("{{first_name}} dan {{first_name}}", &vars),
            "Bo dan Bo"
        );
    }
}
