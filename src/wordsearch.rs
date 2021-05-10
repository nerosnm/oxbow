use std::iter::repeat;

lazy_static::lazy_static! {
    static ref ALL: Vec<String> = {
        include_str!("../assets/words.txt").split_whitespace().map(ToOwned::to_owned).collect()
    };
}

pub struct WordSearch {
    possible: Vec<String>,
}

impl WordSearch {
    pub fn new() -> Self {
        let mut ws = Self { possible: vec![] };
        ws.reset();
        ws
    }

    pub fn reset(&mut self) {
        self.possible = ALL[..].to_vec();
    }

    pub fn guess(&self) -> &str {
        let mid = self.possible.len() / 2;
        &self.possible[mid]
    }

    pub fn set_lower(&mut self, word: &str, distance: usize) {
        if let Some(index) = self.possible.iter().position(|ea| *ea == word) {
            let sliced = self.possible[(index + 1)..].to_vec();

            self.possible = sliced
                .into_iter()
                .filter(|ea| Self::hamming_distance(word, ea) == distance)
                .collect();
        }
    }

    pub fn set_upper(&mut self, word: &str, distance: usize) {
        if let Some(index) = self.possible.iter().position(|ea| *ea == word) {
            let sliced = self.possible[..index].to_vec();

            self.possible = sliced
                .into_iter()
                .filter(|ea| Self::hamming_distance(word, ea) == distance)
                .collect();
        }
    }

    pub fn hamming_distance(word1: &str, word2: &str) -> usize {
        // w1 is always the longer word.
        let (w1, w2) = if word2.len() > word1.len() {
            (word2, word1)
        } else {
            (word1, word2)
        };
        // Generate the correct amount of spaces to 'pad' the shorter string.
        let append_spaces = repeat(" ").take(w1.len() - w2.len()).collect::<String>();
        // Push spaces to the shorter string.
        let w2_padded = w2.to_owned() + &append_spaces;
        // Calculating the Hamming distance
        w1.chars()
            .zip(w2_padded.chars())
            .filter(|(x, y)| x != y)
            .count()
    }
}
