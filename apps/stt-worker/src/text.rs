//! Transcript/prompt policy shared by CTranslate2 and whisper.cpp workers.
use voicepaste_core::settings::{Language, TranscriptionOptions};
pub fn initial_prompt(options: &TranscriptionOptions) -> String {
    let custom = options.custom_initial_prompt.trim();
    let bilingual = "English and Ukrainian. Transcribe in these languages. Англійська та українська мови. ґ, є, і, ї.";
    let combined = if options.language_mode == Language::Bilingual {
        if custom.is_empty() {
            bilingual.to_owned()
        } else {
            format!("{bilingual} {custom}")
        }
    } else {
        custom.to_owned()
    };
    if combined.is_empty() {
        combined
    } else {
        format!(" {combined}")
    }
}
pub fn ukrainian_cleanup(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            'ы' => 'и',
            'Ы' => 'И',
            'э' | 'ё' => 'е',
            'Э' | 'Ё' => 'Е',
            'ъ' | 'Ъ' => '\'',
            c => c,
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prompt_and_cleanup_match_both_worker_backends() {
        let mut options =
            TranscriptionOptions::from(&voicepaste_core::settings::Settings::initial().unwrap());
        options.custom_initial_prompt = " Example vocabulary\r\nПриклад словника \r\n".into();
        assert_eq!(
            initial_prompt(&options),
            " Example vocabulary\r\nПриклад словника"
        );
        assert_eq!(ukrainian_cleanup("ыЫэЭёЁъЪ"), "иИеЕеЕ''");
        options.language_mode = Language::Bilingual;
        assert!(initial_prompt(&options).contains("English and Ukrainian."));
    }
}
