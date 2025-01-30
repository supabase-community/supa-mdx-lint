use std::path::PathBuf;

use symspell::{AsciiStringStrategy, SymSpell, Verbosity};

const DICTIONARY_PATH: &str = "src/rules/rule003_spelling/dictionary.txt";

#[derive(Default)]
pub struct SuggestionMatcher {
    dictionary: SymSpell<AsciiStringStrategy>,
}

impl SuggestionMatcher {
    pub fn new(exceptions: &[impl AsRef<str>]) -> Self {
        log::warn!(
            "exceptions: {:?}",
            exceptions.iter().map(|s| s.as_ref()).collect::<Vec<_>>()
        );
        let mut symspell = SymSpell::default();

        let dictionary_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(DICTIONARY_PATH);
        symspell.load_dictionary(dictionary_path.to_str().unwrap(), 0, 1, " ");

        // Symspell dictionaries require a frequency to be associated with each
        // word. Since our exception lists don't have corpus-derived
        // frequencies, we'll just use a dummy value. This is set relatively
        // high since any custom exceptions are likely to be highly relevant.
        let dummy_frequency = 1_000_000_000;
        for exception in exceptions {
            symspell.load_dictionary_line(
                &format!("{}\t{}", exception.as_ref(), dummy_frequency),
                0,
                1,
                "\t",
            );
        }

        Self {
            dictionary: symspell,
        }
    }

    pub fn suggest(&self, word: &str) -> Vec<String> {
        self.dictionary
            .lookup(word, Verbosity::Top, 2)
            .into_iter()
            .map(|s| s.term)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_suggestion_matcher() {
        let words: Vec<String> = vec![];
        let matcher = SuggestionMatcher::new(&words);
        let suggestions = matcher.suggest("heloo");
        assert!(suggestions.contains(&"hello".to_string()));
    }

    #[test]
    fn test_suggestion_matcher_with_custom_words() {
        let words: Vec<String> = vec!["asdfghjkl".to_string()];
        let matcher = SuggestionMatcher::new(&words);
        let suggestions = matcher.suggest("asdfhjk");
        assert!(suggestions.contains(&"asdfghjkl".to_string()));
    }
}
