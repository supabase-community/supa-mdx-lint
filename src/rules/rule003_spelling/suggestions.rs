use symspell::{AsciiStringStrategy, SymSpell, Verbosity};

const DICTIONARY_PATH: &str = "./dictionary.txt";

#[derive(Default)]
pub struct SuggestionMatcher {
    dictionary: SymSpell<AsciiStringStrategy>,
}

mod utils {
    pub(super) fn generate_dummy_dictionary_entries(words: &[impl AsRef<str>]) -> String {
        // Symspell dictionaries require a frequency to be associated with each
        // word. Since our exception lists don't have corpus-derived
        // frequencies, we'll just use a dummy value. This is set relatively
        // high since any custom exceptions are likely to be highly relevant.
        let dummy_frequency = 1_000_000_000;

        words
            .iter()
            .map(|word| format!("{} {}", word.as_ref(), dummy_frequency))
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub(super) fn join_with_newline(str1: &str, str2: &str) -> String {
        let str1_trimmed = str1.trim_end_matches('\n');
        let str2_trimmed = str2.trim_start_matches('\n');
        format!("{}\n{}", str1_trimmed, str2_trimmed)
    }
}

impl SuggestionMatcher {
    pub fn new(exceptions: &[impl AsRef<str>]) -> Self {
        let mut symspell = SymSpell::default();
        symspell.load_dictionary(DICTIONARY_PATH, 0, 1, " ");

        // Symspell dictionaries require a frequency to be associated with each
        // word. Since our exception lists don't have corpus-derived
        // frequencies, we'll just use a dummy value. This is set relatively
        // high since any custom exceptions are likely to be highly relevant.
        let dummy_frequency = 1_000_000_000;
        for exception in exceptions {
            symspell.load_dictionary_line(
                &format!("{} {}", exception.as_ref(), dummy_frequency),
                0,
                1,
                " ",
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
